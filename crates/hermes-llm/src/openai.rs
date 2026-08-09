//! OpenAI-compatible provider (Chat Completions API).
//!
//! Talks to any endpoint that accepts the OpenAI v1/chat/completions
//! contract — OpenAI itself, DeepSeek's `/v1` endpoint, Qwen DashScope's
//! compatible mode, OpenRouter, vLLM, etc. The differences with the
//! Anthropic provider are kept inside this module so the chat / agent
//! loop sees the same [`hermes_core::LlmProvider`] trait.
//!
//! Protocol translation (Anthropic-shaped types ↔ OpenAI wire):
//! - assistant `tool_use` blocks ↔ `tool_calls` array on assistant message
//! - user `tool_result` blocks ↔ N messages with `role:"tool"` + `tool_call_id`
//! - `thinking` blocks have no OpenAI representation; we drop them on send
//!   and never emit them on receive.

use std::sync::Arc;
use std::time::Duration;

use crate::retry::{backoff_delay, is_retriable_status, parse_retry_after, RETRY_ATTEMPTS};
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use hermes_core::{
    Capabilities, CompletionRequest, CompletionResponse, ContentBlock, Error, LlmProvider, Message,
    Result, Role, StopReason, StreamEvent, ToolSpec, Usage,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct OpenAiProvider {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl OpenAiProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(300))
            .user_agent(format!("lebi-ai/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Provider(format!("building http client: {e}")))?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                base_url: base_url.into().trim_end_matches('/').to_string(),
                api_key: api_key.into(),
                default_model: default_model.into(),
            }),
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let body = build_request_body(&self.inner.default_model, &req, false);
        let url = format!("{}/chat/completions", self.inner.base_url);
        tracing::debug!(url = %url, "openai request");

        let resp = self.send_with_retry(&url, &body, false).await?;

        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| Error::Provider(format!("decoding response: {e}")))?;

        Ok(parsed.into_completion())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_use: true,
            // OpenAI-compat endpoints generally don't expose Anthropic-style
            // cache_control. Disable to avoid false expectations.
            prompt_caching: false,
            streaming: true,
        }
    }

    fn name(&self) -> &str {
        "openai"
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let body = build_request_body(&self.inner.default_model, &req, true);
        let url = format!("{}/chat/completions", self.inner.base_url);

        let resp = self.send_with_retry(&url, &body, true).await?;

        Ok(Box::pin(parse_openai_stream(resp.bytes_stream().boxed())))
    }
}

impl OpenAiProvider {
    /// POST `body` to `url`, retrying on transient errors (429 / 5xx /
    /// network). `streaming=true` adds the `accept: text/event-stream`
    /// header. Returns the successful response — caller decodes it.
    /// Mirrors the Anthropic provider's policy via [`crate::retry`].
    async fn send_with_retry<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
        streaming: bool,
    ) -> Result<reqwest::Response> {
        let mut last_err: Option<String> = None;
        for attempt in 0..=RETRY_ATTEMPTS {
            if attempt > 0 {
                tracing::debug!(attempt, "openai retry");
            }
            let mut req = self
                .inner
                .client
                .post(url)
                .header("authorization", format!("Bearer {}", self.inner.api_key))
                .header("content-type", "application/json");
            if streaming {
                req = req.header("accept", "text/event-stream");
            }
            let send_res = req.json(body).send().await;

            match send_res {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp);
                    }
                    if is_retriable_status(status) && attempt < RETRY_ATTEMPTS {
                        let delay =
                            parse_retry_after(&resp).unwrap_or_else(|| backoff_delay(attempt));
                        let text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            attempt,
                            status = %status,
                            delay_ms = delay.as_millis(),
                            "openai transient error, retrying"
                        );
                        last_err = Some(format!("HTTP {status}: {text}"));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    let text = resp.text().await.unwrap_or_default();
                    return Err(Error::Provider(format!("HTTP {status}: {text}")));
                }
                Err(e) => {
                    if attempt < RETRY_ATTEMPTS {
                        let delay = backoff_delay(attempt);
                        tracing::warn!(
                            attempt,
                            error = %e,
                            delay_ms = delay.as_millis(),
                            "openai network error, retrying"
                        );
                        last_err = Some(format!("http send: {e}"));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(Error::Provider(format!("http send: {e}")));
                }
            }
        }
        Err(Error::Provider(
            last_err.unwrap_or_else(|| "exhausted retries".to_string()),
        ))
    }
}

