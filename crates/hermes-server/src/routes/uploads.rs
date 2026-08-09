//! Document upload REST — 1:1 with `hermes-gui` upload commands.
//!
//! - `GET  /api/v1/uploads/converter` → `check_document_converter`
//! - `POST /api/v1/uploads` → `import_document`

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use hermes_tools::{
    check_converter, decode_bytes_base64, import_document, ConverterPathConfig, ConverterStatus,
    ImportError, ImportRequest, ImportResult,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

fn converter_cfg() -> ConverterPathConfig {
    ConverterPathConfig::default_for_product()
}

fn map_err(e: ImportError) -> ApiError {
    ApiError::Tool(e.to_string())
}

pub async fn check_document_converter() -> Json<ConverterStatus> {
    Json(check_converter(&converter_cfg()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDocumentBody {
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

pub async fn import_document_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ImportDocumentBody>,
) -> Result<Json<ImportResult>, ApiError> {
    let bytes = decode_bytes_base64(&body.bytes_base64).map_err(map_err)?;
    let workspace = PathBuf::from(state.workspace_root());
    let result = import_document(
        &workspace,
        &converter_cfg(),
        ImportRequest {
            session_id: body.session_id,
            file_name: body.file_name,
            bytes,
            mime_type: body.mime_type,
            delete_original: body.delete_original,
        },
    )
    .map_err(map_err)?;
    Ok(Json(result))
}
