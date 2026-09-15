//! Workspace path safety: resolve + validate that a path stays inside the
//! workspace root — or, for explicit user deliverables, under common home
//! export folders (Desktop / Documents / Downloads).

use std::path::{Path, PathBuf};

use hermes_core::{Error, Result};

/// Resolve `user_path` relative to `workspace`, then verify it stays
/// inside the workspace boundary. Returns the canonical absolute path
/// (or, for a not-yet-created file, the path under the canonical workspace).
pub fn resolve(workspace: &Path, user_path: &str) -> Result<PathBuf> {
    let expanded = expand_user_path(user_path);
    // Secrets are refused by *name* before any allowlist or existence
    // reasoning: a credential file must never come back through a tool,
    // whatever form the path was written in (absolute / `~/` / symlink).
    if expanded.is_absolute() {
        deny_secret(&normalize_path(&expanded), "read")?;
    }

    if expanded.is_absolute() && path_under_any(&normalize_path(&expanded), &user_export_roots()) {
        let candidate = normalize_path(&expanded);
        if candidate.exists() {
            let p = dunce_canonicalize(&candidate)?;
            deny_secret(&p, "read")?;
            return Ok(p);
        }
        return Ok(candidate);
    }

    let ws_canon = dunce_canonicalize(workspace)?;

    let candidate = if expanded.is_absolute() {
        normalize_path(&expanded)
    } else {
        normalize_path(&ws_canon.join(user_path))
    };

    if candidate.exists() {
        let p = dunce_canonicalize(&candidate)?;
        deny_secret(&p, "read")?;
        return if is_under(&p, &ws_canon) {
            Ok(p)
        } else {
            Err(escape_err(user_path, &p, &ws_canon))
        };
    }

    if !is_under(&candidate, &ws_canon) {
        return Err(escape_err(user_path, &candidate, &ws_canon));
    }

    // Not-yet-created: keep the path under the canonical workspace so macOS
    // `/var` vs `/private/var` cannot look like an escape.
    Ok(candidate)
}

fn escape_err(user_path: &str, resolved: &Path, workspace: &Path) -> Error {
    let hint = if user_path.contains("memories") || user_path.contains("memory") {
        " Use memory_save for durable memories (not write/edit)."
    } else {
        ""
    };
    Error::ToolHost(format!(
        "path escapes workspace: {} resolves to {} which is outside {}{hint}",
        user_path,
        resolved.display(),
        workspace.display()
    ))
}

/// Prefix check that treats macOS `/var` and `/private/var` as the same root.
fn is_under(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    let p = strip_macos_private(path);
    let r = strip_macos_private(root);
    p.starts_with(&r)
}

fn strip_macos_private(p: &Path) -> PathBuf {
    match p.strip_prefix("/private") {
        Ok(rest) => Path::new("/").join(rest),
        Err(_) => p.to_path_buf(),
    }
}

/// Like `std::fs::canonicalize` but doesn't fail on macOS `/tmp` → `/private/tmp`.
fn dunce_canonicalize(p: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(p)
        .map_err(|e| Error::ToolHost(format!("cannot canonicalize {}: {e}", p.display())))
}

/// Normalize `..` and `.` components without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Join workspace + user_path and normalize (public for write.rs fallback).
pub fn normalize_join(workspace: &Path, user_path: &str) -> PathBuf {
    normalize_path(&workspace.join(user_path))
}

/// Expand `~/`, `～/`, `$HOME/`, and a leading export folder
/// (`Desktop/`, `Documents/`, `Downloads/`, `桌面/`).
pub fn expand_user_path(user_path: &str) -> PathBuf {
    let p = user_path.trim();
    let p = p
        .strip_prefix('～')
        .map(|r| format!("~{r}"))
        .unwrap_or_else(|| p.to_string());
    if let Some(rest) = p.strip_prefix("$HOME/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if p == "$HOME" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if p == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(home) = dirs::home_dir() {
        const EXPORT: &[&str] = &["Desktop", "Documents", "Downloads", "桌面", "文稿", "下载"];
        for name in EXPORT {
            let prefix = format!("{name}/");
            if let Some(rest) = p.strip_prefix(&prefix) {
                return home.join(name).join(rest);
            }
            if p == *name {
                return home.join(name);
            }
        }
    }
    PathBuf::from(p)
}

