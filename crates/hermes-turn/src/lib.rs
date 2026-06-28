//! Shared agent turn execution loop.
//!
//! All frontends (CLI, GUI) call `run_turn()` and consume the
//! resulting `TurnEvent` stream. This eliminates duplicated streaming
//! and tool-execution logic.

use futures::StreamExt;
use hermes_core::{
    CompletionRequest, ContentBlock, LlmProvider, Message, Role, StopReason, StreamEvent,
    ToolCallOutcome, ToolHost, ToolSpec, Usage,
};
use tokio::sync::{mpsc, oneshot};

pub mod agent;
pub mod permissions;

pub use agent::{AgentConfig, AgentEvent, AgentOutput, run_agent};
pub use permissions::{Permission, PermissionChecker};

const DEFAULT_MAX_TOOL_ROUNDS: usize = 25;

/// User's decision on a dangerous tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Allow,
    /// Allow this and all future calls to the same tool name.
    AlwaysAllow,
    /// Deny, with an optional reason/suggestion fed back to the agent.
    Deny { reason: Option<String> },
}

/// Request sent from the turn loop to the frontend for approval.
pub struct ConfirmRequest {
    pub id: String,
    pub tool_name: String,
    pub summary: String,
    pub reply: oneshot::Sender<ConfirmAction>,
}

/// Returns true if the tool with the given `name` requires user
/// confirmation before execution. Looks up the `requires_confirmation`
/// flag in the provided `tools` slice; unknown tool names default to
/// `true` (fail-safe — better to over-prompt than skip a side-effect).
pub fn is_dangerous_tool(name: &str, tools: &[ToolSpec]) -> bool {
    tools
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.requires_confirmation)
        .unwrap_or(true)
}

/// Produce a human-readable one-liner summarizing what a tool call will do.
pub fn tool_call_summary(name: &str, input: &serde_json::Value) -> String {
    let key_field = match name {
        "bash" => "command",
        "read" | "write" | "edit" => "path",
        "git" => "operation",
        "web_fetch" => "url",
        "web_search" => "query",
        "memory_search" => "query",
        "memory_save" => "content",
        "memory_delete" => "id",
        _ => "",
    };

    if !key_field.is_empty() {
        if let Some(val) = input.get(key_field).and_then(|v| v.as_str()) {
            let truncated: String = val.chars().take(120).collect();
            return format!("{name}: {truncated}");
        }
    }

    // Fallback: truncated JSON
    let s = input.to_string();
    let truncated: String = s.chars().take(120).collect();
    format!("{name}: {truncated}")
}

/// Pair any tool_use blocks in the last assistant message that aren't yet
/// covered by `tool_results`, by appending error-flagged "cancelled"
/// placeholders, then push the resulting User message onto `messages`.
///
/// Used by cancel paths so we never persist an assistant message containing
/// orphan tool_use blocks — the Anthropic API rejects such histories with
/// "tool_use ids were found without tool_result blocks immediately after".
fn flush_with_cancel_pairing(
    messages: &mut Vec<Message>,
    mut tool_results: Vec<ContentBlock>,
) {
    let collected: std::collections::HashSet<String> = tool_results
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect();
    let unpaired: Vec<String> = match messages.last() {
        Some(m) if m.role == Role::Assistant => m
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, .. } if !collected.contains(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    for id in unpaired {
        tool_results.push(ContentBlock::ToolResult {
            tool_use_id: id,
            content: "Tool call cancelled.".to_string(),
            is_error: true,
        });
    }
    if !tool_results.is_empty() {
        messages.push(Message {
            role: Role::User,
            content: tool_results,
        });
    }
}

#[derive(Debug, Clone)]
pub enum TurnEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStart { id: String, name: String },
    /// Emitted right before tool execution, when input is fully known.
    ToolExecStart { id: String, name: String, summary: String, input: serde_json::Value },
    ToolUseResult { id: String, content: String, is_error: bool },
    ToolConfirmPending { id: String, tool_name: String, summary: String },
    Usage { input_tokens: u32, output_tokens: u32 },
    Error(String),
    Done,
}

