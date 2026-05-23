use hermes_core::{Session, SessionEvent, SessionMeta};
use hermes_store::SessionWriter;
use hermes_turn::{ConfirmAction, TurnConfig, TurnEvent};
use tauri::ipc::Channel;
use tauri::State;

use crate::context::ContextSources;
use crate::error::GuiError;
use crate::events::ChatStreamEvent;
use crate::state::{session_path_for, ActiveSession, AppState};

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    content: String,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    let provider = state.provider.clone();
    let host = state.host.clone();
    let model = state.model().to_string();
    let max_tokens = state.max_tokens();
    let tools = state.tools.lock().await.clone();
    let skills = state.skills.lock().await.clone();
    let pinned = state.pinned_memories.lock().await.clone();
    let active = state.active_memories.lock().await.clone();
    let workspace_root = state.workspace_root();
    let limits = state.config.limits;

    // Build the per-turn permission checker by merging the persistent
    // `[permissions]` config with the session-scoped "Always Allow" list
    // that grows as the user approves dangerous tools in the UI.
    let mut allow_rules: Vec<String> = state.config.permissions.allow.clone();
    allow_rules.extend(state.always_allowed_tools.lock().await.iter().cloned());
    let permissions =
        hermes_turn::PermissionChecker::new(&allow_rules, &state.config.permissions.deny);

    let mut sessions = state.sessions.lock().await;
    let active_session = if let Some(s) = sessions.get_mut(&session_id) {
        s
    } else {
        let meta = SessionMeta::new(model.clone(), "anthropic".to_string());
        let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GuiError::Session(e.to_string()))?;
        }
        let mut writer =
            SessionWriter::create(&path).map_err(|e| GuiError::Session(e.to_string()))?;
        writer
            .append(&SessionEvent::Meta(meta.clone()))
            .map_err(|e| GuiError::Session(e.to_string()))?;
        let session = Session {
            meta,
            messages: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        sessions.insert(session_id.clone(), ActiveSession {
            session,
            writer,
            path,
        });
        sessions.get_mut(&session_id).unwrap()
    };

    let sources = ContextSources {
        base: None,
        pinned: &pinned,
        active: &active,
        all_skills: &skills,
        tools: &tools,
        workspace_root: &workspace_root,
        limits,
    };
    let turn_system = sources.build_turn_system(&content);

    let user_msg = active_session.session.push_user(&content).clone();
    let _ = active_session
        .writer
        .append(&SessionEvent::Message(user_msg));

    let history = active_session.session.messages.clone();
    drop(sessions);

    // Snapshot for the propose_skill tool to read mid-turn. Must be set
    // before run_turn fires because the tool may be invoked during it.
    if let Ok(mut guard) = state.propose_messages.write() {
        *guard = history.clone();
    }

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state
        .cancel_tokens
        .lock()
        .await
        .insert(session_id.clone(), cancel_tx);

    // Confirmation channel for dangerous tool calls.
    let (confirm_tx, mut confirm_rx) =
        tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);
    let confirm_tokens_arc = state.confirm_tokens.clone();

    let sid = session_id.clone();
    let sessions_arc = state.sessions.clone();
    let cancel_tokens_arc = state.cancel_tokens.clone();
    let propose_queue = state.propose_queue.clone();

    tokio::spawn(async move {
        let config = TurnConfig {
            model,
            system: if turn_system.is_empty() { None } else { Some(turn_system) },
            max_tokens,
            max_tool_rounds: limits.max_tool_rounds,
            permissions,
        };

        let evt = on_event.clone();
        let confirm_tokens = confirm_tokens_arc.clone();
        let on_turn_event = move |event: TurnEvent| {
            match event {
                TurnEvent::TextDelta(text) => {
                    let _ = evt.send(ChatStreamEvent::TextDelta { text });
                }
                TurnEvent::ThinkingDelta(text) => {
                    let _ = evt.send(ChatStreamEvent::ThinkingDelta { text });
                }
                TurnEvent::ToolUseStart { id, name } => {
                    let _ = evt.send(ChatStreamEvent::ToolUseStart { id, name });
                }
                TurnEvent::ToolExecStart { id, name, summary } => {
                    let _ = evt.send(ChatStreamEvent::ToolExecStart { id, name, summary });
                }
                TurnEvent::ToolConfirmPending { id, tool_name, summary } => {
                    // The confirm_rx bridge below will pick up the full
                    // ConfirmRequest and store the reply sender. Here we
                    // just notify the frontend that a confirmation is needed.
                    let _ = evt.send(ChatStreamEvent::ConfirmRequired {
                        id,
                        tool_name,
                        summary,
                    });
                }
                TurnEvent::ToolUseResult { id, content, is_error } => {
                    let _ = evt.send(ChatStreamEvent::ToolUseResult { id, content, is_error });
                }
                TurnEvent::Usage { input_tokens, output_tokens } => {
                    let _ = evt.send(ChatStreamEvent::UsageUpdate { input_tokens, output_tokens });
                }
                TurnEvent::Error(message) => {
                    let _ = evt.send(ChatStreamEvent::Error { message });
                }
                TurnEvent::Done => {
                    let _ = evt.send(ChatStreamEvent::Done);
                }
            }
        };

        // Bridge: receive ConfirmRequest from the turn loop, store the
        // oneshot reply sender by tool-use-id so respond_confirm can
        // look it up later.
        let ct = confirm_tokens.clone();
        let confirm_bridge = tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                ct.lock().await.insert(req.id, req.reply);
            }
        });

        let result = hermes_turn::run_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &history,
            &config,
            Some(confirm_tx),
            on_turn_event,
            cancel_rx,
        )
        .await;

        confirm_bridge.abort();

        match result {
            Ok(output) => {
                if let Some(s) = sessions_arc.lock().await.get_mut(&sid) {
                    for msg in &output.new_messages {
                        s.session.messages.push(msg.clone());
                        let _ = s.writer.append(&SessionEvent::Message(msg.clone()));
                    }
                    s.session.total_input_tokens += output.usage.input_tokens;
                    s.session.total_output_tokens += output.usage.output_tokens;
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "turn failed");
            }
        }

        // Drain any skill candidates the `propose_skill` tool queued during
        // the turn; surface them to the frontend so the approval UI can
        // run them through the same modal as `/reflect` candidates.
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

        cancel_tokens_arc.lock().await.remove(&sid);
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_stream(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), GuiError> {
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
                state
                    .always_allowed_tools
                    .lock()
                    .await
                    .insert(name.clone());
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
