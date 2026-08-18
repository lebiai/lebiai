use hermes_core::{
    derive_title_from_messages, session_has_user_text, Session, SessionMeta, DEFAULT_SESSION_TITLE,
};
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
    /// Last activity time (file mtime) — used for sidebar day grouping.
    pub updated_at: String,
    pub path: String,
    /// `wechat` / `feishu` / `telegram` when this file is a channel log.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Channel logs are view-only in the desktop composer.
    pub read_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSessionData {
    pub id: String,
    pub messages: Vec<MessageData>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    pub read_only: bool,
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
        hermes_core::ContentBlock::Image { source } => ContentBlockData::Text {
            text: format!("[image: {}]", source.media_type),
        },
    }
}

fn wechat_title(created: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::{Datelike, Timelike};
    let local = created.with_timezone(&chrono::Local);
    // Process restarts make several files a day — date alone collides.
    format!(
        "微信消息 · {}月{}日 {:02}:{:02}",
        local.month(),
        local.day(),
        local.hour(),
        local.minute()
    )
}

fn display_title(session: &Session, channel: Option<&str>) -> String {
    if channel == Some("wechat") {
        return wechat_title(&session.meta.created_at);
    }
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

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionSummary>, GuiError> {
    let sessions_dir = hermes_core::data_path("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    let _ = hermes_store::purge_empty_sessions(&sessions_dir);

    let paths =
        hermes_store::list_sessions(&sessions_dir).map_err(|e| GuiError::Session(e.to_string()))?;

    let mut entries: Vec<SessionSummary> = Vec::new();
    for path in paths {
        if let Ok(session) = hermes_store::read_session(&path) {
            if !session_has_user_text(&session.messages) {
                continue;
            }
            let updated_at = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_else(|| session.meta.created_at.to_rfc3339());
            let channel = hermes_store::channel_of_session_path(&path).map(|s| s.to_string());
            let read_only = channel.is_some();
            entries.push(SessionSummary {
                id: session.meta.id.clone(),
                title: display_title(&session, channel.as_deref()),
                created_at: session.meta.created_at.to_rfc3339(),
                updated_at,
                path: path.to_string_lossy().into_owned(),
                channel,
                read_only,
            });
        }
    }
    // Newest first after empty drafts are gone — empty files must not eat the cap.
    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    entries.truncate(50);
    crate::commands::reflect::spawn_idle_wechat_distill(&state);
    Ok(entries)
}

/// Start a draft chat. **Not** a historical session until the first user message:
/// - no JSONL on disk yet
/// - reuses an existing empty in-memory draft if any
/// - abandoned empty drafts are dropped without writing files
#[tauri::command]
pub async fn new_session(state: State<'_, AppState>) -> Result<SessionSummary, GuiError> {
    let _ = hermes_store::purge_empty_sessions(hermes_core::data_path("sessions"));

    let mut sessions = state.sessions.lock().await;

    // Reuse the single empty draft — spam "New Chat" must not create N empties.
    if let Some(id) = sessions
        .iter()
        .find(|(_, a)| a.session.messages.is_empty())
        .map(|(id, _)| id.clone())
    {
        let active = sessions.get(&id).expect("id just found");
        return Ok(SessionSummary {
            id: id.clone(),
            title: DEFAULT_SESSION_TITLE.into(),
            created_at: active.session.meta.created_at.to_rfc3339(),
            updated_at: active.session.meta.created_at.to_rfc3339(),
            path: active.path.to_string_lossy().into_owned(),
            channel: None,
            read_only: false,
        });
    }

    // Drop other empty drafts from memory (should be none after reuse branch).
    sessions.retain(|_, a| !a.session.messages.is_empty());

    let model = state.model();
    let provider = state.config.read().unwrap().default_provider.clone();
    let meta = SessionMeta::new(model, provider);
    let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;

    let id = meta.id.clone();
    let created_at = meta.created_at.to_rfc3339();

    // Memory-only draft: writer stays None until first send_message.
    sessions.insert(
        id.clone(),
        ActiveSession {
            session: Session {
                meta,
                messages: Vec::new(),
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
            writer: None,
            path: path.clone(),
        },
    );

    Ok(SessionSummary {
        id,
        title: DEFAULT_SESSION_TITLE.into(),
        created_at: created_at.clone(),
        updated_at: created_at,
        path: path.to_string_lossy().into_owned(),
        channel: None,
        read_only: false,
    })
}

#[tauri::command]
pub async fn load_session(
    state: State<'_, AppState>,
    path: String,
) -> Result<LoadedSessionData, GuiError> {
    let path =
        hermes_store::ensure_session_path(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    let mut session =
        hermes_store::read_session(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    // Repair incomplete tool pairs so continue-chat does not 400 on providers.
    session.messages = hermes_core::sanitize_history_for_provider(&session.messages);
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

    let channel = hermes_store::channel_of_session_path(&path).map(|s| s.to_string());
    let read_only = channel.is_some();
    let writer = if read_only {
        None
    } else {
        Some(SessionWriter::open_append(&path).map_err(|e| GuiError::Session(e.to_string()))?)
    };

    let data = LoadedSessionData {
        id: id.clone(),
        messages,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
        channel: channel.clone(),
        read_only,
    };

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
pub async fn delete_session(state: State<'_, AppState>, path: String) -> Result<(), GuiError> {
    let path =
        hermes_store::ensure_session_path(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| GuiError::Session(e.to_string()))?;
    }
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, a| a.path != path);
    Ok(())
}
