//! Chat WebSocket handler — the heart of the Flutter client backend.
//!
//! One persistent WS connection per client. Upstream frames (text):
//!   `{"type":"send","sessionId","content"}`    — start a turn
//!   `{"type":"cancel","sessionId"}`            — cancel the running turn
//!   `{"type":"confirm","id","action",...}`     — resolve a dangerous-tool prompt
//! Downstream frames are serialized [`ChatStreamEvent`]s. Blueprint:
//! `hermes-gui/src/commands/chat.rs`, with `ToolExecStart` keeping `input`.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use hermes_core::{Session, SessionEvent, SessionMeta};
use hermes_memory::MemoryStore;
use hermes_turn::{ConfirmAction, ConfirmRequest, TurnConfig, TurnEvent};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::context::ContextSources;
use crate::events::ChatStreamEvent;
use crate::state::{session_path_for, ActiveSession, AppState};

/// One client → server frame.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientFrame {
    Send {
        #[serde(rename = "sessionId")]
        session_id: String,
        content: String,
        #[serde(default)]
        attachments: Vec<Attachment>,
    },
    Cancel {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Confirm {
        id: String,
        action: String,
        #[serde(rename = "toolName")]
        tool_name: Option<String>,
        reason: Option<String>,
    },
}

/// One image attachment on a `send` frame (base64, no `data:` prefix).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attachment {
    media_type: String,
    data: String,
}

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| chat(socket, state))
}

