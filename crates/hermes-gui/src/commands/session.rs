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
    /// 项目组 id；`None` = 不属于任何项目组。与 `persona` **互斥**。
    /// 前端按它把会话挂到侧栏「项目组」那一节。
    pub team: Option<String>,
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
    /// 人物（工位）id；`None` = 无人物会话。
    /// **打开历史时也要带上**：不带的话，点回某个工位会丢掉「这是谁在跟你说话」。
    pub persona: Option<String>,
    /// 项目组 id；`None` = 不属于任何项目组。与 `persona` **互斥**。
    pub team: Option<String>,
    /// 窗口前面还有多少条消息（会话数组里的下标）。
    /// **编辑重发的截断要把它加回去**：窗口内的下标不是文件里的下标。
    pub base_offset: usize,
    /// 更早的日子（最新在前）；短会话是空的——空 = 没有折叠条。
    pub days: Vec<SessionDayData>,
}

/// 一条被折起来的「那天」。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDayData {
    /// 本地日 `YYYY-MM-DD`；`None` = 那段消息没有日期（老文件 / 压缩摘要）。
    pub day: Option<String>,
    /// 界面上写的一行：`9 月 16 日` / `更早`。
    pub label: String,
    /// 人说了几句。
    pub turns: usize,
    /// 这一段有多少条消息。
    pub messages: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageData {
    pub role: String,
    pub content: Vec<ContentBlockData>,
    /// 这一轮开口的人（人物 id）。组会话里每一轮的人会换，界面按它标名字 ——
    /// 没有这一项，吕老师开口那一轮会和海燕的并成一块（用户原话：「被埋」）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
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

fn message_to_data(m: &hermes_core::Message) -> MessageData {
    MessageData {
        role: match m.role {
            hermes_core::Role::User => "user".into(),
            hermes_core::Role::Assistant => "assistant".into(),
        },
        content: m.content.iter().map(content_block_to_data).collect(),
        speaker: m.speaker.clone(),
    }
}

/// 整条会话 → 界面要的那一份：`(窗口前有多少条, 更早的按天索引, 最近窗口)`。
///
/// 判据只有 `hermes_store::window_split` 一处；这里只做形状转换。短会话
/// （不超过一个窗口）返回 `(0, 空, 全部)`——**逐条不变**是硬要求。
fn window_for_display(
    messages: &[hermes_core::Message],
) -> (usize, Vec<SessionDayData>, Vec<MessageData>) {
    let (base, days) = hermes_store::window_split(messages, hermes_store::DEFAULT_WINDOW);
    let days = days
        .into_iter()
        .map(|g| SessionDayData {
            day: g.day,
            label: g.label,
            turns: g.turns,
            messages: g.messages,
        })
        .collect();
    (
        base,
        days,
        messages[base..].iter().map(message_to_data).collect(),
    )
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
            team: meta.team.clone(),
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

/// 这个空草稿能不能被这次开会话复用：**身份必须完全一致**（人物与项目组两条轴）。
/// 换身份开会话 = 换一位同事 / 换一张桌子，不能顶着上一个身份接着写。
fn reuses_empty_draft(
    draft_persona: Option<&str>,
    draft_team: Option<&str>,
    requested_persona: Option<&str>,
    requested_team: Option<&str>,
) -> bool {
    draft_persona == requested_persona && draft_team == requested_team
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
        team: active.session.meta.team.clone(),
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
    team_id: Option<String>,
) -> Result<SessionSummary, GuiError> {
    // 人物 id 必须先存在：写进 `meta.persona` 的不认识的 id 会让这个会话
    // 既没有人设也没有归属，且一路静默——所以在这里挡住，而不是让它落盘。
    if let Some(id) = persona_id.as_deref() {
        crate::commands::personas::require_persona(id)?;
    }
    // 两条轴互斥：一条会话要么属于某个人物、要么属于某个项目组。两个都写会让
    // 「这段记忆归谁」「这一轮谁在说」同时失去唯一答案。
    if persona_id.is_some() && team_id.is_some() {
        return Err(GuiError::Internal(
            "会话不能同时属于一个人物和一个项目组".into(),
        ));
    }
    // 项目组必须先存在、且接口人开着：没人接的桌子开不出一条会话。
    if let Some(id) = team_id.as_deref() {
        crate::commands::teams::require_team_ready(id)?;
    }
    let mut sessions = state.sessions.lock().await;

    // Reuse the single empty draft — spam "New Chat" must not create N empties.
    if let Some(id) = sessions
        .iter()
        .find(|(_, a)| {
            a.session.messages.is_empty()
                && reuses_empty_draft(
                    a.session.meta.persona.as_deref(),
                    a.session.meta.team.as_deref(),
                    persona_id.as_deref(),
                    team_id.as_deref(),
                )
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
    meta.team = team_id.clone();
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
                flow: Default::default(),
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

    let channel = hermes_store::channel_of_session_path(&path).map(|s| s.to_string());
    let read_only = channel.is_some();

    // 只把**最近窗口**交给界面（`docs/spec/projects.md` §4.4）：更早的按天折起来，
    // 点开那一条时才来取（`load_session_day`）。`base_offset` 是窗口前还有多少条，
    // 编辑重发要把它加回去——判据只有 `hermes_store::window_split` 一处。
    let (base_offset, days, messages) = window_for_display(&session.messages);

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
        persona: session.meta.persona.clone(),
        team: session.meta.team.clone(),
        base_offset,
        days,
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

/// 展开某一天：把被折起来的那一段取回来。
///
/// 从**内存里的那条会话**取，所以下标与窗口、与编辑重发的截断是同一个数组；
/// 旧账只是回看——界面不给它编辑/重发的入口（v1）。
#[tauri::command]
pub async fn load_session_day(
    state: State<'_, AppState>,
    session_id: String,
    day: Option<String>,
) -> Result<Vec<MessageData>, GuiError> {
    let sessions = state.sessions.lock().await;
    let active = sessions
        .get(&session_id)
        .ok_or_else(|| GuiError::Session("session not found".into()))?;

    // 判据与 `load_session` 画折叠条时用的那一刀是同一个（`window_day`）：
    // 声称多少条就取回多少条，且绝不会和已经展开的最近窗口重叠。
    let group = hermes_store::window_day(
        &active.session.messages,
        hermes_store::DEFAULT_WINDOW,
        day.as_deref(),
    )
    .ok_or_else(|| GuiError::Session("这一天不在折叠区里".into()))?;

    Ok(
        active.session.messages[group.from..group.from + group.messages]
            .iter()
            .map(message_to_data)
            .collect(),
    )
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

    fn say(text: &str, day: u32, hour: u32) -> hermes_core::Message {
        let mut m = hermes_core::Message::user_text(text);
        m.at = Some(
            format!("2026-09-{day:02}T{hour:02}:00:00Z")
                .parse()
                .unwrap(),
        );
        m
    }

    /// 短会话：一条都不折，下标从 0 起——开箱即用的那条路逐条不变。
    #[test]
    fn a_short_session_is_sent_whole_with_no_folds_and_no_offset() {
        let msgs: Vec<hermes_core::Message> =
            (0..5).map(|i| say(&format!("第 {i} 句"), 17, 4)).collect();
        let (base, days, window) = window_for_display(&msgs);
        assert_eq!(base, 0, "没有折起来的东西，偏移就是 0");
        assert!(days.is_empty(), "短会话不出现折叠条");
        assert_eq!(window.len(), 5, "一条不少");
    }

    /// 长会话：只发窗口，并且**说清楚前面还有多少条**——编辑重发就靠这个数。
    #[test]
    fn a_long_session_sends_the_window_and_the_offset_that_editing_needs() {
        let msgs: Vec<hermes_core::Message> = (0..200)
            .map(|i| say(&format!("第 {i} 句"), 16 + (i % 2), 4))
            .collect();
        let (base, days, window) = window_for_display(&msgs);
        assert_eq!(base, 200 - hermes_store::DEFAULT_WINDOW);
        assert_eq!(window.len(), hermes_store::DEFAULT_WINDOW);
        assert_eq!(base + window.len(), msgs.len(), "窗口 + 偏移 = 全量");
        assert!(!days.is_empty(), "更早的日子要有折叠条");
        assert!(
            days.iter().all(|d| d.messages > 0 && d.turns > 0),
            "每条折叠条都得说清楚有多少轮"
        );
    }

    #[test]
    fn a_draft_summary_carries_the_persona_the_frontend_reads() {
        let summary = draft_summary("abc12345", &draft(Some("sao-di-seng")));
        assert_eq!(summary.persona.as_deref(), Some("sao-di-seng"));
        let v = serde_json::to_value(&summary).unwrap();
        assert_eq!(v["persona"], "sao-di-seng", "字段名就是前端读的那个");
        assert_eq!(v["readOnly"], false);

        let free = draft_summary("abc12345", &draft(None));
        assert_eq!(free.persona, None);
        assert!(serde_json::to_value(&free).unwrap()["persona"].is_null());
    }

    #[test]
    fn an_empty_draft_is_reused_only_by_its_own_identity() {
        assert!(reuses_empty_draft(
            Some("sao-di-seng"),
            None,
            Some("sao-di-seng"),
            None
        ));
        assert!(
            reuses_empty_draft(None, None, None, None),
            "自由对话的空草稿照样复用"
        );
        assert!(
            !reuses_empty_draft(Some("sao-di-seng"), None, Some("yu-tian"), None),
            "换了人物就是另一位同事，不能复用同一份草稿"
        );
        assert!(!reuses_empty_draft(Some("sao-di-seng"), None, None, None));
        assert!(!reuses_empty_draft(
            None,
            Some("caifu-zaozhidao"),
            None,
            None
        ));
        assert!(
            !reuses_empty_draft(None, Some("caifu-zaozhidao"), Some("sao-di-seng"), None),
            "从工位切到项目组 = 换了一张桌子，不能复用同一份草稿"
        );
        assert!(reuses_empty_draft(
            None,
            Some("caifu-zaozhidao"),
            None,
            Some("caifu-zaozhidao")
        ));
    }
}
