//! Session REST endpoints. 1:1 with `hermes-gui/src/commands/session.rs` —
//! same DTOs and storage calls, minus `#[tauri::command]`.
//!
//! `GET    /api/v1/sessions`            → list (newest first, capped 50)
//! `POST   /api/v1/sessions`            → new
//! `GET    /api/v1/sessions/load?path=` → load (replay + re-attach writer)
//! `DELETE /api/v1/sessions?path=`      → delete

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use hermes_core::{
    derive_title_from_messages, session_has_user_text, Session, SessionMeta, DEFAULT_SESSION_TITLE,
};
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
    Image { source: ImageSourceData },
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
        hermes_core::ContentBlock::Thinking { thinking, .. } => ContentBlockData::Thinking {
            thinking: thinking.clone(),
        },
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

fn display_title(session: &Session) -> String {
    if let Some(t) = session
        .meta
        .title
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return t.to_string();
    }
    derive_title_from_messages(&session.messages)
}

pub async fn list_sessions() -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let sessions_dir = hermes_core::data_path("sessions");
    if !sessions_dir.exists() {
        return Ok(Json(Vec::new()));
    }
    let _ = hermes_store::purge_empty_sessions(&sessions_dir);

    let paths =
        hermes_store::list_sessions(&sessions_dir).map_err(|e| ApiError::Session(e.to_string()))?;

    let mut entries = Vec::new();
    for path in paths {
        if let Ok(session) = hermes_store::read_session(&path) {
            if !session_has_user_text(&session.messages) {
                continue;
            }
            entries.push(SessionSummary {
                id: session.meta.id.clone(),
                title: display_title(&session),
                created_at: session.meta.created_at.to_rfc3339(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    entries.truncate(50);
    Ok(Json(entries))
}

pub async fn new_session(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionSummary>, ApiError> {
    let _ = hermes_store::purge_empty_sessions(hermes_core::data_path("sessions"));

    let mut sessions = state.sessions.lock().await;
    if let Some(id) = sessions
        .iter()
        .find(|(_, a)| a.session.messages.is_empty())
        .map(|(id, _)| id.clone())
    {
        let active = sessions.get(&id).expect("id just found");
        return Ok(Json(SessionSummary {
            id: id.clone(),
            title: DEFAULT_SESSION_TITLE.into(),
            created_at: active.session.meta.created_at.to_rfc3339(),
            path: active.path.to_string_lossy().into_owned(),
        }));
    }
    sessions.retain(|_, a| !a.session.messages.is_empty());

    let model = state.model();
    let provider = state.config.read().unwrap().default_provider.clone();
    let meta = SessionMeta::new(model, provider);
    let path = session_path_for(&meta).map_err(|e| ApiError::Session(e.to_string()))?;

    let id = meta.id.clone();
    let created_at = meta.created_at.to_rfc3339();
    let session = Session {
        meta,
        messages: Vec::new(),
        total_input_tokens: 0,
        total_output_tokens: 0,
    };
    sessions.insert(
        id.clone(),
        ActiveSession {
            session,
            writer: None,
            path: path.clone(),
        },
    );

    Ok(Json(SessionSummary {
        id,
        title: DEFAULT_SESSION_TITLE.into(),
        created_at,
        path: path.to_string_lossy().into_owned(),
    }))
}

pub async fn load_session(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Result<Json<LoadedSessionData>, ApiError> {
    let path =
        hermes_store::ensure_session_path(&q.path).map_err(|e| ApiError::Session(e.to_string()))?;
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

    let writer = SessionWriter::open_append(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    state.sessions.lock().await.insert(
        id,
        ActiveSession {
            session,
            writer: Some(writer),
            path,
        },
    );

    Ok(Json(data))
}

pub async fn delete_session(Query(q): Query<PathQuery>) -> Result<Json<()>, ApiError> {
    let path =
        hermes_store::ensure_session_path(&q.path).map_err(|e| ApiError::Session(e.to_string()))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| ApiError::Session(e.to_string()))?;
    }
    Ok(Json(()))
}
