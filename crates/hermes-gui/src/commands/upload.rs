//! Document import IPC — thin wrapper over `hermes_tools::document_import`.
//!
//! Converter resolution (host injects bundled path):
//! 1. `HERMES_MARKITDOWN` (dev override)
//! 2. App Resources / crate `resources/markitdown-sidecar/markitdown` (release + dev)
//! 3. `~/.lebi-ai/bin/markitdown` (setup script fallback)
//!
//! See `docs/records/20260803-markitdown-release-bundle.md`.

use std::path::PathBuf;

use hermes_sources::IngestOutcome;
use hermes_tools::{
    decode_bytes_base64, import_document as import_document_core, ConverterPathConfig, ImportError,
    ImportRequest, ImportResult,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::source::ingest_auto_keep;
use crate::error::GuiError;
use crate::state::AppState;

/// Build converter config: product data-bin + optional app-bundled sidecar.
fn converter_cfg(app: &AppHandle) -> ConverterPathConfig {
    let mut cfg = ConverterPathConfig::default_for_product();
    if let Some(bundled) = resolve_bundled_markitdown(app) {
        cfg = cfg.with_bundled(bundled);
    }
    cfg
}

/// Prefer Tauri resource dir (release .app); fall back to crate resources (dev).
fn resolve_bundled_markitdown(app: &AppHandle) -> Option<PathBuf> {
    // Windows ships a `.cmd` wrapper (runs the bundled embeddable Python);
    // macOS/Linux ship a bash wrapper named `markitdown`.
    let file_name = if cfg!(windows) {
        "markitdown.cmd"
    } else {
        "markitdown"
    };

    // 1) Packaged app: Resources/markitdown-sidecar/markitdown[.cmd]
    if let Ok(resource_dir) = app.path().resource_dir() {
        for rel in [
            format!("markitdown-sidecar/{file_name}"),
            format!("resources/markitdown-sidecar/{file_name}"),
        ] {
            let p = resource_dir.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 2) Dev / cargo run: next to hermes-gui crate
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/markitdown-sidecar")
        .join(file_name);
    if dev.is_file() {
        return Some(dev);
    }

    None
}

fn map_import_err(e: ImportError) -> GuiError {
    GuiError::Tool(e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDocumentRequest {
    pub session_id: String,
    pub file_name: String,
    pub bytes_base64: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default = "default_true")]
    pub delete_original: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiImportResult {
    #[serde(flatten)]
    pub import: ImportResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<IngestOutcome>,
}

#[tauri::command]
pub async fn import_document(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportDocumentRequest,
) -> Result<GuiImportResult, GuiError> {
    let cfg = converter_cfg(&app);
    let workspace = PathBuf::from(state.workspace_root());
    let store = state.source_store.clone();
    tokio::task::spawn_blocking(move || import_document_blocking(cfg, workspace, store, request))
        .await
        .map_err(|e| GuiError::Internal(e.to_string()))?
}

fn import_document_blocking(
    cfg: ConverterPathConfig,
    workspace: PathBuf,
    store: std::sync::Arc<hermes_sources::SourceStore>,
    request: ImportDocumentRequest,
) -> Result<GuiImportResult, GuiError> {
    let bytes = decode_bytes_base64(&request.bytes_base64).map_err(map_import_err)?;
    let ext = std::path::Path::new(&request.file_name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    match import_document_core(
        &workspace,
        &cfg,
        ImportRequest {
            session_id: request.session_id,
            file_name: request.file_name.clone(),
            bytes: bytes.clone(),
            mime_type: request.mime_type,
            delete_original: request.delete_original,
        },
    ) {
        Ok(import) => {
            let body = workspace.join(&import.md_rel_path);
            let md = std::fs::read_to_string(&body).ok();
            let kept = ingest_auto_keep(
                &store,
                &request.file_name,
                &bytes,
                md.as_deref(),
                &import.source_ext,
            )
            .map_err(crate::commands::source::map_keep_err)?;
            Ok(GuiImportResult { import, kept })
        }
        Err(e) => {
            let kept = ingest_auto_keep(&store, &request.file_name, &bytes, None, &ext)
                .map_err(crate::commands::source::map_keep_err)?;
            if kept.is_some() {
                Ok(GuiImportResult {
                    import: ImportResult {
                        ok: false,
                        file_id: String::new(),
                        md_rel_path: String::new(),
                        display_name: request.file_name.clone(),
                        original_name: request.file_name.clone(),
                        source_ext: ext,
                        kind: "document".into(),
                        chars: 0,
                        bytes_md: 0,
                        original_deleted: false,
                        warning: Some(human_keep_unreadable(&e)),
                    },
                    kept,
                })
            } else {
                Err(map_import_err(e))
            }
        }
    }
}

fn human_keep_unreadable(e: &ImportError) -> String {
    match e.code() {
        "too_large" => "这份太大了，先拆开再丢进来。已留下原件，但还读不出字。".into(),
        "empty_markdown" => "留下了，但还读不出里面的字。可以打开原件。".into(),
        other if other.contains("encrypt") || other.contains("password") => {
            "打不开，是不是加密了？原件已留下。".into()
        }
        _ => "留下了，但还读不出里面的字。可以打开原件。".into(),
    }
}
