use hermes_core::{Message, Role, Session, SessionEvent, SessionMeta};

use hermes_memory::MemoryStore;
use hermes_store::SessionWriter;
use hermes_turn::{ConfirmAction, TurnConfig, TurnEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use crate::commands::micro;
use crate::context::ContextSources;
use crate::error::GuiError;
use crate::events::ChatStreamEvent;
use crate::state::{session_path_for, ActiveSession, AppState};

/// Last non-empty human user text (skips tool-result-only user rows).
fn last_human_user_text(messages: &[Message]) -> Option<String> {
    for m in messages.iter().rev() {
        if m.role != Role::User || m.is_tool_result_only() || m.is_internal_instruction_only() {
            continue;
        }
        let text: String = m
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}

fn strip_attachments_header(text: &str) -> String {
    match text.find("[attachments]") {
        Some(i) => text[..i].trim().to_string(),
        None => text.trim().to_string(),
    }
}

/// Index after the last human user message (exclusive end for truncate-to-user-keep).
fn end_after_last_human_user(messages: &[Message]) -> Option<usize> {
    for (i, m) in messages.iter().enumerate().rev() {
        if m.role == Role::User && !m.is_tool_result_only() && !m.is_internal_instruction_only() {
            let text: String = m
                .content
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return Some(i + 1);
            }
        }
    }
    None
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    begin_turn(app, state, session_id, Some(content), on_event).await
}

/// Re-run the agent on current history **without** pushing a new user message.
/// Caller must truncate trailing assistant/tool turns first (see `truncate_session`).
#[tauri::command]
pub async fn regenerate_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    begin_turn(app, state, session_id, None, on_event).await
}