// ---- request building --------------------------------------------------

fn build_request_body(default_model: &str, req: &CompletionRequest, stream: bool) -> ChatRequest {
    let model = if req.model.is_empty() {
        default_model.to_string()
    } else {
        req.model.clone()
    };

    let mut messages: Vec<ChatMessage> = Vec::new();
    if let Some(sys) = &req.system {
        if !sys.is_empty() {
            messages.push(ChatMessage::system(sys.clone()));
        }
    }
    for m in &req.messages {
        messages.extend(translate_outbound(m));
    }

    let tools = if req.tools.is_empty() {
        None
    } else {
        Some(req.tools.iter().map(translate_tool).collect())
    };

    ChatRequest {
        model,
        messages,
        max_tokens: Some(req.max_tokens),
        temperature: req.temperature,
        stream,
        tools,
    }
}

fn translate_tool(t: &ToolSpec) -> ChatTool {
    ChatTool {
        kind: "function".into(),
        function: ChatFunction {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        },
    }
}

/// One Anthropic-shaped Message can become 1..N OpenAI messages:
/// - user with N tool_result blocks → N `role:"tool"` messages
/// - user with mixed text + (no) tool_result → one `role:"user"` message
/// - assistant with text + tool_use blocks → one `role:"assistant"`
///   message carrying both `content` and `tool_calls`
fn translate_outbound(m: &Message) -> Vec<ChatMessage> {
    match m.role {
        Role::User => {
            let mut out = Vec::new();
            let mut text_parts: Vec<String> = Vec::new();
            for block in &m.content {
                match block {
                    ContentBlock::Text { text } => text_parts.push(text.clone()),
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        // Send as a `tool` message. is_error is not part of
                        // OpenAI spec; encode inline for transparency.
                        let body = if *is_error {
                            format!("[error] {content}")
                        } else {
                            content.clone()
                        };
                        out.push(ChatMessage {
                            role: "tool".into(),
                            content: Some(body),
                            tool_call_id: Some(tool_use_id.clone()),
                            tool_calls: None,
                            name: None,
                        });
                    }
                    ContentBlock::ToolUse { .. } | ContentBlock::Thinking { .. } => {
                        // user-side tool_use never happens in our model;
                        // thinking is dropped.
                    }
                    ContentBlock::Image { source } => {
                        // OpenAI-compat endpoints are text-only for now
                        // (intentional degradation — same rule documented in
                        // hermes-core message.rs). Embed a placeholder so the
                        // model still knows an image was attached and which
                        // media type it had; real multi-part image_url
                        // support would be a new provider capability.
                        text_parts.push(format!("[image: {}]", source.media_type));
                    }
                }
            }
            if !text_parts.is_empty() {
                out.insert(
                    0,
                    ChatMessage {
                        role: "user".into(),
                        content: Some(text_parts.join("\n")),
                        tool_call_id: None,
                        tool_calls: None,
                        name: None,
                    },
                );
            }
            out
        }
        Role::Assistant => {
            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<ChatToolCall> = Vec::new();
            for block in &m.content {
                match block {
                    ContentBlock::Text { text } => text_parts.push(text.clone()),
                    ContentBlock::ToolUse { id, name, input } => {
                        let arguments =
                            serde_json::to_string(input).unwrap_or_else(|_| "{}".into());
                        tool_calls.push(ChatToolCall {
                            id: id.clone(),
                            kind: "function".into(),
                            function: ChatToolCallFn {
                                name: name.clone(),
                                arguments,
                            },
                        });
                    }
                    // Thinking is provider-internal; drop on send.
                    ContentBlock::Thinking { .. } => {}
                    ContentBlock::ToolResult { .. } => {
                        // Assistant never produces tool_result in our model.
                    }
                    ContentBlock::Image { .. } => {
                        // Assistant never produces images in our model.
                    }
                }
            }
            let content = if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join(""))
            };
            let tc = if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            };
            vec![ChatMessage {
                role: "assistant".into(),
                content,
                tool_call_id: None,
                tool_calls: tc,
                name: None,
            }]
        }
    }
}

