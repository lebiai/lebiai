//! Document import: convert Word/PDF/Excel/… to Markdown in the workspace.
//!
//! Engine capability (not a skill). Shared by GUI and hermes-server.
//! See `docs/records/20260803-document-import-compliant.md`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
const MIN_MD_CHARS: usize = 1;
const CONVERSION_TIMEOUT_SECS: u64 = 120;
const ENV_MARKITDOWN: &str = "HERMES_MARKITDOWN";

/// Build the command that runs the MarkItDown converter for the current
/// platform. On Windows the bundled sidecar is a `.cmd` wrapper (self-contained
/// embeddable Python), which must go through `cmd /C call`; macOS/Linux run
/// the bash wrapper directly.
fn markitdown_command(binary: &Path) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg("call").arg(binary);
        c
    } else {
        Command::new(binary)
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Where to look for the bundled markitdown binary (fixed order, no PATH luck).
#[derive(Debug, Clone, Default)]
pub struct ConverterPathConfig {
    /// Optional app Resources path (Tauri / install dir). Checked 2nd.
    pub bundled_binary: Option<PathBuf>,
    /// Data-dir sidecar, default `{data_root}/bin/markitdown`. Checked 3rd.
    pub data_bin: Option<PathBuf>,
}

impl ConverterPathConfig {
    /// Default: data root `bin/markitdown`; no bundled path unless set by host.
    pub fn default_for_product() -> Self {
        let data_bin = hermes_core::data_root().join("bin").join("markitdown");
        Self {
            bundled_binary: None,
            data_bin: Some(data_bin),
        }
    }

    pub fn with_bundled(mut self, path: impl Into<PathBuf>) -> Self {
        self.bundled_binary = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConverterStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportRequest {
    pub session_id: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub delete_original: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub ok: bool,
    pub file_id: String,
    pub md_rel_path: String,
    pub display_name: String,
    pub original_name: String,
    pub source_ext: String,
    pub kind: String,
    pub chars: usize,
    pub bytes_md: usize,
    pub original_deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("import_document: {code}: {message}")]
    Coded { code: &'static str, message: String },
}

impl ImportError {
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self::Coded {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Coded { code, .. } => code,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadMeta {
    version: u32,
    file_id: String,
    original_name: String,
    source_ext: String,
    source_mime: String,
    kind: String,
    md_rel_path: String,
    chars: usize,
    bytes_md: usize,
    converted_at: String,
    converter: String,
    converter_version: Option<String>,
    original_deleted: bool,
    warning: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn check_converter(cfg: &ConverterPathConfig) -> ConverterStatus {
    resolve_converter(cfg)
}

/// Import raw file bytes → Markdown under `workspace/uploads/{session}/`.
pub fn import_document(
    workspace: &Path,
    cfg: &ConverterPathConfig,
    request: ImportRequest,
) -> Result<ImportResult, ImportError> {
    let session_id = sanitize_session_id(&request.session_id)?;
    let original_name = request.file_name.trim();
    if original_name.is_empty() {
        return Err(ImportError::coded("invalid_session", "fileName is empty"));
    }
    let base_name = Path::new(original_name)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ImportError::coded("unsupported_type", "invalid fileName"))?
        .to_string();

    let ext = extension_of(&base_name)
        .ok_or_else(|| ImportError::coded("unsupported_type", "missing file extension"))?;
    let kind = kind_for_ext(&ext).ok_or_else(|| {
        ImportError::coded("unsupported_type", format!("unsupported extension .{ext}"))
    })?;

    if request.bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ImportError::coded(
            "too_large",
            format!(
                "file is {} bytes; max is {} bytes",
                request.bytes.len(),
                MAX_UPLOAD_BYTES
            ),
        ));
    }
    if request.bytes.is_empty() {
        return Err(ImportError::coded("empty_markdown", "file is empty"));
    }

    // pdf/docx/xlsx/csv → markitdown. txt/md → passthrough. doc (legacy) → OS tools.
    let needs_markitdown = matches!(ext.as_str(), "pdf" | "docx" | "xlsx" | "csv");
    let status = if needs_markitdown {
        let s = resolve_converter(cfg);
        if !s.available {
            return Err(ImportError::coded(
                "markitdown_missing",
                s.error.unwrap_or_else(|| {
                    "document converter not found; run scripts/setup-markitdown-sidecar.sh".into()
                }),
            ));
        }
        Some(s)
    } else {
        None
    };

    let file_id = short_file_id();
    let safe_stem = safe_stem_from_name(&base_name);
    let mime = request
        .mime_type
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| default_mime(&ext).to_string());

    let uploads_dir = workspace.join("uploads").join(&session_id);
    std::fs::create_dir_all(&uploads_dir)
        .map_err(|e| ImportError::coded("io_error", format!("create uploads dir: {e}")))?;

    let tmp_dir = workspace
        .join(".upload_tmp")
        .join(&session_id)
        .join(&file_id);
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| ImportError::coded("io_error", format!("create tmp dir: {e}")))?;

    let tmp_src = tmp_dir.join(format!("{file_id}_{safe_stem}.{ext}"));
    let md_name = format!("{file_id}_{safe_stem}.md");
    let meta_name = format!("{file_id}_{safe_stem}.meta.json");
    let md_abs = uploads_dir.join(&md_name);
    let meta_abs = uploads_dir.join(&meta_name);
    let md_rel = format!("uploads/{session_id}/{md_name}");

    let cleanup_tmp = |keep_md: bool| {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        if !keep_md {
            let _ = std::fs::remove_file(&md_abs);
            let _ = std::fs::remove_file(&meta_abs);
        }
    };

    if let Err(e) = std::fs::write(&tmp_src, &request.bytes) {
        cleanup_tmp(false);
        return Err(ImportError::coded("io_error", format!("write temp: {e}")));
    }

    let body = if matches!(ext.as_str(), "txt" | "md") {
        decode_plain_bytes(&request.bytes)
    } else if ext == "doc" {
        // Legacy Word binary — MarkItDown offline only handles .docx.
        match extract_legacy_doc_text(&tmp_src) {
            Ok(s) => s,
            Err(e) => {
                cleanup_tmp(false);
                return Err(e);
            }
        }
    } else {
        let bin = status
            .as_ref()
            .and_then(|s| s.binary_path.as_ref())
            .cloned()
            .ok_or_else(|| {
                ImportError::coded("markitdown_missing", "converter path missing after check")
            })?;
        match run_markitdown_blocking(&bin, &tmp_src, &md_abs) {
            Ok(s) => s,
            Err(e) => {
                cleanup_tmp(false);
                return Err(e);
            }
        }
    };

    let chars = body.chars().count();
    if chars < MIN_MD_CHARS {
        cleanup_tmp(false);
        return Err(ImportError::coded(
            "empty_markdown",
            "could not extract usable text (scanned PDF, encrypted, or empty)",
        ));
    }

    let converted_by = if matches!(ext.as_str(), "txt" | "md") {
        "passthrough"
    } else if ext == "doc" {
        "legacy_doc"
    } else {
        "markitdown"
    };
    let frontmatter = format!(
        "---\noriginal_name: {}\nsource_ext: {}\nconverted_by: {}\n---\n\n",
        yaml_escape_plain(&base_name),
        ext,
        converted_by
    );
    let final_md = if body.trim_start().starts_with("---") {
        body
    } else {
        format!("{frontmatter}{body}")
    };
    let final_chars = final_md.chars().count();
    let bytes_md = final_md.len();

    if let Err(e) = std::fs::write(&md_abs, final_md.as_bytes()) {
        cleanup_tmp(false);
        return Err(ImportError::coded("io_error", format!("write md: {e}")));
    }

    let original_deleted = if request.delete_original {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        true
    } else {
        false
    };

    let converter_version = status.as_ref().and_then(|s| s.version.clone());
    let converter_name = if matches!(ext.as_str(), "txt" | "md") {
        "passthrough"
    } else if ext == "doc" {
        "legacy_doc"
    } else {
        "markitdown"
    };
    let meta = UploadMeta {
        version: 1,
        file_id: file_id.clone(),
        original_name: base_name.clone(),
        source_ext: ext.clone(),
        source_mime: mime,
        kind: kind.to_string(),
        md_rel_path: md_rel.clone(),
        chars: final_chars,
        bytes_md,
        converted_at: Utc::now().to_rfc3339(),
        converter: converter_name.into(),
        converter_version,
        original_deleted,
        warning: None,
    };

    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(&meta_abs, json);
    }
    if request.delete_original {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    Ok(ImportResult {
        ok: true,
        file_id,
        md_rel_path: md_rel,
        display_name: format!("{safe_stem}.md"),
        original_name: base_name,
        source_ext: ext,
        kind: kind.to_string(),
        chars: final_chars,
        bytes_md,
        original_deleted,
        warning: None,
    })
}

/// Decode standard base64 into bytes for IPC/HTTP callers.
pub fn decode_bytes_base64(b64: &str) -> Result<Vec<u8>, ImportError> {
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| ImportError::coded("io_error", format!("base64 decode: {e}")))
}