async fn begin_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mut new_user: Option<String>,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    // License / trial gate (docs/spec/license-ux.md).
    if !hermes_core::can_use_main() {
        return Err(GuiError::Config("license_locked".into()));
    }

    // No API key configured → refuse before any request leaves the machine.
    // Frontend already gates on hasApiKey; this is defense for direct invoke.
    let active_key = state
        .config
        .read()
        .unwrap()
        .active_provider()
        .map_err(|e| GuiError::Config(e.to_string()))?
        .api_key
        .clone();
    if active_key.trim().is_empty() {
        return Err(GuiError::Config(
            "no API key configured — add one in Settings first".into(),
        ));
    }

    let provider = state.provider.read().unwrap().clone();
    let host = state.host.clone();
    let model = state.model();
    let max_tokens = state.max_tokens();
    let tools = state.tools.lock().await.clone();
    let skills = state.refresh_skills().await;
    // Refresh memory cache from disk so mid-session memory_save is visible next turn.
    let active = {
        let all = match state.memory_store.list_active() {
            Ok(v) => v,
            Err(_) => state.active_memories.lock().await.clone(),
        };
        let pinned: Vec<_> = all
            .iter()
            .filter(|m| m.frontmatter.pinned)
            .cloned()
            .collect();
        *state.active_memories.lock().await = all.clone();
        *state.pinned_memories.lock().await = pinned;
        all
    };
    let workspace_root = state.workspace_root();
    let (limits, default_provider) = {
        let cfg = state.config.read().unwrap();
        (cfg.limits, cfg.default_provider.clone())
    };

    let allow_rules: Vec<String> = state.config.read().unwrap().permissions.allow.clone();
    let deny_rules: Vec<String> = state.config.read().unwrap().permissions.deny.clone();
    let permissions = hermes_turn::PermissionChecker::new(&allow_rules, &deny_rules);

    let mut sessions = state.sessions.lock().await;
    if let Some(s) = sessions.get(&session_id) {
        if hermes_store::channel_of_session_path(&s.path).is_some() {
            return Err(GuiError::Session(
                "channel records are view-only in the desktop app".into(),
            ));
        }
    }
    let active_session = if let Some(s) = sessions.get_mut(&session_id) {
        s
    } else {
        if new_user.is_none() {
            return Err(GuiError::Session("no active session for regenerate".into()));
        }
        let meta = SessionMeta::new(model.clone(), default_provider);
        let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;
        sessions.insert(
            session_id.clone(),
            ActiveSession {
                session: Session {
                    meta,
                    messages: Vec::new(),
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
                writer: None,
                path,
            },
        );
        sessions.get_mut(&session_id).unwrap()
    };

    let prompt_for_system = if let Some(ref content) = new_user {
        content.clone()
    } else {
        last_human_user_text(&active_session.session.messages).ok_or_else(|| {
            GuiError::Session("cannot regenerate: no user message in history".into())
        })?
    };

    let material_query = strip_attachments_header(&prompt_for_system);
    if hermes_sources::is_topic_reset(&material_query)
        || hermes_sources::wants_other_file(&material_query)
    {
        state.source_focus.lock().await.remove(&session_id);
    }
    let stored_focus = state
        .source_focus
        .lock()
        .await
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let focus: &[String] = if hermes_sources::is_topic_reset(&material_query) {
        &[]
    } else {
        &stored_focus
    };
    let material_hits: Vec<hermes_core::MaterialHit> = state
        .source_store
        .search(&material_query, focus)
        .into_iter()
        .map(|h| hermes_core::MaterialHit {
            id: h.id,
            title: h.title,
            excerpt: h.excerpt,
        })
        .collect();
    if hermes_sources::is_topic_reset(&material_query) {
        // stay cleared
    } else if !material_hits.is_empty() {
        let ids: Vec<String> = material_hits.iter().map(|h| h.id.clone()).collect();
        state
            .source_focus
            .lock()
            .await
            .insert(session_id.clone(), ids);
    } else if !hermes_sources::is_followup_query(&material_query) {
        state.source_focus.lock().await.remove(&session_id);
    }

    if hermes_sources::wants_on_hand(&material_query) {
        state.source_store.set_read_allowlist(None);
    } else if material_hits.is_empty() {
        state.source_store.set_read_allowlist(Some(Vec::new()));
    } else {
        let mut allow: Vec<String> = material_hits
            .iter()
            .flat_map(|h| [h.id.clone(), h.title.clone()])
            .collect();
        allow.sort();
        allow.dedup();
        state.source_store.set_read_allowlist(Some(allow));
    }
    if hermes_sources::wants_on_hand(&material_query) {
        let rows = state.source_store.list_active();
        if let Some(ref mut content) = new_user {
            if !content.contains("[on-hand]") {
                content.push_str("\n\n[on-hand]\n");
                if rows.is_empty() {
                    content.push_str("(none)\n");
                } else {
                    for r in &rows {
                        content.push_str(&format!("- 《{}》\n", r.title));
                    }
                }
            }
        }
    }

    if hermes_sources::wants_remember_standard(&material_query) {
        let title = material_hits
            .first()
            .map(|h| h.title.clone())
            .unwrap_or_else(|| "这份材料".into());
        let fact = format!(
            "做同类事时，好的标准是：{}。出处：《{}》。",
            material_query.chars().take(240).collect::<String>().trim(),
            title
        );
        let output = hermes_reflect::ReflectionOutput {
            summary: "from kept material".into(),
            memory_candidates: vec![hermes_reflect::MemoryCandidate {
                fact,
                tags: vec!["standard".into(), "from-material".into()],
                zone: "standards".into(),
                scope: hermes_memory::Scope::User,
                confidence: hermes_memory::Confidence::Medium,
                rationale: "user asked to remember a standard".into(),
                supersedes: vec![],
                owner: None,
            }],
            ..Default::default()
        };
        // 用户当场点「记住」是一个片段、不是整场 distill 的结果，所以只记工位、
        // 不带 `session_id`（`append_only`）——否则它会被同会话下一批 micro 候选
        // 或 session-end 那份挤掉。
        let mark = hermes_reflect::EnqueueMark::append_only(
            hermes_core::persona::memory_owner_for(active_session.session.meta.persona.as_deref()),
        );
        if let Ok(n) = hermes_reflect::enqueue_from_reflection_marked(
            &output,
            hermes_reflect::InboxSource::Micro,
            mark,
        ) {
            if n > 0 {
                let _ = on_event.send(crate::events::ChatStreamEvent::RememberQueued);
            }
        }
    }

    let open_work = state.commitment_store.list_live().unwrap_or_default();
    let first_human_today = {
        let (seq, at) = active_session.session.last_human_send();
        let today = chrono::Utc::now().date_naive();
        seq == 0 || at.map(|t| t.date_naive() < today).unwrap_or(true)
    };
    // 卡面按工位分区（§5.5）：全局那份所有视图共用，这个会话的人物接自己那份；
    // 无人物 / 自带角色 → `None`（只看全局那份）。
    let cards_owner =
        hermes_core::persona::memory_owner_for(active_session.session.meta.persona.as_deref());
    // 注入面与卡面**同轴**：人物会话只看得见「全局 + 本人物」，别人的一条都不给
    // （`docs/spec/personas.md` §5.2）。装配只走 `views_for` 一处——CLI 用的是同
    // 一个函数，谁再自己写一份判据，就会有一天只改一处、某个人物开始看见别人的记忆。
    // 注意：上面写进 `state.active_memories` 缓存的是**全量**，缓存本身不受影响。
    let (active, pinned) = visible_memories(&active, cards_owner.as_deref());
    let topic_cards = hermes_memory::topics::render_from_disk(&active, cards_owner.as_deref());
    // 指路名册 = 侧栏上真正有的那几个工位（授权 ∩ 勾选 + 自带）。
    let roster = hermes_core::persona::open();
    let sources = turn_sources(
        active_session,
        &roster,
        topic_cards.as_deref(),
        &pinned,
        &active,
        &skills,
        &open_work,
        &material_hits,
        first_human_today,
        &workspace_root,
        limits,
    );
    let turn_system = sources.build_turn_system(&prompt_for_system);

    if let Some(content) = new_user {
        let user_msg = active_session.session.push_user(&content).clone();
        active_session
            .ensure_writer()
            .map_err(|e| GuiError::Session(e.to_string()))?
            .append(&SessionEvent::Message(user_msg))
            .map_err(|e| GuiError::Session(e.to_string()))?;

        {
            use hermes_core::{
                derive_title_from_messages, is_trivial_user_text, DEFAULT_SESSION_TITLE,
            };
            let derived = derive_title_from_messages(&active_session.session.messages);
            let current = active_session
                .session
                .meta
                .title
                .as_deref()
                .unwrap_or(DEFAULT_SESSION_TITLE);
            let should_write = !is_trivial_user_text(&content)
                && (current == DEFAULT_SESSION_TITLE
                    || active_session.session.meta.title.is_none()
                    || hermes_core::is_trivial_user_text(current));
            if should_write && derived != DEFAULT_SESSION_TITLE {
                active_session.session.meta.title = Some(derived.clone());
                let path = active_session.path.clone();
                let _ = hermes_store::update_session_title(&path, &derived);
                if let Ok(w) = SessionWriter::open_append(&path) {
                    active_session.writer = Some(w);
                }
            }
        }
    }

    // Repair incomplete tool_use / tool_result pairs so OpenAI-compatible
    // providers (DeepSeek etc.) do not 400 on resume.
    let history = hermes_core::sanitize_history_for_provider(&active_session.session.messages);
    // Keep in-memory session aligned so subsequent turns stay clean.
    active_session.session.messages = history.clone();
    drop(sessions);

    if let Ok(mut guard) = state.propose_messages.write() {
        *guard = history.clone();
    }

    let allowed_titles: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(
        material_hits.iter().map(|h| h.title.clone()).collect(),
    ));

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state
        .cancel_tokens
        .lock()
        .await
        .insert(session_id.clone(), cancel_tx);

    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);
    let confirm_tokens_arc = state.confirm_tokens.clone();

    let sid = session_id.clone();
    let sessions_arc = state.sessions.clone();
    let cancel_tokens_arc = state.cancel_tokens.clone();
    let propose_queue = state.propose_queue.clone();
    let provider_for_micro = state.provider.read().unwrap().clone();
    let memory_store = state.memory_store.clone();
    let skills_for_micro = skills.clone();
    let memories_for_micro = active.clone();
    let active_memories_arc = state.active_memories.clone();
    let pinned_memories_arc = state.pinned_memories.clone();
    let micro_cooldown = state.micro_turns_since.clone();
    let (auto_accept, min_confidence) = micro::reflect_policy(&state);
    let persist_thinking = state.config.read().unwrap().ui.persist_thinking;
    let ui_lang = state
        .config
        .read()
        .map(|c| c.ui.language.clone())
        .unwrap_or_else(|_| "zh-CN".into());
    let live_llm = state.live_llm.clone();

    tokio::spawn(async move {
        let _live = live_llm.enter();
        let config = TurnConfig {
            model,
            system: if turn_system.is_empty() {
                None
            } else {
                Some(turn_system)
            },
            max_tokens,
            max_tool_rounds: limits.max_tool_rounds,
            permissions,
        };

        let evt = on_event.clone();
        let confirm_tokens = confirm_tokens_arc.clone();
        let ui_lang_ev = ui_lang.clone();
        let app_ev = app.clone();
        let tool_names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let tool_names_ev = tool_names.clone();
        let allowed_ev = allowed_titles.clone();
        let on_turn_event = move |event: TurnEvent| match event {
            TurnEvent::TextDelta(text) => {
                let _ = evt.send(ChatStreamEvent::TextDelta { text });
            }
            TurnEvent::ThinkingDelta(text) => {
                let _ = evt.send(ChatStreamEvent::ThinkingDelta { text });
            }
            TurnEvent::ToolUseStart { id, name } => {
                let _ = evt.send(ChatStreamEvent::ToolUseStart { id, name });
            }
            TurnEvent::ToolExecStart {
                id, name, summary, ..
            } => {
                if let Ok(mut m) = tool_names_ev.lock() {
                    m.insert(id.clone(), name.clone());
                }
                let _ = evt.send(ChatStreamEvent::ToolExecStart { id, name, summary });
            }
            TurnEvent::ToolConfirmPending {
                id,
                tool_name,
                summary,
                reason,
            } => {
                let _ = evt.send(ChatStreamEvent::ConfirmRequired {
                    id,
                    tool_name,
                    summary,
                    reason,
                });
            }
            TurnEvent::ToolUseResult {
                id,
                content,
                is_error,
            } => {
                let name = tool_names_ev
                    .lock()
                    .ok()
                    .and_then(|m| m.get(&id).cloned())
                    .unwrap_or_default();
                if !is_error {
                    if let Some(title) = hermes_core::companion::parse_source_read_title(&content) {
                        if let Ok(mut a) = allowed_ev.lock() {
                            if !a.iter().any(|t| t == &title) {
                                a.push(title);
                            }
                        }
                    }
                }
                let _ = evt.send(ChatStreamEvent::ToolUseResult {
                    id,
                    content: content.clone(),
                    is_error,
                });
                if name.starts_with("commitment_") {
                    let _ = app_ev.emit("hermes://zaiban-changed", ());
                    if let Some(z) = parse_zaiban_tool(&name, &content, is_error) {
                        let _ = evt.send(z);
                    }
                }
            }
            TurnEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            } => {
                let _ = evt.send(ChatStreamEvent::UsageUpdate {
                    input_tokens,
                    output_tokens,
                });
            }
            TurnEvent::Error(message) => {
                let _ = evt.send(ChatStreamEvent::Error {
                    message: hermes_llm::humanize_error_lang(&message, &ui_lang_ev),
                });
            }
            TurnEvent::Cancelled => {
                let _ = evt.send(ChatStreamEvent::Cancelled);
            }
            TurnEvent::Done => {
                let _ = evt.send(ChatStreamEvent::Done);
            }
        };

        let ct = confirm_tokens.clone();
        let confirm_bridge = tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                ct.lock().await.insert(req.id, req.reply);
            }
        });

        let history_for_turn = hermes_channel::inject_time_header(history.clone());
        let result = hermes_turn::run_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &history_for_turn,
            &config,
            Some(confirm_tx),
            on_turn_event,
            cancel_rx,
        )
        .await;

        confirm_bridge.abort();

        let mut turn_messages: Vec<hermes_core::Message> = Vec::new();
        let mut session_id_for_log = sid.clone();
        let mut session_persona_for_log: Option<String> = None;

        match result {
            Ok(output) => {
                let mut msgs = output.new_messages;
                let allowed = allowed_titles.lock().map(|a| a.clone()).unwrap_or_default();
                for m in &mut msgs {
                    m.sanitize_material_cites(&allowed);
                }
                let corrected: String = msgs
                    .iter()
                    .filter(|m| m.role == hermes_core::Role::Assistant)
                    .flat_map(|m| m.content.iter())
                    .filter_map(|b| match b {
                        hermes_core::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !corrected.is_empty() {
                    let _ = on_event.send(ChatStreamEvent::TextCorrected { text: corrected });
                }
                turn_messages = msgs.clone();
                if let Some(s) = sessions_arc.lock().await.get_mut(&sid) {
                    session_id_for_log = s.session.meta.id.clone();
                    session_persona_for_log = s.session.meta.persona.clone();
                    for msg in &msgs {
                        s.session.messages.push(msg.clone());
                        let to_disk = msg.for_persist(persist_thinking);
                        if to_disk.content.is_empty() {
                            continue;
                        }
                        if let Ok(w) = s.ensure_writer() {
                            let _ = w.append(&SessionEvent::Message(to_disk));
                        }
                    }
                    s.session.total_input_tokens += output.usage.input_tokens;
                    s.session.total_output_tokens += output.usage.output_tokens;
                    if let Ok(w) = s.ensure_writer() {
                        let _ = w.append(&SessionEvent::Usage(hermes_core::Usage {
                            input_tokens: output.usage.input_tokens,
                            output_tokens: output.usage.output_tokens,
                            cache_read_tokens: output.usage.cache_read_tokens,
                            cache_creation_tokens: output.usage.cache_creation_tokens,
                        }));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "turn failed");
                let _ = on_event.send(ChatStreamEvent::Error {
                    message: hermes_llm::humanize_error_lang(&format!("{e:#}"), &ui_lang),
                });
                let _ = on_event.send(ChatStreamEvent::Done);
            }
        }

        let drained: Vec<hermes_reflect::SkillCandidate> = {
            match propose_queue.lock() {
                Ok(mut q) => q.drain(..).collect(),
                Err(_) => Vec::new(),
            }
        };
        if !drained.is_empty() {
            let pending = hermes_reflect::ReflectionOutput {
                summary: String::new(),
                skill_candidates: drained,
                memory_candidates: vec![],
                conflicts: vec![],
            };
            match hermes_reflect::enqueue_from_reflection(
                &pending,
                hermes_reflect::InboxSource::ManualReflect,
            ) {
                Ok(n) if n > 0 => {
                    tracing::info!(added = n, "propose_skill enqueued to inbox");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error=%e, "enqueue proposed skill failed"),
            }
        }

        // Yield the model before housekeeping so the next send is not queued.
        drop(_live);

        // Micro-reflection: separate lifecycle from the turn stream.
        // Notifies UI via Tauri event `hermes://micro-reflection` (not Channel).
        micro::spawn_after_turn(
            app,
            provider_for_micro,
            memory_store,
            active_memories_arc,
            pinned_memories_arc,
            micro_cooldown,
            skills_for_micro,
            memories_for_micro,
            turn_messages,
            sid.clone(),
            session_id_for_log,
            session_persona_for_log,
            auto_accept,
            min_confidence,
            live_llm,
        );

        cancel_tokens_arc.lock().await.remove(&sid);
    });

    Ok(())
}