// ---- wire types --------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ChatTool>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    name: Option<String>,
}

impl ChatMessage {
    fn system(text: String) -> Self {
        Self {
            role: "system".into(),
            content: Some(text),
            tool_call_id: None,
            tool_calls: None,
            name: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatTool {
    #[serde(rename = "type")]
    kind: String,
    function: ChatFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatToolCall {
    id: String,
    #[serde(rename = "type", default = "default_tool_kind")]
    kind: String,
    function: ChatToolCallFn,
}

fn default_tool_kind() -> String {
    "function".into()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatToolCallFn {
    name: String,
    /// JSON-encoded arguments string per OpenAI spec.
    arguments: String,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize, Debug)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

impl ChatResponse {
    fn into_completion(self) -> CompletionResponse {
        let mut content: Vec<ContentBlock> = Vec::new();
        let mut stop_reason = StopReason::Other;
        if let Some(choice) = self.choices.into_iter().next() {
            stop_reason = match choice.finish_reason.as_deref() {
                Some("stop") => StopReason::EndTurn,
                Some("length") => StopReason::MaxTokens,
                Some("tool_calls") => StopReason::ToolUse,
                Some("stop_sequence") => StopReason::StopSequence,
                _ => StopReason::Other,
            };
            if let Some(text) = choice.message.content {
                if !text.is_empty() {
                    content.push(ContentBlock::Text { text });
                }
            }
            if let Some(calls) = choice.message.tool_calls {
                for call in calls {
                    let input = serde_json::from_str(&call.function.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    content.push(ContentBlock::ToolUse {
                        id: call.id,
                        name: call.function.name,
                        input,
                    });
                }
            }
        }
        let usage = self
            .usage
            .map(|u| Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            })
            .unwrap_or_default();
        CompletionResponse {
            content,
            stop_reason,
            usage,
            truncated_tool_ids: Vec::new(),
        }
    }
}

// ---- streaming SSE -----------------------------------------------------

fn parse_openai_stream(
    bytes: BoxStream<'static, std::result::Result<bytes::Bytes, reqwest::Error>>,
) -> impl futures::Stream<Item = Result<StreamEvent>> {
    use futures::stream;

    let state = State {
        bytes,
        line_buf: String::new(),
        pending: std::collections::VecDeque::new(),
        finished: false,
        text_buf: String::new(),
        tool_calls: Vec::new(),
        stop_reason: StopReason::Other,
        usage: Usage::default(),
        announced_message_start: false,
    };

    stream::unfold(state, |mut s| async move {
        loop {
            if let Some(ev) = s.pending.pop_front() {
                return Some((ev, s));
            }
            if s.finished {
                return None;
            }

            match s.bytes.next().await {
                Some(Ok(chunk)) => match std::str::from_utf8(&chunk) {
                    Ok(text) => s.line_buf.push_str(text),
                    Err(_) => {
                        let text = String::from_utf8_lossy(&chunk);
                        s.line_buf.push_str(&text);
                    }
                },
                Some(Err(e)) => {
                    tracing::debug!(error=%e, "openai stream chunk error (continuing)");
                    continue;
                }
                None => {
                    finalise(&mut s);
                    s.finished = true;
                    continue;
                }
            }

            while let Some(nl) = s.line_buf.find('\n') {
                let mut line = s.line_buf[..nl].to_string();
                s.line_buf.drain(..=nl);
                if line.ends_with('\r') {
                    line.pop();
                }
                handle_line(&line, &mut s);
            }
        }
    })
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn handle_line(line: &str, s: &mut StreamState) {
    if line.is_empty() || line.starts_with(':') {
        return;
    }
    let payload = match line.strip_prefix("data:") {
        Some(p) => p.trim_start(),
        None => return,
    };
    if payload == "[DONE]" {
        return;
    }
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, line=%payload, "skipping unparseable openai chunk");
            return;
        }
    };

    if !s.announced_message_start {
        s.pending.push_back(Ok(StreamEvent::MessageStart));
        s.announced_message_start = true;
    }

    if let Some(usage) = v.get("usage") {
        if let Some(u) = parse_usage(usage) {
            s.usage = u;
        }
    }

    let Some(choices) = v.get("choices").and_then(|c| c.as_array()) else {
        return;
    };
    let Some(choice) = choices.first() else {
        return;
    };
    if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
        s.stop_reason = match reason {
            "stop" => StopReason::EndTurn,
            "length" => StopReason::MaxTokens,
            "tool_calls" => StopReason::ToolUse,
            "stop_sequence" => StopReason::StopSequence,
            _ => StopReason::Other,
        };
    }
    let Some(delta) = choice.get("delta") else {
        return;
    };

    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            s.text_buf.push_str(text);
            s.pending.push_back(Ok(StreamEvent::TextDelta {
                index: 0,
                text: text.to_string(),
            }));
        }
    }

