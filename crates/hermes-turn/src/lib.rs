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
pub mod danger;
pub mod permissions;

pub use agent::{run_agent, AgentConfig, AgentEvent, AgentOutput};
pub use danger::{assess_confirmation, bash_high_risk_reason, ConfirmAssessment};
pub use permissions::{Permission, PermissionChecker};

const DEFAULT_MAX_TOOL_ROUNDS: usize = 25;

/// User's decision on a dangerous tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Allow,
    /// Allow this and all future calls to the same tool name.
    AlwaysAllow,
    /// Deny, with an optional reason/suggestion fed back to the agent.
    Deny {
        reason: Option<String>,
    },
}

/// Request sent from the turn loop to the frontend for approval.
pub struct ConfirmRequest {
    pub id: String,
    pub tool_name: String,
    pub summary: String,
    /// Why this call is considered especially dangerous (for the UI).
    pub reason: Option<String>,
    pub reply: oneshot::Sender<ConfirmAction>,
}

/// Produce a human-readable one-liner summarizing what a tool call will do.
pub fn tool_call_summary(name: &str, input: &serde_json::Value) -> String {
    let key_field = match name {
        "bash" => "command",
        "read" => "path",
        "write" | "edit" => {
            // Prefer file_path (actual tool schema); fall back to path.
            if input.get("file_path").and_then(|v| v.as_str()).is_some() {
                "file_path"
            } else {
                "path"
            }
        }
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
fn flush_with_cancel_pairing(messages: &mut Vec<Message>, mut tool_results: Vec<ContentBlock>) {
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
    ToolUseStart {
        id: String,
        name: String,
    },
    /// Emitted right before tool execution, when input is fully known.
    ToolExecStart {
        id: String,
        name: String,
        summary: String,
        input: serde_json::Value,
    },
    ToolUseResult {
        id: String,
        content: String,
        is_error: bool,
    },
    ToolConfirmPending {
        id: String,
        tool_name: String,
        summary: String,
        /// Why approval is required (especially-dangerous policy).
        reason: Option<String>,
    },
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_creation_tokens: u32,
    },
    Error(String),
    /// User stopped generation (Stop button). Distinct from hard errors.
    Cancelled,
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
        // Accumulate deltas so cancel mid-stream can still persist a partial reply.
        let mut partial_text = String::new();
        let mut partial_thinking = String::new();

        loop {
            tokio::select! {
                biased;
                _ = &mut cancel => {
                    // Flush partial assistant content if Final never arrived.
                    if final_resp.is_none()
                        && (!partial_text.is_empty() || !partial_thinking.is_empty())
                    {
                        let mut content = Vec::new();
                        if !partial_thinking.is_empty() {
                            content.push(ContentBlock::Thinking {
                                thinking: partial_thinking,
                                signature: None,
                            });
                        }
                        let stopped_note = "\n\n*(Generation stopped.)*";
                        let text = if partial_text.is_empty() {
                            "*(Generation stopped before any answer text.)*"
                                .to_string()
                        } else {
                            format!("{partial_text}{stopped_note}")
                        };
                        content.push(ContentBlock::Text { text });
                        messages.push(Message {
                            role: Role::Assistant,
                            content,
                        });
                    }
                    on_event(TurnEvent::Cancelled);
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
                            partial_text.push_str(&text);
                            on_event(TurnEvent::TextDelta(text));
                        }
                        Ok(StreamEvent::ThinkingDelta { text, .. }) => {
                            partial_thinking.push_str(&text);
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
            cache_read_tokens: resp.usage.cache_read_tokens,
            cache_creation_tokens: resp.usage.cache_creation_tokens,
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
                Permission::Prompt => {
                    let assessment = assess_confirmation(&name, &input, tools);
                    if assessment.needs_confirm {
                        confirm_calls.push((id, name, input, assessment.reason));
                    } else {
                        safe_calls.push((id, name, input));
                    }
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
                    on_event(TurnEvent::Cancelled);
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

        // Phase 3: Especially-dangerous tools — sequential + confirmation
        for (id, name, input, reason) in confirm_calls {
            if let Some(tx) = &confirm_tx {
                let (reply_tx, reply_rx) = oneshot::channel();
                let summary = tool_call_summary(&name, &input);
                on_event(TurnEvent::ToolConfirmPending {
                    id: id.clone(),
                    tool_name: name.clone(),
                    summary: summary.clone(),
                    reason: reason.clone(),
                });
                let req = ConfirmRequest {
                    id: id.clone(),
                    tool_name: name.clone(),
                    summary,
                    reason,
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
                        on_event(TurnEvent::Cancelled);
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

        // C-CARE: after write/edit tools, nudge the next model step to offer
        // brief improvement suggestions (any work domain — not genre-specific).
        // Recover tool names from the assistant message's tool_use blocks.
        let produced_deliverable = tool_results.iter().any(|b| {
            matches!(
                b,
                ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    ..
                } if !*is_error
                    && resp.content.iter().any(|ab| {
                        matches!(
                            ab,
                            ContentBlock::ToolUse { id, name, .. }
                                if id == tool_use_id
                                    && hermes_core::companion::tool_suggests_deliverable(name)
                        )
                    })
            )
        });
        if produced_deliverable {
            tool_results.push(ContentBlock::Text {
                text: hermes_core::companion::care_after_tools_nudge().to_string(),
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
                        cache_read_tokens: resp.usage.cache_read_tokens,
                        cache_creation_tokens: resp.usage.cache_creation_tokens,
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use hermes_core::Result as CoreResult;
    use hermes_core::{Capabilities, CompletionResponse, StopReason, StreamEvent, Usage};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// Scripted provider: pops the next response from a queue per `stream()`
    /// call, emitting a `TextDelta` before `Final` when the response is text.
    struct ScriptedProvider {
        responses: Mutex<VecDeque<CompletionResponse>>,
    }

    impl ScriptedProvider {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
        fn next(&self) -> hermes_core::Result<CompletionResponse> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| hermes_core::Error::Provider("no scripted response left".into()))
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn complete(&self, _req: CompletionRequest) -> hermes_core::Result<CompletionResponse> {
            self.next()
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> hermes_core::Result<futures::stream::BoxStream<'static, CoreResult<StreamEvent>>> {
            let resp = self.next()?;
            let mut evs: Vec<CoreResult<StreamEvent>> = Vec::new();
            for (i, block) in resp.content.iter().enumerate() {
                if let ContentBlock::Text { text } = block {
                    evs.push(Ok(StreamEvent::TextDelta {
                        index: i,
                        text: text.clone(),
                    }));
                }
            }
            evs.push(Ok(StreamEvent::Final(resp)));
            Ok(Box::pin(stream::iter(evs)))
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: true,
            }
        }

        fn name(&self) -> &str {
            "scripted-test"
        }
    }

    /// Minimal host: `echo_tool` returns its args; `danger_tool` is only
    /// reachable after a confirm — both keep the test off real tools.
    struct EchoHost;

    #[async_trait::async_trait]
    impl ToolHost for EchoHost {
        async fn list_tools(&self) -> hermes_core::Result<Vec<ToolSpec>> {
            Ok(vec![
                ToolSpec {
                    name: "echo_tool".into(),
                    description: "echoes its input".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    requires_confirmation: false,
                },
                ToolSpec {
                    name: "danger_tool".into(),
                    description: "requires approval".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    requires_confirmation: true,
                },
            ])
        }

        async fn call(
            &self,
            name: &str,
            args: serde_json::Value,
        ) -> hermes_core::Result<ToolCallOutcome> {
            match name {
                "echo_tool" => Ok(ToolCallOutcome {
                    content: format!("echo:{args}"),
                    is_error: false,
                }),
                "danger_tool" => Ok(ToolCallOutcome {
                    content: "approved-executed".into(),
                    is_error: false,
                }),
                other => Err(hermes_core::Error::ToolHost(format!("unknown test tool {other}"))),
            }
        }
    }

    fn usage(n_in: u32, n_out: u32) -> Usage {
        Usage {
            input_tokens: n_in,
            output_tokens: n_out,
            cache_read_tokens: 11,
            cache_creation_tokens: 22,
        }
    }

    fn text_resp(text: &str) -> CompletionResponse {
        CompletionResponse {
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: usage(5, 7),
            truncated_tool_ids: vec![],
        }
    }

    fn tool_resp(id: &str, name: &str, input: serde_json::Value) -> CompletionResponse {
        CompletionResponse {
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: usage(5, 7),
            truncated_tool_ids: vec![],
        }
    }

    fn config() -> TurnConfig {
        TurnConfig {
            model: "test-model".into(),
            system: None,
            max_tokens: 100,
            max_tool_rounds: 4,
            permissions: PermissionChecker::default(),
        }
    }

    fn no_cancel() -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel::<()>();
        // Keep the sender alive: dropping it would resolve the receiver and
        // make the turn loop treat the turn as cancelled.
        std::mem::forget(tx);
        rx
    }

    fn events_of(
        provider: &dyn LlmProvider,
        host: &dyn ToolHost,
        history: &[Message],
        confirm: Option<mpsc::Sender<ConfirmRequest>>,
    ) -> (TurnOutput, Vec<TurnEvent>) {
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let out = futures::executor::block_on(run_turn(
            provider,
            host,
            &[],
            history,
            &config(),
            confirm,
            move |e| sink.lock().unwrap().push(e),
            no_cancel(),
        ))
        .expect("run_turn should succeed");
        let evs = Arc::try_unwrap(collected).unwrap().into_inner().unwrap();
        (out, evs)
    }

    fn text_of(msg: &Message) -> String {
        msg.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    
    #[test]
    fn run_turn_pure_text_round_trip() {
        let provider = ScriptedProvider::new(vec![text_resp("hello world")]);
        let (out, evs) = events_of(&provider, &EchoHost, &[Message::user_text("hi")], None);

        assert_eq!(out.new_messages.len(), 1);
        assert_eq!(text_of(&out.new_messages[0]), "hello world");
        assert!(evs.iter().any(|e| matches!(e, TurnEvent::TextDelta(t) if t == "hello world")));
        assert!(evs.iter().any(|e| matches!(
            e,
            TurnEvent::Usage {
                cache_read_tokens: 11,
                cache_creation_tokens: 22,
                ..
            }
        )));
        assert!(evs.iter().any(|e| matches!(e, TurnEvent::Done)));
        assert!(!evs.iter().any(|e| matches!(e, TurnEvent::ToolUseStart { .. })));
    }

    #[test]
    fn run_turn_feeds_tool_result_back_then_finishes() {
        let provider = ScriptedProvider::new(vec![
            tool_resp("t1", "echo_tool", serde_json::json!({"msg": "x"})),
            text_resp("done after tool"),
        ]);
        let tools = futures::executor::block_on(EchoHost.list_tools()).unwrap();
        let host = EchoHost;
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let out = futures::executor::block_on(run_turn(
            &provider,
            &host,
            &tools,
            &[Message::user_text("use echo")],
            &config(),
            None,
            move |e| sink.lock().unwrap().push(e),
            no_cancel(),
        ))
        .expect("run_turn should succeed");
        let evs = Arc::try_unwrap(collected).unwrap().into_inner().unwrap();

        // Assistant tool-use + user tool-result + final assistant text.
        assert_eq!(out.new_messages.len(), 3);
        assert!(evs
            .iter()
            .any(|e| matches!(e, TurnEvent::ToolExecStart { name, .. } if name == "echo_tool")));
        assert!(evs.iter().any(|e| matches!(
            e,
            TurnEvent::ToolUseResult { content, is_error: false, .. } if content.starts_with("echo:")
        )));
        assert!(evs.iter().any(|e| matches!(e, TurnEvent::TextDelta(t) if t == "done after tool")));
        assert!(evs.iter().any(|e| matches!(e, TurnEvent::Done)));
        // Tool result must have been fed back into a user message.
        let last = out.new_messages.last().unwrap();
        assert_eq!(text_of(last), "done after tool");
    }

    #[tokio::test]
    async fn run_turn_confirm_allow_executes() {
        let provider = ScriptedProvider::new(vec![
            tool_resp("t9", "danger_tool", serde_json::json!({})),
            text_resp("finished"),
        ]);
        let tools = EchoHost.list_tools().await.unwrap();
        let (confirm_tx, mut confirm_rx) = mpsc::channel::<ConfirmRequest>(4);
        let approver = tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                let _ = req.reply.send(ConfirmAction::Allow);
            }
        });
        let host = EchoHost;
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let out = run_turn(
            &provider,
            &host,
            &tools,
            &[Message::user_text("danger")],
            &config(),
            Some(confirm_tx),
            move |e| sink.lock().unwrap().push(e),
            no_cancel(),
        )
        .await
        .expect("run_turn should succeed");
        let _ = approver.await;
        let evs = Arc::try_unwrap(collected).unwrap().into_inner().unwrap();

        assert!(evs.iter().any(|e| matches!(e, TurnEvent::ToolConfirmPending { .. })));
        assert!(evs.iter().any(|e| matches!(
            e,
            TurnEvent::ToolUseResult { content, is_error: false, .. } if content == "approved-executed"
        )));
        assert_eq!(text_of(out.new_messages.last().unwrap()), "finished");
    }

    #[tokio::test]
    async fn run_turn_confirm_deny_blocks_execution() {
        let provider = ScriptedProvider::new(vec![
            tool_resp("t8", "danger_tool", serde_json::json!({})),
            text_resp("after deny"),
        ]);
        let tools = EchoHost.list_tools().await.unwrap();
        let (confirm_tx, mut confirm_rx) = mpsc::channel::<ConfirmRequest>(4);
        let denier = tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                let _ = req.reply.send(ConfirmAction::Deny { reason: None });
            }
        });
        let host = EchoHost;
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let out = run_turn(
            &provider,
            &host,
            &tools,
            &[Message::user_text("danger")],
            &config(),
            Some(confirm_tx),
            move |e| sink.lock().unwrap().push(e),
            no_cancel(),
        )
        .await
        .expect("run_turn should succeed");
        let _ = denier.await;
        let evs = Arc::try_unwrap(collected).unwrap().into_inner().unwrap();

        assert!(evs.iter().any(|e| matches!(e, TurnEvent::ToolConfirmPending { .. })));
        assert!(evs.iter().any(|e| matches!(
            e,
            TurnEvent::ToolUseResult { content, is_error: true, .. } if content.contains("denied")
        )));
        // Model still gets its final text after being told the call was denied.
        assert_eq!(text_of(out.new_messages.last().unwrap()), "after deny");
    }
}
