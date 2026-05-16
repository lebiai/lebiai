//! Shared agent turn execution loop.
//!
//! All frontends (CLI, TUI, GUI) call `run_turn()` and consume the
//! resulting `TurnEvent` stream. This eliminates duplicated streaming,
//! tool-execution, and micro-reflection logic.

use futures::StreamExt;
use hermes_core::{
    CompletionRequest, ContentBlock, LlmProvider, Message, Role, StopReason, StreamEvent,
    ToolCallOutcome, ToolHost, ToolSpec, Usage,
};
use hermes_memory::LoadedMemory;
use hermes_reflect::ReflectionOutput;
use hermes_skills::LoadedSkill;
use tokio::sync::{mpsc, oneshot};

const DEFAULT_MAX_TOOL_ROUNDS: usize = 10;

/// User's decision on a dangerous tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Allow,
    Deny,
}

/// Request sent from the turn loop to the frontend for approval.
pub struct ConfirmRequest {
    pub id: String,
    pub tool_name: String,
    pub summary: String,
    pub reply: oneshot::Sender<ConfirmAction>,
}

/// Returns true if the tool requires user confirmation before execution.
pub fn is_dangerous_tool(name: &str) -> bool {
    matches!(
        name,
        "bash" | "write" | "edit" | "web_fetch" | "web_search" | "todo_add" | "todo_update"
    ) || name.contains("__") // MCP tools: server__tool
}

/// Produce a human-readable one-liner summarizing what a tool call will do.
pub fn tool_call_summary(name: &str, input: &serde_json::Value) -> String {
    let key_field = match name {
        "bash" => "command",
        "write" => "file_path",
        "edit" => "file_path",
        "web_fetch" => "url",
        "web_search" => "query",
        "memory_search" => "query",
        "todo_add" | "todo_update" => "text",
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

#[derive(Debug, Clone)]
pub enum TurnEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStart { id: String, name: String },
    ToolUseResult { id: String, content: String, is_error: bool },
    ToolConfirmPending { id: String, tool_name: String, summary: String },
    Usage { input_tokens: u32, output_tokens: u32 },
    MicroReflection(ReflectionOutput),
    Error(String),
    Done,
}

pub struct TurnConfig {
    pub model: String,
    pub system: Option<String>,
    pub max_tokens: u32,
    pub max_tool_rounds: usize,
    pub enable_micro_reflect: bool,
    pub turns_since_last_reflect: usize,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            system: None,
            max_tokens: 16_384,
            max_tool_rounds: DEFAULT_MAX_TOOL_ROUNDS,
            enable_micro_reflect: true,
            turns_since_last_reflect: 3,
        }
    }
}

pub struct TurnOutput {
    pub new_messages: Vec<Message>,
    pub usage: Usage,
    pub reflection: Option<ReflectionOutput>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_turn<F>(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[ToolSpec],
    history: &[Message],
    config: &TurnConfig,
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
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
                        reflection: None,
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

        if resp.stop_reason != StopReason::ToolUse {
            break;
        }

        // Extract and execute tool calls
        let tool_uses: Vec<(String, String, serde_json::Value)> = resp
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            tracing::warn!("stop_reason=tool_use but no tool_use blocks");
            break;
        }

        let mut tool_results = Vec::with_capacity(tool_uses.len());
        for (id, name, input) in tool_uses {
            // Confirmation gate for dangerous tools
            if is_dangerous_tool(&name) {
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
                        // Channel closed — default to deny
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
                            let new_messages = messages[turn_start_idx..].to_vec();
                            return Ok(TurnOutput {
                                new_messages,
                                usage: cumulative_usage,
                                reflection: None,
                            });
                        }
                        r = reply_rx => r,
                    };
                    match action {
                        Ok(ConfirmAction::Allow) => { /* proceed to host.call() below */ }
                        Ok(ConfirmAction::Deny) | Err(_) => {
                            tool_results.push(ContentBlock::ToolResult {
                                tool_use_id: id.clone(),
                                content: "Tool call denied by user.".into(),
                                is_error: true,
                            });
                            on_event(TurnEvent::ToolUseResult {
                                id,
                                content: "Tool call denied by user.".into(),
                                is_error: true,
                            });
                            continue;
                        }
                    }
                }
            }

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

    // Micro-reflection: include the user message that triggered this turn
    // (it's the last message in history, i.e. messages[turn_start_idx - 1])
    let reflect_start = turn_start_idx.saturating_sub(1);
    let turn_msgs = &messages[reflect_start..];
    let reflection = if config.enable_micro_reflect
        && hermes_reflect::should_micro_reflect(turn_msgs, config.turns_since_last_reflect)
    {
        match hermes_reflect::micro_reflect(provider, turn_msgs, skills, memories).await {
            Ok(output) if !output.is_empty() => {
                tracing::info!(summary=%output.summary, "micro-reflection produced candidates");
                on_event(TurnEvent::MicroReflection(output.clone()));
                Some(output)
            }
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(error=%e, "micro-reflection failed");
                None
            }
        }
    } else {
        None
    };

    on_event(TurnEvent::Done);

    let new_messages = messages[turn_start_idx..].to_vec();
    Ok(TurnOutput {
        new_messages,
        usage: cumulative_usage,
        reflection,
    })
}