    if let Some(arr) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
        for entry in arr {
            let idx = entry.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            while s.tool_calls.len() <= idx {
                s.tool_calls.push(PartialToolCall::default());
            }
            let slot = &mut s.tool_calls[idx];
            if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                if !id.is_empty() && slot.id.is_empty() {
                    slot.id = id.to_string();
                }
            }
            if let Some(func) = entry.get("function") {
                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                    if !name.is_empty() && slot.name.is_empty() {
                        slot.name = name.to_string();
                        s.pending.push_back(Ok(StreamEvent::ToolUseStart {
                            index: idx + 1, // shift past the text block at index 0
                            id: slot.id.clone(),
                            name: slot.name.clone(),
                        }));
                    }
                }
                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                    if !args.is_empty() {
                        slot.arguments.push_str(args);
                        s.pending.push_back(Ok(StreamEvent::ToolUseInputDelta {
                            index: idx + 1,
                            partial_json: args.to_string(),
                        }));
                    }
                }
            }
        }
    }
}

type StreamState = State;
struct State {
    bytes: BoxStream<'static, std::result::Result<bytes::Bytes, reqwest::Error>>,
    line_buf: String,
    pending: std::collections::VecDeque<Result<StreamEvent>>,
    finished: bool,
    text_buf: String,
    tool_calls: Vec<PartialToolCall>,
    stop_reason: StopReason,
    usage: Usage,
    announced_message_start: bool,
}

fn finalise(s: &mut StreamState) {
    let mut content: Vec<ContentBlock> = Vec::new();
    let mut truncated_tool_ids = Vec::new();
    if !s.text_buf.is_empty() {
        content.push(ContentBlock::Text {
            text: std::mem::take(&mut s.text_buf),
        });
        // Text occupies block index 0 (see `handle_line`).
        s.pending.push_back(Ok(StreamEvent::BlockStop { index: 0 }));
    }
    for (i, c) in s.tool_calls.drain(..).enumerate() {
        // Tool blocks are streamed at `idx + 1` (shifted past the text block).
        let block_index = i + 1;
        s.pending
            .push_back(Ok(StreamEvent::BlockStop { index: block_index }));
        let input = if c.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&c.arguments).unwrap_or_else(|e| {
                tracing::warn!(error=%e, "openai tool_call arguments parse failed; using {{}}");
                truncated_tool_ids.push(c.id.clone());
                serde_json::json!({})
            })
        };
        content.push(ContentBlock::ToolUse {
            id: c.id,
            name: c.name,
            input,
        });
    }
    s.pending
        .push_back(Ok(StreamEvent::Final(CompletionResponse {
            content,
            stop_reason: s.stop_reason,
            usage: s.usage,
            truncated_tool_ids,
        })));
}