#[derive(Debug, Clone)]
pub struct TurnConfig {
    pub model: String,
    pub system: Option<String>,
    pub max_tokens: u32,
    pub max_tool_rounds: usize,
    pub permissions: PermissionChecker,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            system: None,
            max_tokens: 16_384,
            max_tool_rounds: DEFAULT_MAX_TOOL_ROUNDS,
            permissions: PermissionChecker::default(),
        }
    }
}

pub struct TurnOutput {
    pub new_messages: Vec<Message>,
    pub usage: Usage,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_turn<F>(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[ToolSpec],
    history: &[Message],
    config: &TurnConfig,
    confirm_tx: Option<mpsc::Sender<ConfirmRequest>>,
    on_event: F,
    mut cancel: oneshot::Receiver<()>,
) -> hermes_core::Result<TurnOutput>
where
    F: Fn(TurnEvent) + Send + Sync,
{
    let mut messages: Vec<Message> = history.to_vec();
    let turn_start_idx = messages.len();
    let mut cumulative_usage = Usage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
    };
    // True if the loop runs out of rounds while the model still wants to call
    // tools (vs. breaking early because it produced a final answer). When set,
    // we do one final tool-less turn so the user isn't left with dangling tool
    // results and no answer.
    let mut hit_round_cap = true;

    for _round in 0..config.max_tool_rounds {
        let req = CompletionRequest {
            model: config.model.clone(),
            system: config.system.clone(),
            messages: messages.clone(),
            tools: tools.to_vec(),
            max_tokens: config.max_tokens,
            temperature: None,
            enable_caching: provider.capabilities().prompt_caching,
        };

        let mut stream = match provider.stream(req).await {
            Ok(s) => s,
            Err(e) => {
                on_event(TurnEvent::Error(format!("stream start: {e}")));
                on_event(TurnEvent::Done);
                return Err(e);
            }
        };

        let mut final_resp = None;

        loop {
            tokio::select! {
                biased;
                _ = &mut cancel => {
                    on_event(TurnEvent::Error("cancelled".into()));
                    on_event(TurnEvent::Done);
                    let new_messages = messages[turn_start_idx..].to_vec();
                    return Ok(TurnOutput {
                        new_messages,
                        usage: cumulative_usage,
                    });
                }
                ev = stream.next() => {
                    let Some(ev) = ev else { break };
                    match ev {
                        Ok(StreamEvent::TextDelta { text, .. }) => {
                            on_event(TurnEvent::TextDelta(text));
                        }
                        Ok(StreamEvent::ThinkingDelta { text, .. }) => {
                            on_event(TurnEvent::ThinkingDelta(text));
                        }
                        Ok(StreamEvent::ToolUseStart { id, name, .. }) => {
                            on_event(TurnEvent::ToolUseStart { id, name });
                        }
                        Ok(StreamEvent::Final(resp)) => {
                            final_resp = Some(resp);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            on_event(TurnEvent::Error(format!("stream: {e}")));
                            on_event(TurnEvent::Done);
                            return Err(e);
                        }
                    }
                }
            }
        }

        let resp = match final_resp {
            Some(r) => r,
            None => {
                let msg = "stream ended without Final event";
                on_event(TurnEvent::Error(msg.into()));
                on_event(TurnEvent::Done);
                return Err(hermes_core::Error::Provider(msg.into()));
            }
        };

        on_event(TurnEvent::Usage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        });
        cumulative_usage.input_tokens += resp.usage.input_tokens;
        cumulative_usage.output_tokens += resp.usage.output_tokens;
        cumulative_usage.cache_read_tokens += resp.usage.cache_read_tokens;
        cumulative_usage.cache_creation_tokens += resp.usage.cache_creation_tokens;

        let assistant_msg = Message {
            role: Role::Assistant,
            content: resp.content.clone(),
        };
        messages.push(assistant_msg);