/// 这一轮系统提示词的来源。人物只从 `session.meta.persona` 取——没绑定或不认识的
/// id → `None`（容错与 `persona::memory_owner_for` 一致，见 `personas::bound_persona`）。
///
/// 单独成函数是为了让「会话身份 → 人物块」这条接线被测试直接盯住：把 `persona`
/// 写死回 `None`，`a_bound_persona_reaches_the_turn_prompt` 必须红。
#[allow(clippy::too_many_arguments)]
fn turn_sources<'a>(
    active_session: &ActiveSession,
    roster: &'a [&'a hermes_core::persona::Persona],
    topic_cards: Option<&'a str>,
    pinned: &'a [hermes_memory::LoadedMemory],
    active: &'a [hermes_memory::LoadedMemory],
    all_skills: &'a [hermes_skills::LoadedSkill],
    open_work: &'a [hermes_commitments::Commitment],
    material_hits: &'a [hermes_core::MaterialHit],
    first_human_today: bool,
    workspace_root: &'a str,
    limits: hermes_llm::ContextLimits,
) -> ContextSources<'a> {
    ContextSources {
        base: None,
        persona: crate::commands::personas::bound_persona(
            active_session.session.meta.persona.as_deref(),
        ),
        roster,
        topic_cards,
        pinned,
        active,
        all_skills,
        open_work,
        material_hits,
        first_human_today,
        workspace_root,
        limits,
    }
}

