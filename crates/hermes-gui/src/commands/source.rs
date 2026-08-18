use serde::Serialize;
use tauri::State;

use hermes_sources::{IngestOutcome, SourceItem, SourceStoreError};

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceListItem {
    pub id: String,
    pub title: String,
    pub original_name: String,
    pub ext: String,
    pub created_at: String,
    pub readable: bool,
    pub chars: usize,
    #[serde(default)]
    pub original_missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<SourceListItem>>,
}

impl From<SourceItem> for SourceListItem {
    fn from(s: SourceItem) -> Self {
        Self {
            id: s.id,
            title: s.title,
            original_name: s.original_name,
            ext: s.ext,
            created_at: s.created_at,
            readable: s.readable,
            chars: s.chars,
            original_missing: s.original_missing,
            previous: None,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepSourceRequest {
    pub file_name: String,
    pub bytes_base64: String,
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub md_rel_path: Option<String>,
}

#[tauri::command]
pub fn keep_source(
    state: State<'_, AppState>,
    request: KeepSourceRequest,
) -> Result<IngestOutcome, GuiError> {
    let bytes = hermes_tools::decode_bytes_base64(&request.bytes_base64)
        .map_err(|e| GuiError::Tool(e.to_string()))?;
    let ext = std::path::Path::new(&request.file_name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let mut body = request.body_md;
    if body.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        if let Some(rel) = request.md_rel_path.as_deref() {
            let p = std::path::PathBuf::from(state.workspace_root()).join(rel);
            body = std::fs::read_to_string(p).ok();
        }
    }
    state
        .source_store
        .ingest(&request.file_name, &bytes, body.as_deref(), &ext)
        .map_err(map_err)
}

#[tauri::command]
pub fn preview_source(state: State<'_, AppState>, id: String) -> Result<String, GuiError> {
    state
        .source_store
        .preview(id.trim(), 800)
        .ok_or_else(|| GuiError::NotFound("no readable text".into()))
}

#[tauri::command]
pub fn list_sources(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<SourceListItem>, GuiError> {
    let q = query.unwrap_or_default();
    Ok(state
        .source_store
        .list_matching(&q)
        .into_iter()
        .map(|(item, prev)| {
            let mut row = SourceListItem::from(item);
            row.previous = prev.map(|p| Box::new(SourceListItem::from(p)));
            row
        })
        .collect())
}

#[tauri::command]
pub fn delete_source(state: State<'_, AppState>, id: String) -> Result<(), GuiError> {
    let id = id.trim().to_string();
    state.source_store.delete(&id).map_err(map_err)?;
    clear_focus(&state, &id);
    Ok(())
}

/// Undo auto-keep / new version: remove this id and restore the previous file.
#[tauri::command]
pub fn undo_source(state: State<'_, AppState>, id: String) -> Result<(), GuiError> {
    let id = id.trim().to_string();
    state.source_store.undo_keep(&id).map_err(map_err)?;
    clear_focus(&state, &id);
    Ok(())
}

fn clear_focus(state: &State<'_, AppState>, id: &str) {
    if let Ok(mut map) = state.source_focus.try_lock() {
        for ids in map.values_mut() {
            ids.retain(|x| x != id);
        }
    }
}

#[tauri::command]
pub fn open_source(state: State<'_, AppState>, id: String) -> Result<(), GuiError> {
    let path = state
        .source_store
        .original_path(id.trim())
        .ok_or_else(|| GuiError::NotFound("material original missing".into()))?;
    open_path(&path).map_err(GuiError::Internal)
}

pub fn ingest_auto_keep(
    store: &hermes_sources::SourceStore,
    file_name: &str,
    bytes: &[u8],
    body_md: Option<&str>,
    ext: &str,
) -> Result<Option<IngestOutcome>, SourceStoreError> {
    if !hermes_sources::is_auto_keep_ext(ext) {
        return Ok(None);
    }
    match store.ingest(file_name, bytes, body_md, ext) {
        Ok(o) => Ok(Some(o)),
        Err(e) => Err(e),
    }
}

pub fn map_keep_err(e: SourceStoreError) -> GuiError {
    map_err(e)
}

fn map_err(e: SourceStoreError) -> GuiError {
    match e {
        SourceStoreError::NotFound => GuiError::NotFound(e.to_string()),
        SourceStoreError::Quota => GuiError::Tool("材料太多了，先丢掉不用的再加。".into()),
        other => GuiError::Internal(other.to_string()),
    }
}

fn open_path(path: &std::path::Path) -> std::result::Result<(), String> {
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(path).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(path).status()
    };
    match r {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("open exited {s}")),
        Err(e) => Err(e.to_string()),
    }
}