// ---------------------------------------------------------------------------
// Converter resolution (no system PATH as default success)
// ---------------------------------------------------------------------------

fn resolve_converter(cfg: &ConverterPathConfig) -> ConverterStatus {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(p) = std::env::var_os(ENV_MARKITDOWN) {
        candidates.push(PathBuf::from(p));
    }
    if let Some(p) = &cfg.bundled_binary {
        candidates.push(p.clone());
    }
    if let Some(p) = &cfg.data_bin {
        candidates.push(p.clone());
    }

    if candidates.is_empty() {
        return ConverterStatus {
            available: false,
            binary_path: None,
            version: None,
            error: Some(
                "no converter path configured; set HERMES_MARKITDOWN or run setup-markitdown-sidecar.sh"
                    .into(),
            ),
        };
    }

    let mut last_err: Option<String> = None;
    for cand in candidates {
        if !cand.exists() {
            last_err = Some(format!("not found: {}", cand.display()));
            continue;
        }
        match markitdown_command(&cand).arg("--version").output() {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let version = if version.is_empty() {
                    String::from_utf8_lossy(&out.stderr)
                        .lines()
                        .next()
                        .unwrap_or("markitdown")
                        .trim()
                        .to_string()
                } else {
                    version
                };
                return ConverterStatus {
                    available: true,
                    binary_path: Some(cand.display().to_string()),
                    version: Some(version),
                    error: None,
                };
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                last_err = Some(format!(
                    "{} --version failed: {}",
                    cand.display(),
                    truncate(err.trim(), 200)
                ));
            }
            Err(e) => {
                last_err = Some(format!("cannot execute {}: {e}", cand.display()));
            }
        }
    }

    ConverterStatus {
        available: false,
        binary_path: None,
        version: None,
        error: Some(last_err.unwrap_or_else(|| {
            "document converter unavailable; run scripts/setup-markitdown-sidecar.sh".into()
        })),
    }
}