/// Folders the user commonly wants deliverables written to (outside workspace).
pub fn user_export_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for name in ["Desktop", "Documents", "Downloads", "桌面", "文稿", "下载"] {
            roots.push(home.join(name));
        }
    }
    if let Some(d) = dirs::desktop_dir() {
        roots.push(d);
    }
    if let Some(d) = dirs::document_dir() {
        roots.push(d);
    }
    if let Some(d) = dirs::download_dir() {
        roots.push(d);
    }
    // Dedupe while preserving order
    let mut out = Vec::new();
    for r in roots {
        if !out.iter().any(|e: &PathBuf| e == &r) {
            out.push(r);
        }
    }
    out
}

fn path_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let root_ok = if root.exists() {
            dunce_canonicalize(root).ok()
        } else {
            Some(normalize_path(root))
        };
        let Some(root_c) = root_ok else {
            return false;
        };
        if path.starts_with(&root_c) {
            return true;
        }
        // Non-existing file: check ancestor
        let mut anc = path.to_path_buf();
        while !anc.exists() {
            match anc.parent() {
                Some(p) if p != anc => anc = p.to_path_buf(),
                _ => break,
            }
        }
        dunce_canonicalize(&anc)
            .map(|c| c.starts_with(&root_c))
            .unwrap_or(false)
    })
}

/// Resolve a path for write/edit: `~/` and export folders first, else workspace.
/// Returns `(absolute_path, is_export_outside_workspace)`.
///
/// `~/Desktop/x` must never be treated as workspace-relative `~/Desktop/x`.
pub fn resolve_for_write(workspace: &Path, user_path: &str) -> Result<(PathBuf, bool)> {
    let expanded = expand_user_path(user_path);
    let normalized = normalize_path(&expanded);
    deny_secret(&normalized, "write")?;
    if expanded.is_absolute() {
        let roots = user_export_roots();
        if path_under_any(&normalized, &roots) {
            return Ok((normalized, true));
        }
        // Absolute path inside the workspace (or a real escape) — same rules as read.
        return resolve(workspace, &expanded.to_string_lossy()).map(|p| (p, false));
    }
    resolve(workspace, user_path).map(|p| (p, false))
}

/// Resolve a local path the user asked to **open** (must already exist).
///
/// Opening is not writing: any existing file under the workspace or the
/// user's home is allowed (videos in Movies, a doc they named). System
/// paths (`/etc`, `/System`, …) and well-known secret locations stay closed.
pub fn resolve_for_open(workspace: &Path, user_path: &str) -> Result<PathBuf> {
    let expanded = expand_user_path(user_path);
    let normalized = normalize_path(&expanded);
    let candidate = if normalized.is_absolute() {
        normalized
    } else {
        normalize_join(workspace, user_path)
    };

    // Before "does it exist": a secret must not be distinguishable by error
    // message either, and an absolute path under an export root would
    // otherwise be allowed.
    deny_secret(&candidate, "read")?;

    if !candidate.exists() {
        return Err(Error::ToolHost(format!(
            "nothing to open: {} does not exist",
            candidate.display()
        )));
    }

    let path = dunce_canonicalize(&candidate)?;
    deny_secret(&path, "read")?;

    let ws = dunce_canonicalize(workspace)?;
    if path.starts_with(&ws) {
        return Ok(path);
    }

    if let Some(home) = dirs::home_dir() {
        let home = dunce_canonicalize(&home).unwrap_or(home);
        if path.starts_with(&home) {
            return Ok(path);
        }
    }

    if path_under_any(&path, &user_export_roots()) {
        return Ok(path);
    }

    Err(Error::ToolHost(format!(
        "path not allowed to open: {} (workspace or your home only)",
        path.display()
    )))
}

