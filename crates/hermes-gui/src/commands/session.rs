use hermes_core::{Session, SessionMeta, DEFAULT_SESSION_TITLE};
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
    /// 人物（工位）id；`None` = 无人物会话（自由对话 / 旧会话）。
    /// 前端按它把会话挂到对应人物下。
    pub persona: Option<String>,
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

#[tauri::command]
pub async fn list_sessions(_state: State<'_, AppState>) -> Result<Vec<SessionSummary>, GuiError> {
    let sessions_dir = hermes_core::data_path("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    let purge_dir = sessions_dir.clone();
    tokio::task::spawn_blocking(move || {
        let _ = hermes_store::purge_empty_sessions(purge_dir);
    });

    let paths =
        hermes_store::list_sessions(&sessions_dir).map_err(|e| GuiError::Session(e.to_string()))?;

    let mut desktop: Vec<SessionSummary> = Vec::new();
    let mut channel_rows: Vec<SessionSummary> = Vec::new();
    for path in paths {
        let Ok((meta, has_user)) = hermes_store::read_session_listing(&path) else {
            continue;
        };
        let channel = hermes_store::channel_of_session_path(&path).map(|s| s.to_string());
        let read_only = channel.is_some();
        if !read_only && !has_user {
            continue;
        }
        let updated_at = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            })
            .unwrap_or_else(|| meta.created_at.to_rfc3339());
        let title = if channel.as_deref() == Some("wechat") {
            wechat_title(&meta.created_at)
        } else {
            meta.title
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_SESSION_TITLE)
                .to_string()
        };
        let row = SessionSummary {
            id: meta.id.clone(),
            title,
            created_at: meta.created_at.to_rfc3339(),
            updated_at,
            path: path.to_string_lossy().into_owned(),
            persona: meta.persona.clone(),
            channel,
            read_only,
        };
        if read_only {
            channel_rows.push(row);
        } else {
            desktop.push(row);
        }
    }
    desktop.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    desktop.truncate(50);
    channel_rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut entries = desktop;
    entries.extend(channel_rows);
    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(entries)
}

/// 这个空草稿能不能被这次开会话复用：**人物必须一致**。
/// 换人物开会话 = 换一位同事，不能顶着上一位的身份接着写。
fn reuses_empty_draft(draft_persona: Option<&str>, requested: Option<&str>) -> bool {
    draft_persona == requested
}

/// 草稿摘要：草稿还没落盘、也还没有标题，所以标题一律是占位。
/// 身份（persona）照 `meta` 报——这是前端认领会话的唯一来源。
fn draft_summary(id: &str, active: &ActiveSession) -> SessionSummary {
    let created_at = active.session.meta.created_at.to_rfc3339();
    SessionSummary {
        id: id.to_string(),
        title: DEFAULT_SESSION_TITLE.into(),
        created_at: created_at.clone(),
        updated_at: created_at,
        path: active.path.to_string_lossy().into_owned(),
        channel: None,
        read_only: false,
        persona: active.session.meta.persona.clone(),
    }
}

/// Start a draft chat. **Not** a historical session until the first user message:
/// - no JSONL on disk yet
/// - reuses an existing empty in-memory draft **only when the persona matches**
///   (换人物开会话 = 换一位同事，不能顶着上一位的身份接着写)
/// - abandoned empty drafts are dropped without writing files
#[tauri::command]
pub async fn new_session(
    state: State<'_, AppState>,
    persona_id: Option<String>,
) -> Result<SessionSummary, GuiError> {
    // 人物 id 必须先存在：写进 `meta.persona` 的不认识的 id 会让这个会话
    // 既没有人设也没有归属，且一路静默——所以在这里挡住，而不是让它落盘。
    if let Some(id) = persona_id.as_deref() {
        crate::commands::personas::require_persona(id)?;
    }
    let mut sessions = state.sessions.lock().await;

    // Reuse the single empty draft — spam "New Chat" must not create N empties.
    if let Some(id) = sessions
        .iter()
        .find(|(_, a)| {
            a.session.messages.is_empty()
                && reuses_empty_draft(a.session.meta.persona.as_deref(), persona_id.as_deref())
        })
        .map(|(id, _)| id.clone())
    {
        let active = sessions.get(&id).expect("id just found");
        return Ok(draft_summary(&id, active));
    }

    // Drop other empty drafts from memory (should be none after reuse branch):
    // 换人物的草稿不落盘、也不留着，避免下次开会话摸到一具旧身份的空壳。
    sessions.retain(|_, a| !a.session.messages.is_empty());

    let model = state.model();
    let provider = state.config.read().unwrap().default_provider.clone();
    let mut meta = SessionMeta::new(model, provider);
    meta.persona = persona_id.clone();
    let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;

    let id = meta.id.clone();

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

    let active = sessions.get(&id).expect("just inserted");
    Ok(draft_summary(&id, active))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(persona: Option<&str>) -> ActiveSession {
        let mut meta = SessionMeta::new("test-model", "test-provider");
        meta.persona = persona.map(str::to_string);
        ActiveSession {
            session: Session::new(meta),
            writer: None,
            path: std::path::PathBuf::from("/tmp/lebi-gui-draft.jsonl"),
        }
    }

    #[test]
    fn a_draft_summary_carries_the_persona_the_frontend_reads() {
        let summary = draft_summary("abc12345", &draft(Some("xiao-jin")));
        assert_eq!(summary.persona.as_deref(), Some("xiao-jin"));
        let v = serde_json::to_value(&summary).unwrap();
        assert_eq!(v["persona"], "xiao-jin", "字段名就是前端读的那个");
        assert_eq!(v["readOnly"], false);

        let free = draft_summary("abc12345", &draft(None));
        assert_eq!(free.persona, None);
        assert!(serde_json::to_value(&free).unwrap()["persona"].is_null());
    }

    #[test]
    fn an_empty_draft_is_reused_only_by_its_own_persona() {
        assert!(reuses_empty_draft(Some("xiao-xie"), Some("xiao-xie")));
        assert!(reuses_empty_draft(None, None), "自由对话的空草稿照样复用");
        assert!(
            !reuses_empty_draft(Some("xiao-xie"), Some("yu-tian")),
            "换了人物就是另一位同事，不能复用同一份草稿"
        );
        assert!(!reuses_empty_draft(Some("xiao-xie"), None));
        assert!(!reuses_empty_draft(None, Some("yu-tian")));
    }
}