fn run_markitdown_blocking(
    binary: &str,
    src: &Path,
    dest_md: &Path,
) -> Result<String, ImportError> {
    let _ = std::fs::remove_file(dest_md);
    let binary = binary.to_string();
    let src = src.to_path_buf();
    let dest = dest_md.to_path_buf();

    let handle = std::thread::spawn(move || {
        markitdown_command(Path::new(&binary))
            .arg(&src)
            .arg("-o")
            .arg(&dest)
            .output()
    });

    let timeout = Duration::from_secs(CONVERSION_TIMEOUT_SECS);
    let start = std::time::Instant::now();
    while !handle.is_finished() {
        if start.elapsed() > timeout {
            return Err(ImportError::coded(
                "conversion_failed",
                format!("markitdown timed out after {CONVERSION_TIMEOUT_SECS}s"),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let output = handle
        .join()
        .map_err(|_| ImportError::coded("conversion_failed", "markitdown thread panicked"))?
        .map_err(|e| {
            ImportError::coded(
                "markitdown_missing",
                format!("failed to run markitdown: {e}"),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        return Err(ImportError::coded(
            "conversion_failed",
            format!(
                "markitdown exit {}: {}",
                output.status,
                truncate(detail, 500)
            ),
        ));
    }

    match std::fs::read_to_string(dest_md) {
        Ok(body) => Ok(body),
        Err(e) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            if !stdout.trim().is_empty() {
                Ok(stdout)
            } else {
                Err(ImportError::coded(
                    "io_error",
                    format!("read md after convert: {e}"),
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitize_session_id(raw: &str) -> Result<String, ImportError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ImportError::coded("invalid_session", "sessionId is empty"));
    }
    if s.contains('/') || s.contains('\\') || s.contains("..") {
        return Err(ImportError::coded(
            "invalid_session",
            "sessionId must not contain path separators",
        ));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ImportError::coded(
            "invalid_session",
            "sessionId has invalid characters",
        ));
    }
    Ok(s.to_string())
}

fn extension_of(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

fn kind_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "pdf" | "docx" | "doc" => Some("document"),
        "xlsx" => Some("spreadsheet"),
        "csv" | "txt" | "md" => Some("text"),
        _ => None,
    }
}

fn default_mime(ext: &str) -> &'static str {
    match ext {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "md" => "text/markdown",
        _ => "application/octet-stream",
    }
}

/// Extract text from legacy `.doc`.
///
/// Chinese court / export tooling often writes **HTML wrapped in an OLE `.doc`
/// shell** (CDFv2). textutil/LibreOffice reject those; we detect embedded HTML
/// first. Real binary Word still falls back to OS helpers.
fn extract_legacy_doc_text(src: &Path) -> Result<String, ImportError> {
    let bytes = std::fs::read(src)
        .map_err(|e| ImportError::coded("io_error", format!("read .doc: {e}")))?;

    // 1) HTML-in-OLE / HTML-saved-as-.doc (common for 裁判文书 exports)
    if let Some(text) = extract_html_from_doc_bytes(&bytes) {
        if text.chars().count() >= MIN_MD_CHARS {
            return Ok(text);
        }
    }

    // 2) Dense Chinese already in the binary (GBK/GB18030), no HTML wrapper
    if let Some(text) = extract_gbk_plain_from_doc_bytes(&bytes) {
        if text.chars().count() >= MIN_MD_CHARS {
            return Ok(text);
        }
    }

    // 3) macOS textutil
    if Path::new("/usr/bin/textutil").is_file() {
        if let Ok(output) = Command::new("/usr/bin/textutil")
            .args(["-convert", "txt", "-stdout"])
            .arg(src)
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).into_owned();
                if text.chars().count() >= MIN_MD_CHARS
                    && !text.contains("isn’t in the correct format")
                    && !text.contains("isn't in the correct format")
                {
                    return Ok(text);
                }
            }
        }
    }

    // 4) antiword (skip broken pure-python stubs that crash)
    if let Ok(output) = Command::new("antiword").arg(src).output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            if text.chars().count() >= MIN_MD_CHARS {
                return Ok(text);
            }
        }
    }

    // 5) LibreOffice / soffice
    for bin in ["soffice", "libreoffice"] {
        let parent = src.parent().unwrap_or_else(|| Path::new("."));
        if let Ok(output) = Command::new(bin)
            .args(["--headless", "--convert-to", "txt:Text", "--outdir"])
            .arg(parent)
            .arg(src)
            .output()
        {
            if output.status.success() {
                let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
                let txt_path = parent.join(format!("{stem}.txt"));
                if let Ok(text) = std::fs::read_to_string(&txt_path) {
                    let _ = std::fs::remove_file(&txt_path);
                    if text.chars().count() >= MIN_MD_CHARS {
                        return Ok(text);
                    }
                }
            }
        }
    }

    Err(ImportError::coded(
        "conversion_failed",
        "legacy .doc could not be read (unusual binary or encrypted). Please open in Word and Save As .docx or PDF, then import again.",
    ))
}

