//! Session REST endpoints. 1:1 with `hermes-gui/src/commands/session.rs` —
//! same DTOs and storage calls, minus `#[tauri::command]`.
//!
//! `GET    /api/v1/sessions`            → list (newest first, capped 50)
//! `POST   /api/v1/sessions`            → new
//! `GET    /api/v1/sessions/load?path=` → load (replay + re-attach writer)
//! `DELETE /api/v1/sessions?path=`      → delete

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use hermes_core::{Session, SessionEvent, SessionMeta};
use hermes_store::{self, SessionWriter};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::{session_path_for, ActiveSession, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSessionData {
    pub id: String,
    pub messages: Vec<MessageData>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageData {
    pub role: String,
    pub content: Vec<ContentBlockData>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ContentBlockData {
    #[serde(rename_all = "camelCase")]
    Text { text: String },
    #[serde(rename_all = "camelCase")]
    Thinking { thinking: String },
    #[serde(rename_all = "camelCase")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        source: ImageSourceData,
    },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageSourceData {
    #[serde(rename = "type")]
    pub kind: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: String,
}

fn content_block_to_data(block: &hermes_core::ContentBlock) -> ContentBlockData {
    match block {
        hermes_core::ContentBlock::Text { text } => ContentBlockData::Text { text: text.clone() },
        hermes_core::ContentBlock::Thinking { thinking, .. } => {
            ContentBlockData::Thinking { thinking: thinking.clone() }
        }
        hermes_core::ContentBlock::ToolUse { id, name, input } => ContentBlockData::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        hermes_core::ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => ContentBlockData::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        hermes_core::ContentBlock::Image { source } => ContentBlockData::Image {
            source: ImageSourceData {
                kind: source.kind.clone(),
                media_type: source.media_type.clone(),
                data: source.data.clone(),
            },
        },
    }
}

pub async fn list_sessions() -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let home = dirs::home_dir().ok_or_else(|| ApiError::Internal("no $HOME".into()))?;
    let sessions_dir = home.join(".small-rust-hermes").join("sessions");
    if !sessions_dir.exists() {
        return Ok(Json(Vec::new()));
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&sessions_dir)
        .map_err(|e| ApiError::Session(e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    paths.sort_by(|a, b| b.cmp(a));
    paths.truncate(50);

    let mut entries = Vec::new();
    for path in paths {
        if let Ok(session) = hermes_store::read_session(&path) {
            let title = session
                .messages
                .iter()
                .find(|m| m.role == hermes_core::Role::User)
                .and_then(|m| {
                    m.content.iter().find_map(|b| match b {
                        hermes_core::ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "New Chat".into());
            let title = if title.chars().count() > 60 {
                let head: String = title.chars().take(57).collect();
                format!("{head}...")
            } else {
                title
            };
            entries.push(SessionSummary {
                id: session.meta.id.clone(),
                title,
                created_at: session.meta.created_at.to_rfc3339(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(Json(entries))
}

pub async fn new_session(State(state): State<Arc<AppState>>) -> Result<Json<SessionSummary>, ApiError> {
    let model = state.model().to_string();
    let provider = state.config.default_provider.clone();
    let meta = SessionMeta::new(model, provider);
    let path = session_path_for(&meta).map_err(|e| ApiError::Session(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Session(e.to_string()))?;
    }
    let mut writer = SessionWriter::create(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    writer
        .append(&SessionEvent::Meta(meta.clone()))
        .map_err(|e| ApiError::Session(e.to_string()))?;

    let id = meta.id.clone();
    let created_at = meta.created_at.to_rfc3339();
    let session = Session {
        meta,
        messages: Vec::new(),
        total_input_tokens: 0,
        total_output_tokens: 0,
    };
    state.sessions.lock().await.insert(
        id.clone(),
        ActiveSession {
            session,
            writer,
            path: path.clone(),
        },
    );

    Ok(Json(SessionSummary {
        id,
        title: "New Chat".into(),
        created_at,
        path: path.to_string_lossy().into_owned(),
    }))
}

pub async fn load_session(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Result<Json<LoadedSessionData>, ApiError> {
    let path = PathBuf::from(&q.path);
    let session =
        hermes_store::read_session(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    let id = session.meta.id.clone();

    let messages: Vec<MessageData> = session
        .messages
        .iter()
        .map(|m| MessageData {
            role: match m.role {
                hermes_core::Role::User => "user".into(),
                hermes_core::Role::Assistant => "assistant".into(),
            },
            content: m.content.iter().map(content_block_to_data).collect(),
        })
        .collect();

    let data = LoadedSessionData {
        id: id.clone(),
        messages,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
    };

    let writer =
        SessionWriter::open_append(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    state.sessions.lock().await.insert(
        id,
        ActiveSession {
            session,
            writer,
            path,
        },
    );

    Ok(Json(data))
}

pub async fn delete_session(Query(q): Query<PathQuery>) -> Result<Json<()>, ApiError> {
    let path = PathBuf::from(&q.path);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    }
    Ok(Json(()))
}