/// Block ordinary bash from reading keys / product secrets (all OS).
///
/// This is the early, human-readable gate in front of the OS sandbox. A shell
/// string cannot be resolved reliably, so a command that touches a secret is
/// refused **as a whole** (fail-closed) — but the reason names the file.
pub fn bash_secret_read_blocked(command: &str) -> Option<String> {
    let c = command.replace('\\', "/");
    let lower = c.to_ascii_lowercase();
    if lower.contains(".ssh/")
        || lower.contains("/.ssh")
        || lower.contains("id_rsa")
        || lower.contains("id_ed25519")
    {
        return Some("refusing to read SSH keys".into());
    }
    if lower.contains(".gnupg/") {
        return Some("refusing to read GPG material".into());
    }
    if lower.contains(".aws/") || lower.contains(".kube/") || lower.contains(".netrc") {
        return Some("refusing to read credential files".into());
    }
    if lower.contains("server.token") {
        return Some("refusing to read the server token".into());
    }

    // Product credentials: the command mentions a known data root *and* a
    // secret file name. Root strings include the `~/` spelling, because the
    // shell gets the literal text the model wrote.
    for root in product_root_strings() {
        if !lower.contains(&root) {
            continue;
        }
        if let Some(name) = PRODUCT_SECRET_FILES.iter().find(|n| lower.contains(**n)) {
            return Some(format!(
                "refusing to read a product secret file: {root}/{name}"
            ));
        }
    }
    None
}

/// Absolute and `~/`-spelled forms of every known data root, lowercased.
fn product_root_strings() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let roots = product_data_roots();
    for root in &roots {
        let s = root
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let home = home
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        for root in &roots {
            let s = root
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            if let Some(rest) = s.strip_prefix(&home) {
                let tilde = format!("~{rest}");
                if !out.contains(&tilde) {
                    out.push(tilde);
                }
            }
        }
    }
    out
}

/// Credential-bearing file names that live directly under a product data root.
///
/// **One list.** The file tools (`read` / `grep` / `open` / `write` / `edit`),
/// the bash pre-filter and the macOS sandbox all derive from it: a secret only
/// one of them knows about is not protected.
const PRODUCT_SECRET_FILES: &[&str] = &[
    "config.toml", // LLM provider keys
    "wechat.toml", // WeChat bot credentials
    "feishu.toml", // Feishu app credentials
    "telegram.toml",
    "mcp.json",     // MCP servers (usually bearer tokens)
    "server.token", // REST / WS bearer token
];

/// Legacy data roots older installs left behind. Users move their data root
/// (this machine's live one is `~/Documents/codeINDEx/test`), so knowing only
/// the current root leaves yesterday's keys readable — and knowing only the
/// legacy roots misses today's.
const LEGACY_DATA_DIRS: &[&str] = &[".lebi-ai", ".lebi-law", ".lebi-superlawyer", ".lebi"];