/// 这一轮**看得见**的记忆：`(active, 其中的 pinned)`。
///
/// 装配只走 `hermes_memory::views_for` 一处（与 CLI 同一个函数）——GUI 曾经在这里
/// 直接把全量记忆喂进提示词，于是小谢的会话看得见王海燕的记忆，而主题卡那边却是
/// 按工位切过的：同一轮里两条轴不一致，是明确的隔离失效。
/// 单独成函数是为了让「按人物切」这条接线可测：`owner` 写成 `None` 必须变红。
fn visible_memories(
    all: &[hermes_memory::LoadedMemory],
    owner: Option<&str>,
) -> (
    Vec<hermes_memory::LoadedMemory>,
    Vec<hermes_memory::LoadedMemory>,
) {
    hermes_memory::views_for(all, owner)
}

fn parse_zaiban_tool(name: &str, content: &str, is_error: bool) -> Option<ChatStreamEvent> {
    if is_error || !name.starts_with("commitment_") {
        return None;
    }
    if content.starts_with("Near existing") {
        let existing_id = bracket_id(content)?;
        let existing_title = content
            .split('「')
            .nth(1)
            .and_then(|s| s.split('」').next())
            .map(|s| s.to_string());
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: "near".into(),
            id: None,
            title: None,
            existing_id: Some(existing_id),
            existing_title,
        });
    }
    if content.starts_with("Recorded open work") || content.starts_with("Folded into") {
        let id = bracket_id(content);
        let title = content
            .split("]: ")
            .nth(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let action = if content.starts_with("Folded") {
            "folded"
        } else {
            "saved"
        };
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: action.into(),
            id,
            title,
            existing_id: None,
            existing_title: None,
        });
    }
    if content.starts_with("Closed")
        || content.starts_with("Dropped")
        || content.starts_with("Updated")
        || content.starts_with("Split")
    {
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: "changed".into(),
            id: bracket_id(content),
            title: None,
            existing_id: None,
            existing_title: None,
        });
    }
    Some(ChatStreamEvent::ZaibanUpdated {
        action: "changed".into(),
        id: None,
        title: None,
        existing_id: None,
        existing_title: None,
    })
}

