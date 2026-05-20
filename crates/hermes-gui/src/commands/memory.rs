use serde::Serialize;
use tauri::State;

use hermes_memory::{Confidence, MemoryFrontmatter, MemoryStore, Scope, Source};

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub body: String,
    pub scope: String,
    pub pinned: bool,
    pub confidence: String,
    pub tags: Vec<String>,
    pub zone: String,
    pub created_at: String,
    pub source: String,
}

fn to_item(m: &hermes_memory::LoadedMemory) -> MemoryItem {
    MemoryItem {
        id: m.frontmatter.id.clone(),
        body: m.body.clone(),
        scope: format!("{:?}", m.scope),
        pinned: m.frontmatter.pinned,
        confidence: format!("{:?}", m.frontmatter.confidence),
        tags: m.frontmatter.tags.clone(),
        zone: m.frontmatter.zone.clone(),
        created_at: m.frontmatter.created.to_rfc3339(),
        source: format!("{:?}", m.frontmatter.source),
    }
}

#[tauri::command]
pub fn list_memories(state: State<'_, AppState>) -> Result<Vec<MemoryItem>, GuiError> {
    let memories = state.memory_store.list_active().map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(memories.iter().map(to_item).collect())
}

#[tauri::command]
pub fn create_memory(
    state: State<'_, AppState>,
    body: String,
    tags: Vec<String>,
    scope: String,
    zone: Option<String>,
    pinned: bool,
) -> Result<MemoryItem, GuiError> {
    let s = match scope.as_str() {
        "Project" => Scope::Project,
        _ => Scope::User,
    };
    let zone = zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, tags, zone);
    fm.pinned = pinned;
    state
        .memory_store
        .put(s, fm.clone(), &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let item = MemoryItem {
        id: fm.id.clone(),
        body,
        scope: format!("{:?}", s),
        pinned: fm.pinned,
        confidence: format!("{:?}", fm.confidence),
        tags: fm.tags.clone(),
        zone: fm.zone.clone(),
        created_at: fm.created.to_rfc3339(),
        source: "User".into(),
    };
    Ok(item)
}

#[tauri::command]
pub fn delete_memory(
    state: State<'_, AppState>,
    id: String,
    scope: String,
) -> Result<bool, GuiError> {
    let s = match scope.as_str() {
        "Project" => Scope::Project,
        _ => Scope::User,
    };
    state
        .memory_store
        .delete(s, &id)
        .map_err(|e| GuiError::Internal(e.to_string()))
}

#[tauri::command]
pub fn toggle_pin_memory(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<MemoryItem>, GuiError> {
    let mem = state
        .memory_store
        .get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    let Some(mem) = mem else { return Ok(None) };

    let mut fm = mem.frontmatter.clone();
    fm.pinned = !fm.pinned;
    state
        .memory_store
        .put(mem.scope, fm, &mem.body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let updated = state
        .memory_store
        .get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(updated.as_ref().map(to_item))
}