async fn chat(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    // Connection-level outbound queue: turn tasks push serialized events here,
    // a single forwarder drains it into the WS sink.
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<Message>();
    let send_task = tokio::spawn(async move {
        while let Some(msg) = ws_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                let frame = match serde_json::from_str::<ClientFrame>(&text) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = ws_tx.send(Message::Text(
                            serde_json::to_string(&ChatStreamEvent::Error {
                                message: format!("bad frame: {e}"),
                            })
                            .unwrap_or_default()
                            .into(),
                        ));
                        continue;
                    }
                };
                match frame {
                    ClientFrame::Send {
                        session_id,
                        content,
                        attachments,
                    } => {
                        handle_send(
                            state.clone(),
                            session_id,
                            content,
                            attachments,
                            ws_tx.clone(),
                        )
                        .await
                    }
                    ClientFrame::Cancel { session_id } => {
                        handle_cancel(state.clone(), session_id).await
                    }
                    ClientFrame::Confirm {
                        id,
                        action,
                        tool_name,
                        reason,
                    } => handle_confirm(state.clone(), id, action, tool_name, reason).await,
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    drop(ws_tx);
    let _ = send_task.await;
}

/// `send` → look up / create the session, append the user message, spawn a
/// turn that streams `ChatStreamEvent`s back over the connection.
#[allow(clippy::too_many_arguments)]
async fn handle_send(
    state: Arc<AppState>,
    session_id: String,
    content: String,
    attachments: Vec<Attachment>,
    ws_tx: mpsc::UnboundedSender<Message>,
) {
    let provider = state.provider.read().unwrap().clone();
    let host = state.host.clone();
    let model = state.model();
    let max_tokens = state.max_tokens();
    let tools = state.tools.lock().await.clone();
    let skills = state.skills.lock().await.clone();
    let pinned = state.pinned_memories.lock().await.clone();
    let active = state.active_memories.lock().await.clone();
    let workspace_root = state.workspace_root();
    let (limits, provider_name) = {
        let cfg = state.config.read().unwrap();
        (cfg.limits, cfg.default_provider.clone())
    };

    let mut allow_rules: Vec<String> = state.config.read().unwrap().permissions.allow.clone();
    let deny_rules: Vec<String> = state.config.read().unwrap().permissions.deny.clone();
    allow_rules.extend(state.always_allowed_tools.lock().await.iter().cloned());
    let permissions = hermes_turn::PermissionChecker::new(&allow_rules, &deny_rules);

    let (history, turn_system) = {
        let mut sessions = state.sessions.lock().await;
        let active_session = if let Some(s) = sessions.get_mut(&session_id) {
            s
        } else {
            let meta = SessionMeta::new(model.clone(), provider_name.clone());
            let path = match session_path_for(&meta) {
                Ok(p) => p,
                Err(e) => {
                    let _ = ws_tx.send(Message::Text(
                        serde_json::to_string(&ChatStreamEvent::Error {
                            message: format!("session path: {e}"),
                        })
                        .unwrap_or_default()
                        .into(),
                    ));
                    return;
                }
            };
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let session = Session {
                meta,
                messages: Vec::new(),
                total_input_tokens: 0,
                total_output_tokens: 0,
            };
            sessions.insert(
                session_id.clone(),
                ActiveSession {
                    session,
                    writer: None,
                    path,
                },
            );
            sessions.get_mut(&session_id).unwrap()
        };

        let sources = ContextSources {
            base: None,
            pinned: &pinned,
            active: &active,
            all_skills: &skills,
            workspace_root: &workspace_root,
            limits,
        };
        let turn_system = sources.build_turn_system(&content);

        let user_msg = hermes_core::Message {
            role: hermes_core::Role::User,
            content: std::iter::once(hermes_core::ContentBlock::Text {
                text: content.clone(),
            })
            .chain(
                attachments
                    .iter()
                    .map(|a| hermes_core::ContentBlock::Image {
                        source: hermes_core::ImageSource {
                            kind: "base64".into(),
                            media_type: a.media_type.clone(),
                            data: a.data.clone(),
                        },
                    }),
            )
            .collect(),
        };
        active_session.session.messages.push(user_msg.clone());
        if let Ok(w) = active_session.ensure_writer() {
            let _ = w.append(&SessionEvent::Message(user_msg));
        }

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
                    || is_trivial_user_text(current));
            if should_write && derived != DEFAULT_SESSION_TITLE {
                active_session.session.meta.title = Some(derived.clone());
                let path = active_session.path.clone();
                let _ = hermes_store::update_session_title(&path, &derived);
                if let Ok(w) = hermes_store::SessionWriter::open_append(&path) {
                    active_session.writer = Some(w);
                }
            }
        }

        (active_session.session.messages.clone(), turn_system)
    };
    // Drop the session guard before spawning the turn.

    // Snapshot history for the `propose_skill` tool to read mid-turn.
    if let Ok(mut guard) = state.propose_messages.write() {
        *guard = history.clone();
    }

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state
        .cancel_tokens
        .lock()
        .await
        .insert(session_id.clone(), cancel_tx);

    let (confirm_tx, mut confirm_rx) = mpsc::channel::<ConfirmRequest>(8);
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
    let auto_accept = state.config.read().unwrap().reflect.auto_accept_memories;
    let min_confidence: hermes_memory::Confidence = state
        .config
        .read()
        .unwrap()
        .reflect
        .auto_accept_min_confidence
        .parse()
        .unwrap_or(hermes_memory::Confidence::Medium);
    let out = ws_tx.clone();
    let persist_thinking = hermes_llm::Config::load_default()
        .map(|c| c.ui.persist_thinking)
        .unwrap_or(state.config.read().unwrap().ui.persist_thinking);

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

        let out_for_event = out.clone();
        let on_turn_event = move |event: TurnEvent| {
            let cs = match event {
                TurnEvent::TextDelta(text) => ChatStreamEvent::TextDelta { text },
                TurnEvent::ThinkingDelta(text) => ChatStreamEvent::ThinkingDelta { text },
                TurnEvent::ToolUseStart { id, name } => ChatStreamEvent::ToolUseStart { id, name },
                // Keep `input` (the GUI drops it) so the client can render
                // the tool-call parameters, not just the summary.
                TurnEvent::ToolExecStart {
                    id,
                    name,
                    summary,
                    input,
                } => ChatStreamEvent::ToolExecStart {
                    id,
                    name,
                    summary,
                    input,
                },
                TurnEvent::ToolConfirmPending {
                    id,
                    tool_name,
                    summary,
                    reason,
                } => ChatStreamEvent::ConfirmRequired {
                    id,
                    tool_name,
                    summary,
                    reason,
                },
                TurnEvent::ToolUseResult {
                    id,
                    content,
                    is_error,
                } => ChatStreamEvent::ToolUseResult {
                    id,
                    content,
                    is_error,
                },
                TurnEvent::Usage {
                    input_tokens,
                    output_tokens,
                    ..
                } => ChatStreamEvent::UsageUpdate {
                    input_tokens,
                    output_tokens,
                },
                TurnEvent::Error(message) => ChatStreamEvent::Error {
                    message: hermes_llm::humanize_error(&message),
                },
                TurnEvent::Cancelled => ChatStreamEvent::Error {
                    // Server protocol reuses Error for stop; clients may map "cancelled".
                    message: "cancelled".into(),
                },
                TurnEvent::Done => ChatStreamEvent::Done,
            };
            push_event(&out_for_event, cs);
        };

        // Bridge: receive ConfirmRequest from the turn loop, store the
        // oneshot reply sender by tool-use id so `handle_confirm` resolves it.
        let ct = confirm_tokens_arc.clone();
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
                push_event(
                    &out,
                    ChatStreamEvent::Error {
                        message: format!("{e:#}"),
                    },
                );
            }
        }

        // Drain any skill candidates the `propose_skill` tool queued.
        let drained: Vec<hermes_reflect::SkillCandidate> = match propose_queue.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        for c in drained {
            push_event(
                &out,
                ChatStreamEvent::SkillCandidateProposed {
                    name: c.name,
                    description: c.description,
                    body: c.body,
                    triggers: c.triggers,
                },
            );
        }

        // Micro-reflection: shared pipeline; result over WS (Flutter has no Tauri events).
        if !turn_messages.is_empty() {
            let out_micro = out.clone();
            let prov = provider_for_micro;
            let ms = memory_store;
            let skills_snap = skills_for_micro;
            let mems_snap = memories_for_micro;
            let active_arc = active_memories_arc;
            let pinned_arc = pinned_memories_arc;
            let cooldown = micro_cooldown;
            let session_key = sid.clone();
            let sess_log = session_id_for_log;
            tokio::spawn(async move {
                let turns_since = {
                    let map = cooldown.lock().await;
                    *map.get(&session_key).unwrap_or(&0)
                };
                let apply = hermes_reflect::MicroApplyConfig::new(
                    sess_log,
                    auto_accept,
                    min_confidence,
                    false,
                );
                let outcome =
                    hermes_reflect::run_micro_after_turn(hermes_reflect::MicroRunRequest {
                        provider: prov.as_ref(),
                        store: ms.as_ref(),
                        turn_messages: &turn_messages,
                        skills: &skills_snap,
                        memories: &mems_snap,
                        turns_since_last: turns_since,
                        apply,
                        recompile_on_auto_accept: true,
                    })
                    .await;

                {
                    let mut map = cooldown.lock().await;
                    let entry = map.entry(session_key).or_insert(0);
                    match &outcome {
                        Ok(o) => hermes_reflect::update_cooldown_after(o, entry),
                        Err(_) => *entry = 0,
                    }
                }

                let applied = match outcome {
                    Ok(hermes_reflect::MicroRunOutcome::Applied(a)) => a,
                    Ok(_) => return,
                    Err(e) => {
                        tracing::debug!(error=%e, "server micro-reflection failed");
                        return;
                    }
                };

                if applied.auto_accepted > 0 {
                    if let Ok(fresh) = ms.list_active() {
                        let pinned: Vec<_> = fresh
                            .iter()
                            .filter(|m| m.frontmatter.pinned)
                            .cloned()
                            .collect();
                        *active_arc.lock().await = fresh;
                        *pinned_arc.lock().await = pinned;
                    }
                }

                // Persist pending candidates into the shared pending-review
                // inbox (Micro source) so Flutter-originated evolution survives
                // restarts and can be reviewed on the desktop GUI — mirrors
                // `hermes-gui/src/commands/micro.rs`.
                if applied.has_pending() {
                    let pending_for_inbox = applied.pending_as_output();
                    match hermes_reflect::enqueue_from_reflection(
                        &pending_for_inbox,
                        hermes_reflect::InboxSource::Micro,
                    ) {
                        Ok(added) => {
                            if added > 0 {
                                tracing::info!(added, "server micro pending enqueued to inbox");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error=%e, "enqueue server micro pending to inbox failed")
                        }
                    }
                }

                if applied.auto_accepted == 0 && !applied.has_pending() {
                    return;
                }

                let pending = applied.pending_as_output();
                let reflection = if applied.has_pending() {
                    Some(crate::events::MicroReflectionPayload::from_output(&pending))
                } else {
                    None
                };
                let summary = if applied.summary.is_empty() {
                    "Micro-reflection complete".into()
                } else {
                    applied.summary.clone()
                };
                push_event(
                    &out_micro,
                    ChatStreamEvent::MicroReflection {
                        summary,
                        memory_count: applied.pending_memory_count(),
                        skill_count: applied.pending_skill_count(),
                        auto_accepted: applied.auto_accepted,
                        reflection,
                    },
                );
            });
        }

        cancel_tokens_arc.lock().await.remove(&sid);
    });
}

async fn handle_cancel(state: Arc<AppState>, session_id: String) {
    if let Some(tx) = state.cancel_tokens.lock().await.remove(&session_id) {
        let _ = tx.send(());
    }
}

async fn handle_confirm(
    state: Arc<AppState>,
    id: String,
    action: String,
    tool_name: Option<String>,
    reason: Option<String>,
) {
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
}

/// Serialize a [`ChatStreamEvent`] and push it to the connection's outbound queue.
fn push_event(out: &mpsc::UnboundedSender<Message>, event: ChatStreamEvent) {
    if let Ok(json) = serde_json::to_string(&event) {
        let _ = out.send(Message::Text(json.into()));
    }
}