/// Every known product data root (the live one first). Lexical only — the
/// files may not exist, and the checks must not depend on that.
fn product_data_roots() -> Vec<PathBuf> {
    let mut roots = vec![normalize_path(&hermes_core::data_root())];
    if let Some(home) = dirs::home_dir() {
        for name in LEGACY_DATA_DIRS {
            let p = normalize_path(&home.join(name));
            if !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    roots
}

/// Every product secret file across every known root.
pub fn product_secret_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in product_data_roots() {
        for name in PRODUCT_SECRET_FILES {
            let p = normalize_path(&root.join(name));
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn is_product_secret_path_in(path: &Path, roots: &[PathBuf]) -> bool {
    // Accept any spelling of the same file: lexical (`/var/...`) and
    // canonical (`/private/var/...`), which macOS hands back differently.
    let mut forms = vec![normalize_path(path)];
    if path.exists() {
        if let Ok(c) = dunce_canonicalize(path) {
            forms.push(c);
        }
    }
    roots.iter().any(|root| {
        PRODUCT_SECRET_FILES.iter().any(|name| {
            let target = normalize_path(&root.join(name));
            forms.iter().any(|f| same_file_path(f, &target))
        })
    })
}

fn same_file_path(a: &Path, b: &Path) -> bool {
    a == b || strip_macos_private(a) == strip_macos_private(b)
}

/// True when `path` names one of this product's credential files, under any
/// known data root. A file with the same name elsewhere is *not* a secret.
pub fn is_product_secret_path(path: &Path) -> bool {
    is_product_secret_path_in(path, &product_data_roots())
}

/// Well-known OS credential stores (keys, cloud CLIs). Owned by the OS, not by
/// this product, but the sandbox must deny them too.
fn os_secret_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".ssh"),
        home.join(".gnupg"),
        home.join(".aws"),
        home.join(".kube"),
        home.join(".netrc"),
    ]
}

/// Everything the product must never hand to a model or a tool. The macOS
/// sandbox denies exactly this list, so it cannot drift from the tool gates.
pub fn secret_paths() -> Vec<PathBuf> {
    let mut out = product_secret_paths();
    for p in os_secret_paths() {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn is_os_secret_path(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    const MARKERS: &[&str] = &[
        "/.ssh/",
        "/.gnupg/",
        "/.aws/",
        "/.kube/",
        "/.netrc",
        "server.token",
        "/id_rsa",
        "/id_ed25519",
        "/.ssh",
    ];
    MARKERS.iter().any(|m| {
        if *m == "/.ssh" {
            s.ends_with("/.ssh")
        } else {
            s.contains(m)
        }
    })
}

/// True when `path` is any secret the product refuses to touch.
pub fn is_secret_path(path: &Path) -> bool {
    is_os_secret_path(path) || is_product_secret_path(path)
}

fn deny_secret(path: &Path, action: &str) -> Result<()> {
    if !is_secret_path(path) {
        return Ok(());
    }
    Err(Error::ToolHost(format!(
        "refusing to {action} a secret file: {} \
         (API keys and channel credentials stay out of tools — the user edits them in Settings)",
        path.display()
    )))
}

/// True when `user_path` clearly targets outside the workspace (absolute or ~/).
pub fn path_looks_outside_workspace(user_path: &str) -> bool {
    let p = user_path.trim();
    if p.starts_with("~/")
        || p == "~"
        || p.starts_with('～')
        || p.starts_with("$HOME")
        || Path::new(p).is_absolute()
    {
        return true;
    }
    [
        "Desktop/",
        "Documents/",
        "Downloads/",
        "桌面/",
        "文稿/",
        "下载/",
    ]
    .iter()
    .any(|n| p.starts_with(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_roots_include_desktop_name() {
        let roots = user_export_roots();
        assert!(!roots.is_empty());
        // At least one path ends with Desktop / 桌面 / Documents-like folder.
        let ok = roots.iter().any(|r| {
            let s = r.to_string_lossy();
            s.contains("Desktop")
                || s.contains("Documents")
                || s.contains("Downloads")
                || s.contains("桌面")
                || s.contains("文稿")
                || s.contains("下载")
        });
        assert!(ok, "roots={roots:?}");
    }

    #[test]
    fn path_looks_outside() {
        assert!(path_looks_outside_workspace("~/Desktop/a.docx"));
        assert!(path_looks_outside_workspace("/tmp/x"));
        assert!(!path_looks_outside_workspace("outputs/a.md"));
    }

    #[test]
    fn tilde_desktop_is_export_not_workspace_folder() {
        let ws = tempfile::tempdir().unwrap();
        let (path, is_export) =
            resolve_for_write(ws.path(), "~/Desktop/lebi-export-test.docx").unwrap();
        assert!(is_export, "path={}", path.display());
        let s = path.to_string_lossy();
        assert!(
            s.contains("Desktop") || s.contains("桌面"),
            "expected Desktop export, got {s}"
        );
        assert!(
            !path.starts_with(ws.path()),
            "must not write workspace/~/Desktop: {}",
            path.display()
        );
    }

    #[test]
    fn bare_desktop_folder_is_home_desktop() {
        let ws = tempfile::tempdir().unwrap();
        let (path, is_export) = resolve_for_write(ws.path(), "Desktop/lebi-bare.docx").unwrap();
        assert!(is_export, "path={}", path.display());
        assert!(
            !path.starts_with(ws.path()),
            "Desktop/ must not land in workspace: {}",
            path.display()
        );
    }

    #[test]
    fn relative_path_resolves_inside() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("a.txt"), b"").unwrap();
        let p = resolve(ws.path(), "a.txt").unwrap();
        assert!(p.starts_with(std::fs::canonicalize(ws.path()).unwrap()));
    }

    #[test]
    fn new_file_under_missing_subdir_stays_inside() {
        let ws = tempfile::tempdir().unwrap();
        let p = resolve(ws.path(), "outputs/brief.docx").unwrap();
        let ws_c = std::fs::canonicalize(ws.path()).unwrap();
        assert!(
            p.starts_with(&ws_c) || is_under(&p, &ws_c),
            "new file {} must stay under {}",
            p.display(),
            ws_c.display()
        );
        assert!(p.ends_with("outputs/brief.docx"));
    }

    #[test]
    fn dotdot_escape_rejected() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve(ws.path(), "../../../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("escapes workspace"));
    }

    #[test]
    fn absolute_inside_workspace_ok() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("b.txt"), b"").unwrap();
        let abs = ws.path().join("b.txt").to_string_lossy().to_string();
        let p = resolve(ws.path(), &abs).unwrap();
        assert!(p.exists());
    }

    #[test]
    fn absolute_outside_workspace_rejected() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve(ws.path(), "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("escapes workspace"));
    }

    #[test]
    fn open_workspace_file_ok() {
        let ws = tempfile::tempdir().unwrap();
        let f = ws.path().join("clip.mp4");
        std::fs::write(&f, b"not-a-real-video").unwrap();
        let p = resolve_for_open(ws.path(), "clip.mp4").unwrap();
        assert_eq!(p, std::fs::canonicalize(&f).unwrap());
    }

    #[test]
    fn open_missing_errors() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve_for_open(ws.path(), "nope.docx").unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn open_etc_passwd_rejected() {
        let ws = tempfile::tempdir().unwrap();
        if !Path::new("/etc/passwd").exists() {
            return;
        }
        let err = resolve_for_open(ws.path(), "/etc/passwd").unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("not allowed") || s.contains("secret") || s.contains("escapes"),
            "{s}"
        );
    }

    #[test]
    fn bash_blocks_secret_reads() {
        assert!(bash_secret_read_blocked("cat ~/.ssh/id_rsa").is_some());
        assert!(bash_secret_read_blocked("cat ~/.lebi-ai/config.toml").is_some());
        assert!(bash_secret_read_blocked("cat ~/.lebi-ai/server.token").is_some());
        assert!(bash_secret_read_blocked("ls outputs").is_none());
        assert!(bash_secret_read_blocked("cat workspace/config.toml").is_none());
    }

    /// The files a product data root holds credentials in.
    fn secret_names() -> [&'static str; 6] {
        [
            "config.toml",
            "wechat.toml",
            "feishu.toml",
            "telegram.toml",
            "mcp.json",
            "server.token",
        ]
    }

    fn known_roots() -> Vec<PathBuf> {
        let mut roots = vec![hermes_core::data_root()];
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join(".lebi-ai"));
            roots.push(home.join(".lebi-law"));
        }
        roots
    }

    /// The bug of 2026-09-13: this machine's data root lives under
    /// `~/Documents`, which `resolve` treats as an export root — so `read`
    /// returned the API key file.
    #[test]
    fn current_data_root_secrets_are_refused_by_read() {
        let ws = tempfile::tempdir().unwrap();
        for name in secret_names() {
            let p = hermes_core::data_root().join(name);
            let err = resolve(ws.path(), &p.to_string_lossy())
                .expect_err(&format!("{p:?} must not be readable"));
            assert!(err.to_string().contains("secret"), "{name}: {err}");
        }
    }

    #[test]
    fn legacy_data_root_secrets_are_refused_by_read() {
        let ws = tempfile::tempdir().unwrap();
        let home = dirs::home_dir().expect("home dir");
        let p = home.join(".lebi-ai").join("config.toml");
        let err = resolve(ws.path(), &p.to_string_lossy()).unwrap_err();
        assert!(err.to_string().contains("secret"), "{err}");
    }

    #[test]
    fn secrets_are_refused_for_write_and_open_before_existence() {
        let ws = tempfile::tempdir().unwrap();
        let p = hermes_core::data_root().join("config.toml");
        let s = p.to_string_lossy().to_string();

        let err = resolve_for_write(ws.path(), &s).unwrap_err();
        assert!(err.to_string().contains("secret"), "{err}");

        // Must be the secret rule, not "does not exist" — order matters.
        let err = resolve_for_open(ws.path(), &s).unwrap_err();
        assert!(err.to_string().contains("secret"), "{err}");
    }

    #[test]
    fn every_known_secret_name_is_covered_in_every_root() {
        for root in known_roots() {
            for name in secret_names() {
                let p = root.join(name);
                assert!(is_product_secret_path(&p), "{}", p.display());
                assert!(is_secret_path(&p), "{}", p.display());
            }
        }
    }

    #[test]
    fn same_name_outside_a_data_root_is_not_a_secret() {
        let ws = tempfile::tempdir().unwrap();
        assert!(!is_product_secret_path(&ws.path().join("config.toml")));
        assert!(!is_secret_path(&ws.path().join("mcp.json")));

        // A deliverable that happens to be called config.toml still works.
        std::fs::write(ws.path().join("config.toml"), b"# mine\n").unwrap();
        let p = resolve(ws.path(), "config.toml").unwrap();
        assert!(p.ends_with("config.toml"));
    }

    #[test]
    fn canonicalised_alias_of_a_secret_is_still_a_secret() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("config.toml"), b"key").unwrap();
        let alias = elsewhere.path().join("notes.toml");
        std::os::unix::fs::symlink(root.path().join("config.toml"), &alias).unwrap();

        let roots = vec![root.path().to_path_buf()];
        assert!(is_product_secret_path_in(&alias, &roots));
        assert!(is_product_secret_path_in(
            &std::fs::canonicalize(&alias).unwrap(),
            &roots
        ));
        assert!(!is_product_secret_path_in(
            &elsewhere.path().join("config.toml"),
            &roots
        ));
    }

    #[test]
    fn bash_blocks_current_data_root_secrets_with_a_named_reason() {
        let live = hermes_core::data_root().join("config.toml");
        let reason = bash_secret_read_blocked(&format!("cat {}", live.display()))
            .expect("live data root must be covered");
        assert!(reason.contains("config.toml"), "{reason}");

        let legacy = dirs::home_dir().unwrap().join(".lebi-ai/config.toml");
        assert!(bash_secret_read_blocked(&format!("cat {}", legacy.display())).is_some());
    }

    #[test]
    fn secret_paths_cover_current_and_legacy_roots() {
        let all = secret_paths();
        for root in known_roots() {
            for name in secret_names() {
                assert!(all.contains(&root.join(name)), "{}/{name}", root.display());
            }
        }
        // OS credential locations stay in the same list (used by the sandbox).
        if let Some(home) = dirs::home_dir() {
            assert!(all.contains(&home.join(".ssh")));
        }
    }
}