        // Pre-fill placeholder tool_results for truncated tool_use blocks so
        // they get paired in the same User message as the real tool results.
        // Previously this branch `continue`d before normal tool execution,
        // leaving any non-truncated tool_use in the same response orphaned —
        // which the API then rejected on the next round.
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, .. } = block {
                if resp.truncated_tool_ids.contains(id) {
                    let msg = format!(
                        "Tool call truncated: output exceeded token limit while generating \
                         arguments for '{}'. Retry with shorter content or break into smaller steps.",
                        name
                    );
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: msg.clone(),
                        is_error: true,
                    });
                    on_event(TurnEvent::ToolUseResult {
                        id: id.clone(),
                        content: msg,
                        is_error: true,
                    });
                }
            }
        }

        if resp.stop_reason != StopReason::ToolUse {
            // Flush any truncation placeholders even on non-tool_use stop, so
            // the assistant message's tool_use blocks aren't left orphaned.
            if !tool_results.is_empty() {
                messages.push(Message {
                    role: Role::User,
                    content: tool_results,
                });
            }
            hit_round_cap = false;
            break;
        }

        // Extract tool calls, skipping those already handled as truncated.
        let tool_uses: Vec<(String, String, serde_json::Value)> = resp
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input }
                    if !resp.truncated_tool_ids.contains(id) =>
                {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            // All tool_use blocks were truncated, or none existed. If we have
            // placeholder results, flush them and let the model retry.
            if !tool_results.is_empty() {
                messages.push(Message {
                    role: Role::User,
                    content: tool_results,
                });
                continue;
            }
            tracing::warn!("stop_reason=tool_use but no tool_use blocks");
            hit_round_cap = false;
            break;
        }

        tool_results.reserve(tool_uses.len());
        let mut safe_calls = Vec::new();
        let mut confirm_calls = Vec::new();

        // Phase 1: Categorize and handle denied calls
        for (id, name, input) in tool_uses {
            on_event(TurnEvent::ToolExecStart {
                id: id.clone(),
                name: name.clone(),
                summary: tool_call_summary(&name, &input),
                input: input.clone(),
            });

            match config.permissions.check(&name, &input) {
                Permission::Deny => {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: "Tool call denied by permission rule.".into(),
                        is_error: true,
                    });
                    on_event(TurnEvent::ToolUseResult {
                        id,
                        content: "Tool call denied by permission rule.".into(),
                        is_error: true,
                    });
                }
                Permission::Allow => {
                    safe_calls.push((id, name, input));
                }
                Permission::Prompt if is_dangerous_tool(&name, tools) => {
                    confirm_calls.push((id, name, input));
                }
                Permission::Prompt => {
                    safe_calls.push((id, name, input));
                }
            }
        }

        // Phase 2: Execute safe tools in parallel
        if !safe_calls.is_empty() {
            let futs = safe_calls.into_iter().map(|(id, name, input)| {
                let on_event = &on_event;
                async move {
                    let outcome = match host.call(&name, input).await {
                        Ok(o) => o,
                        Err(e) => ToolCallOutcome {
                            content: format!("tool call failed: {e}"),
                            is_error: true,
                        },
                    };
                    on_event(TurnEvent::ToolUseResult {
                        id: id.clone(),
                        content: outcome.content.clone(),
                        is_error: outcome.is_error,
                    });
                    ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: outcome.content,
                        is_error: outcome.is_error,
                    }
                }
            });
            let parallel = futures::future::join_all(futs);
            tokio::select! {
                biased;
                _ = &mut cancel => {
                    on_event(TurnEvent::Done);
                    flush_with_cancel_pairing(&mut messages, tool_results);
                    let new_messages = messages[turn_start_idx..].to_vec();
                    return Ok(TurnOutput {
                        new_messages,
                        usage: cumulative_usage,
                    });
                }
                results = parallel => {
                    tool_results.extend(results);
                }
            }
        }

        // Phase 3: Execute dangerous tools sequentially with confirmation
        for (id, name, input) in confirm_calls {
            if let Some(tx) = &confirm_tx {
                let (reply_tx, reply_rx) = oneshot::channel();
                let summary = tool_call_summary(&name, &input);
                on_event(TurnEvent::ToolConfirmPending {
                    id: id.clone(),
                    tool_name: name.clone(),
                    summary: summary.clone(),
                });
                let req = ConfirmRequest {
                    id: id.clone(),
                    tool_name: name.clone(),
                    summary,
                    reply: reply_tx,
                };
                if tx.send(req).await.is_err() {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: "Tool call denied (confirmation channel closed).".into(),
                        is_error: true,
                    });
                    continue;
                }
                let action = tokio::select! {
                    biased;
                    _ = &mut cancel => {
                        on_event(TurnEvent::Done);
                        flush_with_cancel_pairing(&mut messages, tool_results);
                        let new_messages = messages[turn_start_idx..].to_vec();
                        return Ok(TurnOutput {
                            new_messages,
                            usage: cumulative_usage,
                        });
                    }
                    r = reply_rx => r,
                };
                match action {
                    Ok(ConfirmAction::Allow | ConfirmAction::AlwaysAllow) => {}
                    deny => {
                        let reason = match deny {
                            Ok(ConfirmAction::Deny { reason }) => reason,
                            _ => None,
                        };
                        let msg = match reason {
                            Some(r) => format!("Tool call denied by user. User says: {r}"),
                            None => "Tool call denied by user.".into(),
                        };
                        tool_results.push(ContentBlock::ToolResult {
                            tool_use_id: id.clone(),
                            content: msg.clone(),
                            is_error: true,
                        });
                        on_event(TurnEvent::ToolUseResult {
                            id,
                            content: msg,
                            is_error: true,
                        });
                        continue;
                    }
                }
            }
            // Approved (or no confirmation channel) — execute
            let outcome = match host.call(&name, input).await {
                Ok(o) => o,
                Err(e) => ToolCallOutcome {
                    content: format!("tool call failed: {e}"),
                    is_error: true,
                },
            };
            on_event(TurnEvent::ToolUseResult {
                id: id.clone(),
                content: outcome.content.clone(),
                is_error: outcome.is_error,
            });
            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: outcome.content,
                is_error: outcome.is_error,
            });
        }

        let result_msg = Message {
            role: Role::User,
            content: tool_results,
        };
        messages.push(result_msg);
    }

    // If we ran out of tool rounds mid-task, the last message is a User turn of
    // tool_results with no assistant answer. Do one final tool-less turn so the
    // user gets a textual answer instead of a silent stop.
    let dangling_results = matches!(
        messages.last(),
        Some(m) if m.role == Role::User
            && m.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
    );
    if hit_round_cap && dangling_results {
        let mut synth_messages = messages.clone();
        synth_messages.push(Message::user_text(
            "You've reached the tool-call budget for this turn. Do not call any more tools. \
             Based on what you've gathered so far, give the user your best final answer now.",
        ));
        let req = CompletionRequest {
            model: config.model.clone(),
            system: config.system.clone(),
            messages: synth_messages,
            tools: Vec::new(), // no tools advertised → forces a textual answer
            max_tokens: config.max_tokens,
            temperature: None,
            enable_caching: provider.capabilities().prompt_caching,
        };
        match provider.stream(req).await {
            Ok(mut stream) => {
                let mut final_resp = None;
                loop {
                    tokio::select! {
                        biased;
                        _ = &mut cancel => break,
                        ev = stream.next() => {
                            let Some(ev) = ev else { break };
                            match ev {
                                Ok(StreamEvent::TextDelta { text, .. }) => {
                                    on_event(TurnEvent::TextDelta(text));
                                }
                                Ok(StreamEvent::ThinkingDelta { text, .. }) => {
                                    on_event(TurnEvent::ThinkingDelta(text));
                                }
                                Ok(StreamEvent::Final(resp)) => final_resp = Some(resp),
                                Ok(_) => {}
                                Err(e) => {
                                    on_event(TurnEvent::Error(format!("final synthesis: {e}")));
                                    break;
                                }
                            }
                        }
                    }
                }
                if let Some(resp) = final_resp {
                    on_event(TurnEvent::Usage {
                        input_tokens: resp.usage.input_tokens,
                        output_tokens: resp.usage.output_tokens,
                    });
                    cumulative_usage.input_tokens += resp.usage.input_tokens;
                    cumulative_usage.output_tokens += resp.usage.output_tokens;
                    cumulative_usage.cache_read_tokens += resp.usage.cache_read_tokens;
                    cumulative_usage.cache_creation_tokens += resp.usage.cache_creation_tokens;
                    messages.push(Message {
                        role: Role::Assistant,
                        content: resp.content,
                    });
                }
            }
            Err(e) => {
                on_event(TurnEvent::Error(format!("final synthesis start: {e}")));
            }
        }
    }

    on_event(TurnEvent::Done);

    let new_messages = messages[turn_start_idx..].to_vec();
    Ok(TurnOutput {
        new_messages,
        usage: cumulative_usage,
    })
}