fn bracket_id(s: &str) -> Option<String> {
    let start = s.find('[')? + 1;
    let rest = s.get(start..)?;
    let end = rest.find(']')?;
    let id = rest[..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Drop messages after the last human user turn (keeps that user message).
/// Used before `regenerate_turn`.
#[tauri::command]
pub async fn truncate_after_last_user(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<usize, GuiError> {
    let mut sessions = state.sessions.lock().await;
    let active = sessions
        .get_mut(&session_id)
        .ok_or_else(|| GuiError::Session("session not found".into()))?;

    let keep = end_after_last_human_user(&active.session.messages)
        .ok_or_else(|| GuiError::Session("no user message to regenerate from".into()))?;

    if keep >= active.session.messages.len() {
        // Nothing after last user — still ok (empty assistant).
        return Ok(keep);
    }

    active.session.messages.truncate(keep);
    active.writer = None;
    hermes_store::rewrite_session(&active.path, &active.session)
        .map_err(|e| GuiError::Session(e.to_string()))?;
    active.writer = Some(
        SessionWriter::open_append(&active.path).map_err(|e| GuiError::Session(e.to_string()))?,
    );
    Ok(keep)
}

/// Keep only the first `keep_count` raw messages (0..=len). Used for edit-resend.
#[tauri::command]
pub async fn truncate_session(
    state: State<'_, AppState>,
    session_id: String,
    keep_count: usize,
) -> Result<(), GuiError> {
    let mut sessions = state.sessions.lock().await;
    let active = sessions
        .get_mut(&session_id)
        .ok_or_else(|| GuiError::Session("session not found".into()))?;

    if keep_count > active.session.messages.len() {
        return Err(GuiError::Session(format!(
            "keep_count {keep_count} > message len {}",
            active.session.messages.len()
        )));
    }

    // No disk file yet (empty draft) — memory only.
    if active.writer.is_none() && !active.path.exists() {
        active.session.messages.truncate(keep_count);
        return Ok(());
    }

    active.session.messages.truncate(keep_count);
    active.writer = None;
    hermes_store::rewrite_session(&active.path, &active.session)
        .map_err(|e| GuiError::Session(e.to_string()))?;
    if active.path.exists() {
        active.writer = Some(
            SessionWriter::open_append(&active.path)
                .map_err(|e| GuiError::Session(e.to_string()))?,
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_stream(state: State<'_, AppState>, session_id: String) -> Result<(), GuiError> {
    if let Some(tx) = state.cancel_tokens.lock().await.remove(&session_id) {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command]
pub async fn respond_confirm(
    state: State<'_, AppState>,
    id: String,
    action: String,
    tool_name: Option<String>,
    reason: Option<String>,
) -> Result<(), GuiError> {
    let parsed = match action.to_lowercase().as_str() {
        "allow" | "y" => ConfirmAction::Allow,
        "alwaysallow" | "always_allow" => {
            if let Some(name) = &tool_name {
                // Persist so it survives a restart; a write failure only means
                // the user gets asked again next launch, never that this call
                // is blocked.
                if let Err(e) = crate::commands::config::remember_allow_rule(&state, name) {
                    eprintln!("could not persist allow rule for {name}: {e}");
                }
            }
            ConfirmAction::AlwaysAllow
        }
        _ => ConfirmAction::Deny {
            reason: reason.filter(|s| !s.trim().is_empty()),
        },
    };
    if let Some(reply) = state.confirm_tokens.lock().await.remove(&id) {
        let _ = reply.send(parsed);
    }
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
            path: std::path::PathBuf::from("/tmp/lebi-gui-turn-sources.jsonl"),
        }
    }

    fn mem(owner: Option<&str>, body: &str, pinned: bool) -> hermes_memory::LoadedMemory {
        let mut fm = hermes_memory::MemoryFrontmatter::new(
            hermes_memory::Source::User,
            hermes_memory::Confidence::High,
            vec![],
            "general".into(),
        )
        .owned(owner.map(str::to_string));
        fm.pinned = pinned;
        hermes_memory::LoadedMemory {
            frontmatter: fm,
            body: body.into(),
            source_path: std::path::PathBuf::from("/nowhere"),
            scope: hermes_memory::Scope::User,
        }
    }

    /// 提示词里那份记忆必须**按工位切过**：只留「全局 + 本人物」。
    /// 把 `visible_memories` 的 owner 写成 `None`（GUI 曾经就是这样直接把全量喂进去），
    /// 这条必须红。
    #[test]
    fn visible_memories_keeps_globals_and_own_only() {
        let all = vec![
            mem(None, "全局：交付一律给 Word 放桌面", true),
            mem(Some("xiao-xie"), "自己的：林碳报告只引 IEA", false),
            mem(Some("wang-hai-yan"), "别人的：标题不夸张", true),
        ];

        let (view, pinned) = visible_memories(&all, Some("xiao-xie"));
        assert_eq!(
            view.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            vec!["全局：交付一律给 Word 放桌面", "自己的：林碳报告只引 IEA"],
            "人物会话只拿「全局 + 本人物」"
        );
        assert!(
            !view
                .iter()
                .any(|m| m.frontmatter.owner.as_deref() == Some("wang-hai-yan")),
            "别的人物的记忆一条都不许进提示词"
        );
        assert_eq!(pinned.len(), 1, "别人的 pinned 不许借「置顶」溜进来");
        assert_eq!(pinned[0].frontmatter.owner, None);

        let (globals, _) = visible_memories(&all, None);
        assert_eq!(globals.len(), 1, "无人物 / 自带角色 → 只看全局");
    }

    fn system_prompt(persona: Option<&str>) -> String {
        let active = draft(persona);
        // 名册给两个人，其中一个是「我」——「我」不许出现在自己的名单里。
        let roster = [
            hermes_core::persona::get("xiao-xie").unwrap(),
            hermes_core::persona::get("yu-tian").unwrap(),
        ];
        turn_sources(
            &active,
            &roster,
            None,
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
            "/tmp/ws",
            hermes_llm::ContextLimits::default(),
        )
        .build_turn_system("你好")
    }

    #[test]
    fn a_bound_persona_reaches_the_turn_prompt() {
        let bound = system_prompt(Some("xiao-xie"));
        assert!(bound.contains("## 你现在是谁"), "{bound}");
        assert!(
            bound.contains("林碳小谢"),
            "人物块必须带上这位人物的身份：{bound}"
        );
        assert!(
            bound.contains("- 编辑雨天 · 错在哪，我指给你"),
            "指路名册必须进提示词，且写成侧栏那行的样子：{bound}"
        );
        assert!(
            !bound.contains("- 林碳小谢 · "),
            "名册是「其他工位」，不该把自己也列进去：{bound}"
        );

        let free = system_prompt(None);
        assert!(!free.contains("## 你现在是谁"), "{free}");

        let unknown = system_prompt(Some("who-is-this"));
        assert!(
            !unknown.contains("## 你现在是谁"),
            "不认识的 id 不该凭空长出一个人物块：{unknown}"
        );
    }
}
