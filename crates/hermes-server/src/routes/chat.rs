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
use hermes_store::SessionWriter;
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
                    } => {
                        handle_confirm(state.clone(), id, action, tool_name, reason).await
                    }
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
    let provider_name = state.config.default_provider.clone();

    let mut allow_rules: Vec<String> = state.config.permissions.allow.clone();
    allow_rules.extend(state.always_allowed_tools.lock().await.iter().cloned());
    let permissions =
        hermes_turn::PermissionChecker::new(&allow_rules, &state.config.permissions.deny);

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
            let mut writer = match SessionWriter::create(&path) {
                Ok(w) => w,
                Err(e) => {
                    let _ = ws_tx.send(Message::Text(
                        serde_json::to_string(&ChatStreamEvent::Error {
                            message: format!("session writer: {e}"),
                        })
                        .unwrap_or_default()
                        .into(),
                    ));
                    return;
                }
            };
            let _ = writer.append(&SessionEvent::Meta(meta.clone()));
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
                    writer,
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
            tools: &tools,
            workspace_root: &workspace_root,
            limits,
        };
        let turn_system = sources.build_turn_system(&content);

        let user_msg = hermes_core::Message {
            role: hermes_core::Role::User,
            content: std::iter::once(hermes_core::ContentBlock::Text { text: content.clone() })
                .chain(attachments.iter().map(|a| hermes_core::ContentBlock::Image {
                    source: hermes_core::ImageSource {
                        kind: "base64".into(),
                        media_type: a.media_type.clone(),
                        data: a.data.clone(),
                    },
                }))
                .collect(),
        };
        active_session.session.messages.push(user_msg.clone());
        let _ = active_session
            .writer
            .append(&SessionEvent::Message(user_msg));
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
    let out = ws_tx.clone();

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
                TurnEvent::ToolUseStart { id, name } => {
                    ChatStreamEvent::ToolUseStart { id, name }
                }
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
                } => ChatStreamEvent::ConfirmRequired {
                    id,
                    tool_name,
                    summary,
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
                } => ChatStreamEvent::UsageUpdate {
                    input_tokens,
                    output_tokens,
                },
                TurnEvent::Error(message) => ChatStreamEvent::Error { message },
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
                push_event(&out, ChatStreamEvent::Error { message: format!("{e:#}") });
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
}

/// Serialize a [`ChatStreamEvent`] and push it to the connection's outbound queue.
fn push_event(out: &mpsc::UnboundedSender<Message>, event: ChatStreamEvent) {
    if let Ok(json) = serde_json::to_string(&event) {
        let _ = out.send(Message::Text(json.into()));
    }
}