/// Many 判决书 `.doc` files are OLE containers whose WordDocument stream is HTML.
fn extract_html_from_doc_bytes(bytes: &[u8]) -> Option<String> {
    let lower: Vec<u8> = bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    let start = lower
        .windows(5)
        .position(|w| w == b"<html")
        .or_else(|| lower.windows(9).position(|w| w == b"<!doctype"))?;
    let slice = &bytes[start..];
    let end_rel = lower[start..]
        .windows(7)
        .rposition(|w| w == b"</html>")
        .map(|i| i + 7);
    let html_bytes = match end_rel {
        Some(e) => &slice[..e.min(slice.len())],
        None => slice,
    };

    let html = decode_doc_bytes(html_bytes);
    if !html.to_ascii_lowercase().contains("<html") && !html.contains('判') {
        // weak signal — still try convert
    }

    // Prefer HTML→Markdown when possible.
    if let Ok(md) = htmd::convert(&html) {
        let t = md.trim().to_string();
        if t.chars().count() >= MIN_MD_CHARS {
            return Some(t);
        }
    }
    let plain = strip_simple_html(&html);
    if plain.chars().count() >= MIN_MD_CHARS {
        Some(plain)
    } else {
        None
    }
}

/// Fallback: pull long Chinese runs via GBK/GB18030 from the whole file.
fn extract_gbk_plain_from_doc_bytes(bytes: &[u8]) -> Option<String> {
    let text = decode_doc_bytes(bytes);
    let mut out = String::new();
    let mut run = String::new();
    for ch in text.chars() {
        if ('\u{4e00}'..='\u{9fff}').contains(&ch)
            || ch.is_ascii_alphanumeric()
            || "，。、；：？！（）【】《》—…·.\n\r\t ，.；:".contains(ch)
            || ch == ' '
        {
            run.push(ch);
        } else if !run.is_empty() {
            if run
                .chars()
                .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
                .count()
                >= 4
            {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(run.trim());
            }
            run.clear();
        }
    }
    if run
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count()
        >= 4
    {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(run.trim());
    }
    let cn = out
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    if cn >= 20 {
        Some(out)
    } else {
        None
    }
}

