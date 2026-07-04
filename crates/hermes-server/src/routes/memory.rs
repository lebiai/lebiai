//! Memory REST. 1:1 with `hermes-gui/src/commands/memory.rs`.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use hermes_memory::{Confidence, LoadedMemory, MemoryFrontmatter, MemoryStore, Scope, Source};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
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

fn to_item(m: &LoadedMemory) -> MemoryItem {
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

#[derive(Deserialize)]
pub struct ScopeQuery {
    pub scope: Option<String>,
}

fn parse_scope(scope: &str) -> Scope {
    match scope {
        "Project" => Scope::Project,
        _ => Scope::User,
    }
}

pub async fn list_memories(State(state): State<Arc<AppState>>) -> Result<Json<Vec<MemoryItem>>, ApiError> {
    let memories = state
        .memory_store
        .list_active()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(memories.iter().map(to_item).collect()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMemoryBody {
    pub body: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub zone: Option<String>,
    pub pinned: bool,
}

pub async fn create_memory(
    State(state): State<Arc<AppState>>,
    Json(b): Json<CreateMemoryBody>,
) -> Result<Json<MemoryItem>, ApiError> {
    let s = parse_scope(&b.scope);
    let zone = b
        .zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, b.tags, zone);
    fm.pinned = b.pinned;
    state
        .memory_store
        .put(s, fm.clone(), &b.body)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(MemoryItem {
        id: fm.id.clone(),
        body: b.body,
        scope: format!("{:?}", s),
        pinned: fm.pinned,
        confidence: format!("{:?}", fm.confidence),
        tags: fm.tags.clone(),
        zone: fm.zone.clone(),
        created_at: fm.created.to_rfc3339(),
        source: "User".into(),
    }))
}

pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<bool>, ApiError> {
    let scope = q.scope.as_deref().unwrap_or("User");
    state
        .memory_store
        .delete(parse_scope(scope), &id)
        .map_err(|e| ApiError::Internal(e.to_string()))
        .map(Json)
}

pub async fn toggle_pin_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<MemoryItem>>, ApiError> {
    let mem = state
        .memory_store
        .get(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let Some(mem) = mem else {
        return Ok(Json(None));
    };

    let mut fm = mem.frontmatter.clone();
    fm.pinned = !fm.pinned;
    state
        .memory_store
        .put(mem.scope, fm, &mem.body)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let updated = state
        .memory_store
        .get(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(updated.as_ref().map(to_item)))
}
