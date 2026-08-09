//! Document import IPC — thin wrapper over `hermes_tools::document_import`.
//!
//! Converter resolution (host injects bundled path):
//! 1. `HERMES_MARKITDOWN` (dev override)
//! 2. App Resources / crate `resources/markitdown-sidecar/markitdown` (release + dev)
//! 3. `~/.lebi-ai/bin/markitdown` (setup script fallback)
//!
//! See `docs/records/20260803-markitdown-release-bundle.md`.

use std::path::PathBuf;

use hermes_tools::{
    check_converter, decode_bytes_base64, import_document as import_document_core,
    ConverterPathConfig, ConverterStatus, ImportError, ImportRequest, ImportResult,
};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

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
    // 1) Packaged app: Contents/Resources/markitdown-sidecar/markitdown
    if let Ok(resource_dir) = app.path().resource_dir() {
        for rel in [
            "markitdown-sidecar/markitdown",
            "resources/markitdown-sidecar/markitdown",
        ] {
            let p = resource_dir.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 2) Dev / cargo run: next to hermes-gui crate
    let dev =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/markitdown-sidecar/markitdown");
    if dev.is_file() {
        return Some(dev);
    }

    None
}

fn map_import_err(e: ImportError) -> GuiError {
    GuiError::Tool(e.to_string())
}

#[tauri::command]
pub fn check_document_converter(app: AppHandle) -> ConverterStatus {
    check_converter(&converter_cfg(&app))
}

/// Alias kept so older front-end probes still work.
#[tauri::command]
pub fn check_markitdown(app: AppHandle) -> ConverterStatus {
    check_document_converter(app)
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

#[tauri::command]
pub fn import_document(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportDocumentRequest,
) -> Result<ImportResult, GuiError> {
    let bytes = decode_bytes_base64(&request.bytes_base64).map_err(map_import_err)?;
    let workspace = PathBuf::from(state.workspace_root());
    import_document_core(
        &workspace,
        &converter_cfg(&app),
        ImportRequest {
            session_id: request.session_id,
            file_name: request.file_name,
            bytes,
            mime_type: request.mime_type,
            delete_original: request.delete_original,
        },
    )
    .map_err(map_import_err)
}