fn decode_doc_bytes(bytes: &[u8]) -> String {
    // Prefer GBK/GB18030 for Chinese court docs; UTF-8 if clean.
    if let Ok(s) = std::str::from_utf8(bytes) {
        let cn = s
            .chars()
            .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
            .count();
        if cn >= 8 {
            return s.to_string();
        }
    }
    let (cow, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    if !had_errors || cow.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
        return cow.into_owned();
    }
    let (cow2, _, _) = encoding_rs::GBK.decode(bytes);
    cow2.into_owned()
}

/// Plain `.txt` / `.md`: keep UTF-8 when clean; otherwise try GB18030
/// so a Windows-saved 记事本 file does not become `���`.
fn decode_plain_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if replacement_ratio(s) < 0.02 {
            return s.to_string();
        }
    }
    let (cow, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    let text = cow.into_owned();
    let cn = text
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    if !had_errors || cn >= 4 {
        return text;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn replacement_ratio(s: &str) -> f64 {
    let n = s.chars().count();
    if n == 0 {
        return 0.0;
    }
    let bad = s.chars().filter(|&c| c == '\u{FFFD}').count();
    bad as f64 / n as f64
}

fn strip_simple_html(html: &str) -> String {
    let mut s = html.to_string();
    // crude but dependency-free fallback when htmd fails
    for (a, b) in [
        ("<br>", "\n"),
        ("<br/>", "\n"),
        ("<br />", "\n"),
        ("</p>", "\n"),
        ("</div>", "\n"),
        ("</tr>", "\n"),
    ] {
        s = s.replace(a, b);
        s = s.replace(&a.to_ascii_uppercase(), b);
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let collapsed = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    collapsed
}

fn short_file_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn safe_stem_from_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let mut out: String = stem
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if out.is_empty() {
        out = "document".into();
    }
    if out.chars().count() > 80 {
        out = out.chars().take(80).collect();
    }
    out
}

fn yaml_escape_plain(s: &str) -> String {
    if s.chars()
        .any(|c| c == ':' || c == '#' || c == '"' || c == '\'')
        || s.contains('\n')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_session_rejects_paths() {
        assert!(sanitize_session_id("../x").is_err());
        assert!(sanitize_session_id("a/b").is_err());
        assert!(sanitize_session_id("").is_err());
        assert_eq!(sanitize_session_id("sess_01").unwrap(), "sess_01");
    }

    #[test]
    fn safe_stem_strips_bad_chars() {
        assert_eq!(safe_stem_from_name("a/b:c.pdf"), "b_c");
        assert_eq!(safe_stem_from_name("合同.docx"), "合同");
    }

    #[test]
    fn kind_whitelist() {
        assert_eq!(kind_for_ext("pdf"), Some("document"));
        assert_eq!(kind_for_ext("xlsx"), Some("spreadsheet"));
        assert_eq!(kind_for_ext("doc"), Some("document"));
        assert_eq!(kind_for_ext("png"), None);
    }

    #[test]
    fn decode_plain_gbk_chinese_not_replacement() {
        // 「你好」 in GBK.
        let gbk = encoding_rs::GBK.encode("你好世界").0;
        let text = decode_plain_bytes(&gbk);
        assert!(text.contains("你好"), "got {text:?}");
        assert!(!text.contains('\u{FFFD}'));
    }

    #[test]
    fn import_txt_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let body = "hello 中文";
        let req = ImportRequest {
            session_id: "testsession1".into(),
            file_name: "note.txt".into(),
            bytes: body.as_bytes().to_vec(),
            mime_type: None,
            delete_original: true,
        };
        let cfg = ConverterPathConfig::default();
        let res = import_document(dir.path(), &cfg, req).expect("import txt");
        assert!(res.ok);
        assert!(res.md_rel_path.ends_with(".md"));
        assert!(res.original_deleted);
        let md = std::fs::read_to_string(dir.path().join(&res.md_rel_path)).unwrap();
        assert!(md.contains("hello 中文"));
        assert!(md.contains("original_name:"));
    }

    #[test]
    fn import_rejects_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let req = ImportRequest {
            session_id: "s1".into(),
            file_name: "x.png".into(),
            bytes: b"fake".to_vec(),
            mime_type: None,
            delete_original: true,
        };
        let err = import_document(dir.path(), &ConverterPathConfig::default(), req).unwrap_err();
        assert_eq!(err.code(), "unsupported_type");
    }

    #[test]
    fn import_rejects_too_large() {
        let dir = tempfile::tempdir().unwrap();
        let req = ImportRequest {
            session_id: "s1".into(),
            file_name: "big.txt".into(),
            bytes: vec![b'a'; MAX_UPLOAD_BYTES + 1],
            mime_type: None,
            delete_original: true,
        };
        let err = import_document(dir.path(), &ConverterPathConfig::default(), req).unwrap_err();
        assert_eq!(err.code(), "too_large");
    }

    #[test]
    fn check_converter_missing_without_paths() {
        // Isolate from HERMES_MARKITDOWN if set.
        let cfg = ConverterPathConfig {
            bundled_binary: Some(PathBuf::from("/nonexistent/markitdown-xyz")),
            data_bin: Some(PathBuf::from("/nonexistent2/markitdown")),
        };
        // Temporarily clear env for this check by only using cfg paths —
        // if HERMES_MARKITDOWN is set and valid, available may be true; skip then.
        if std::env::var_os(ENV_MARKITDOWN).is_some() {
            return;
        }
        let s = check_converter(&cfg);
        assert!(!s.available);
    }

    #[test]
    fn extract_html_in_ole_doc_sample() {
        // Court .doc exports often wrap GBK HTML inside an OLE shell.
        let html_gbk = encoding_rs::GBK
            .encode("黑龙江省鸡西市城子河区人民法院民事判决书 原告包某胜")
            .0
            .into_owned();
        let mut blob = b"xxxxOLEHEADxxxx".to_vec();
        blob.extend_from_slice(b"<html><body>");
        blob.extend_from_slice(&html_gbk);
        blob.extend_from_slice(b"</body></html>");
        let text = extract_html_from_doc_bytes(&blob).expect("html extract");
        assert!(
            text.contains("人民法院") || text.contains("包某胜"),
            "{text}"
        );
    }

    #[test]
    fn import_real_workspace_doc_if_present() {
        let path = hermes_core::data_root()
            .join("workspace/uploads/包某甲包某乙遗嘱继承纠纷一审民事判决书.doc");
        if !path.exists() {
            eprintln!("skip: no sample doc at {}", path.display());
            return;
        }
        let text = extract_legacy_doc_text(&path).expect("extract real doc");
        assert!(
            text.contains("人民法院") || text.contains("包某"),
            "unexpected: {}",
            text.chars().take(200).collect::<String>()
        );
    }

    /// Hermetic e2e of the `data_bin` converter path: a fake `markitdown`
    /// sidecar in a temp dir stands in for the real binary, so the whole
    /// resolution -> conversion -> metadata pipeline runs deterministically
    /// on every machine (no dependence on the user's installed sidecar).
    #[cfg(unix)]
    #[test]
    fn import_csv_via_fake_data_bin_sidecar() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let fake_bin = dir.path().join("markitdown");
        std::fs::write(
            &fake_bin,
            r##"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "--version" ]; then
  echo "fake-markitdown 0.0.0"
  exit 0
fi
src="${1:?}"
dest="${3:?}"
{
  echo "# fake markitdown conversion"
  cat "$src"
} > "$dest"
"##,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_bin, perms).unwrap();

        let cfg = ConverterPathConfig {
            bundled_binary: None,
            data_bin: Some(fake_bin),
        };
        let req = ImportRequest {
            session_id: "e2esession".into(),
            file_name: "费用.csv".into(),
            bytes: b"name,amount\nAlice,10\nBob,20\n".to_vec(),
            mime_type: None,
            delete_original: true,
        };
        let res = import_document(dir.path(), &cfg, req).expect("csv import");
        assert!(res.ok);
        let md = std::fs::read_to_string(dir.path().join(&res.md_rel_path)).unwrap();
        assert!(md.contains("Alice") || md.contains("name"), "{md}");
    }
}