fn parse_usage(v: &serde_json::Value) -> Option<Usage> {
    let prompt = v.get("prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
    let completion = v
        .get("completion_tokens")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    if prompt == 0 && completion == 0 {
        return None;
    }
    Some(Usage {
        input_tokens: prompt,
        output_tokens: completion,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_user_text_message() {
        let m = Message::user_text("hi");
        let out = translate_outbound(&m);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content.as_deref(), Some("hi"));
        assert!(out[0].tool_calls.is_none());
    }

    #[test]
    fn translate_user_with_tool_results() {
        let m = Message {
            role: Role::User,
            content: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "ok".into(),
                    is_error: false,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "t2".into(),
                    content: "err".into(),
                    is_error: true,
                },
            ],
        };
        let out = translate_outbound(&m);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, "tool");
        assert_eq!(out[0].tool_call_id.as_deref(), Some("t1"));
        assert_eq!(out[0].content.as_deref(), Some("ok"));
        assert_eq!(out[1].role, "tool");
        assert_eq!(out[1].tool_call_id.as_deref(), Some("t2"));
        assert_eq!(out[1].content.as_deref(), Some("[error] err"));
    }

    #[test]
    fn translate_assistant_with_tool_use() {
        let m = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "calling".into(),
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"q":"rust"}),
                },
            ],
        };
        let out = translate_outbound(&m);
        assert_eq!(out.len(), 1);
        let am = &out[0];
        assert_eq!(am.role, "assistant");
        assert_eq!(am.content.as_deref(), Some("calling"));
        let calls = am.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "search");
        let parsed: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(parsed, serde_json::json!({"q":"rust"}));
    }

    #[test]
    fn assistant_thinking_blocks_dropped_on_send() {
        let m = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "think...".into(),
                    signature: None,
                },
                ContentBlock::Text {
                    text: "hello".into(),
                },
            ],
        };
        let out = translate_outbound(&m);
        assert_eq!(out[0].content.as_deref(), Some("hello"));
        assert!(out[0].tool_calls.is_none());
    }

    #[test]
    fn response_with_tool_calls_decodes_to_blocks() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "going to search",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "search", "arguments": "{\"q\":\"x\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4}
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let comp = parsed.into_completion();
        assert_eq!(comp.stop_reason, StopReason::ToolUse);
        assert_eq!(comp.content.len(), 2);
        assert!(
            matches!(&comp.content[0], ContentBlock::Text { text } if text == "going to search")
        );
        assert!(
            matches!(&comp.content[1], ContentBlock::ToolUse { id, name, .. } if id == "call_1" && name == "search")
        );
        assert_eq!(comp.usage.input_tokens, 10);
        assert_eq!(comp.usage.output_tokens, 4);
    }

    #[test]
    fn response_finish_reason_mapping() {
        for (raw_reason, expected) in [
            ("stop", StopReason::EndTurn),
            ("length", StopReason::MaxTokens),
            ("tool_calls", StopReason::ToolUse),
            ("nonsense", StopReason::Other),
        ] {
            let raw = format!(
                r#"{{"choices":[{{"message":{{"role":"assistant","content":"x"}},"finish_reason":"{raw_reason}"}}]}}"#
            );
            let parsed: ChatResponse = serde_json::from_str(&raw).unwrap();
            let comp = parsed.into_completion();
            assert_eq!(comp.stop_reason, expected);
        }
    }
}
