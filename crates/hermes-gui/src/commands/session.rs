use std::path::PathBuf;

use hermes_core::{Session, SessionEvent, SessionMeta};
use hermes_store::{self, SessionWriter};
use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
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
        hermes_core::ContentBlock::Image { source } => ContentBlockData::Text {
            text: format!("[image: {}]", source.media_type),
        },
    }
}

#[tauri::command]
pub async fn list_sessions() -> Result<Vec<SessionSummary>, GuiError> {
    let home = dirs::home_dir().ok_or_else(|| GuiError::Internal("no $HOME".into()))?;
    let sessions_dir = home.join(".small-rust-hermes").join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<SessionSummary> = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&sessions_dir)
        .map_err(|e| GuiError::Session(e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    paths.sort_by(|a, b| b.cmp(a));
    paths.truncate(50);

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
                format!("{}...", head)
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
    Ok(entries)
}

#[tauri::command]
pub async fn new_session(state: State<'_, AppState>) -> Result<SessionSummary, GuiError> {
    let model = state.model().to_string();
    let meta = SessionMeta::new(model, "anthropic".to_string());
    let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| GuiError::Session(e.to_string()))?;
    }
    let mut writer = SessionWriter::create(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    writer
        .append(&SessionEvent::Meta(meta.clone()))
        .map_err(|e| GuiError::Session(e.to_string()))?;

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

    Ok(SessionSummary {
        id,
        title: "New Chat".into(),
        created_at,
        path: path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn load_session(
    state: State<'_, AppState>,
    path: String,
) -> Result<LoadedSessionData, GuiError> {
    let path = PathBuf::from(&path);
    let session = hermes_store::read_session(&path).map_err(|e| GuiError::Session(e.to_string()))?;
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
        SessionWriter::open_append(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    state.sessions.lock().await.insert(
        id,
        ActiveSession {
            session,
            writer,
            path,
        },
    );

    Ok(data)
}

#[tauri::command]
pub async fn delete_session(path: String) -> Result<(), GuiError> {
    let path = PathBuf::from(&path);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    }
    Ok(())
}
