use hermes_core::{Message, Role, Session, SessionEvent, SessionMeta};
use hermes_llm::Config;
use hermes_memory::MemoryStore;
use hermes_store::SessionWriter;
use hermes_turn::{ConfirmAction, TurnConfig, TurnEvent};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    new_user: Option<String>,
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
    let skills = state.skills.lock().await.clone();
    // Refresh memory cache from disk so mid-session memory_save is visible next turn.
    let (pinned, active) = {
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
        *state.pinned_memories.lock().await = pinned.clone();
        (pinned, all)
    };
    let workspace_root = state.workspace_root();
    let (limits, default_provider) = {
        let cfg = state.config.read().unwrap();
        (cfg.limits, cfg.default_provider.clone())
    };

    let mut allow_rules: Vec<String> = state.config.read().unwrap().permissions.allow.clone();
    let deny_rules: Vec<String> = state.config.read().unwrap().permissions.deny.clone();
    allow_rules.extend(state.always_allowed_tools.lock().await.iter().cloned());
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

    let open_work = state.commitment_store.list_live().unwrap_or_default();
    let first_human_today = {
        let (seq, at) = active_session.session.last_human_send();
        let today = chrono::Utc::now().date_naive();
        seq == 0 || at.map(|t| t.date_naive() < today).unwrap_or(true)
    };
    let sources = ContextSources {
        base: None,
        pinned: &pinned,
        active: &active,
        all_skills: &skills,
        open_work: &open_work,
        first_human_today,
        workspace_root: &workspace_root,
        limits,
    };
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
    let history =
        hermes_core::sanitize_history_for_provider(&active_session.session.messages);
    // Keep in-memory session aligned so subsequent turns stay clean.
    active_session.session.messages = history.clone();
    drop(sessions);

    if let Ok(mut guard) = state.propose_messages.write() {
        *guard = history.clone();
    }

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
    let persist_thinking = Config::load_default()
        .map(|c| c.ui.persist_thinking)
        .unwrap_or(state.config.read().unwrap().ui.persist_thinking);
    let ui_lang = state
        .config
        .read()
        .map(|c| c.ui.language.clone())
        .unwrap_or_else(|_| "zh-CN".into());

    tokio::spawn(async move {
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
        let tool_names: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let tool_names_ev = tool_names.clone();
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
                let _ = evt.send(ChatStreamEvent::ToolExecStart {
                    id,
                    name,
                    summary,
                });
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

        match result {
            Ok(output) => {
                turn_messages = output.new_messages.clone();
                if let Some(s) = sessions_arc.lock().await.get_mut(&sid) {
                    session_id_for_log = s.session.meta.id.clone();
                    for msg in &output.new_messages {
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
        for c in drained {
            let _ = on_event.send(ChatStreamEvent::SkillCandidateProposed {
                name: c.name,
                description: c.description,
                body: c.body,
                triggers: c.triggers,
            });
        }

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
            auto_accept,
            min_confidence,
        );

        cancel_tokens_arc.lock().await.remove(&sid);
    });

    Ok(())
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
                state.always_allowed_tools.lock().await.insert(name.clone());
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
