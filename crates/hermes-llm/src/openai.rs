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
                            reasoning_content: None,
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
            // CRITICAL ordering for OpenAI-compatible APIs (DeepSeek etc.):
            // after an assistant `tool_calls` message, the next messages MUST be
            // `role:"tool"` replies — never a `user` message in between.
            // So emit tool results first, then any accompanying user text.
            if !text_parts.is_empty() {
                out.push(ChatMessage {
                    role: "user".into(),
                    content: Some(text_parts.join("\n")),
                    reasoning_content: None,
                    tool_call_id: None,
                    tool_calls: None,
                    name: None,
                });
            }
            out
        }
        Role::Assistant => {
            // 空 assistant 不上线：线格式里它会退化成 `{"role":"assistant"}`，端点直接 400。
            // 修复历史时会丢（`sanitize_history_for_provider`），但 CLI 等入口不走那一步，
            // 这里是最后一道闸——判据仍只有 `Message::has_sendable_content` 一处。
            if !m.has_sendable_content() {
                return Vec::new();
            }
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
                reasoning_content: None,
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
    /// 推理模型的思考正文（DeepSeek 等 OpenAI 兼容端点放在这里）。
    ///
    /// **只读不写**：发出去的请求里永远没有这个字段（`skip_serializing_if` +
    /// 所有出站构造点都显式填 `None`）。2026-09-20 之前这里根本没解析，于是
    /// 推理模型的思考整段被丢掉——用户看到的「一句话不说卡了八分钟」，
    /// 那一轮 16384 tokens 里约四成是多出来的、看不见的推理。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    reasoning_content: Option<String>,
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
            reasoning_content: None,
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
    /// OpenAI 标准：`{"prompt_tokens_details": {"cached_tokens": N}}`。
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
    /// DeepSeek 自家：`"prompt_cache_hit_tokens": N`。
    #[serde(default)]
    prompt_cache_hit_tokens: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

impl ChatUsage {
    /// 命中缓存的那部分输入（两种线格式取先有的那个）。夹到 `prompt_tokens`
    /// 以内，免得端点给了离谱的数导致下面减法下溢。
    fn cached_input(&self) -> u32 {
        self.prompt_tokens_details
            .as_ref()
            .map(|d| d.cached_tokens)
            .or(self.prompt_cache_hit_tokens)
            .unwrap_or(0)
            .min(self.prompt_tokens)
    }
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
            // 思考块排在最前（与 Anthropic 线一致）：`has_sendable_content` 认它
            // 不算内容，界面认它是一段可折叠的「思考中」。
            if let Some(think) = choice.message.reasoning_content {
                if !think.trim().is_empty() {
                    content.insert(
                        0,
                        ContentBlock::Thinking {
                            thinking: think,
                            signature: None,
                        },
                    );
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
            .map(|u| {
                let (input_tokens, cache_read_tokens) =
                    split_input(u.prompt_tokens, u.cached_input());
                Usage {
                    input_tokens,
                    output_tokens: u.completion_tokens,
                    cache_read_tokens,
                    cache_creation_tokens: 0,
                }
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
        utf8: crate::utf8::Utf8Carry::new(),
        line_buf: String::new(),
        pending: std::collections::VecDeque::new(),
        finished: false,
        text_buf: String::new(),
        thinking_buf: String::new(),
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
                Some(Ok(chunk)) => {
                    let text = s.utf8.push(&chunk);
                    s.line_buf.push_str(&text);
                }
                Some(Err(e)) => {
                    tracing::debug!(error=%e, "openai stream chunk error (continuing)");
                    continue;
                }
                None => {
                    let tail = s.utf8.finish();
                    s.line_buf.push_str(&tail);
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

    // 推理正文：块下标 0，文本跟在它后面（见 `finalise` 的编号）。
    if let Some(think) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
        if !think.is_empty() {
            s.thinking_buf.push_str(think);
            s.pending.push_back(Ok(StreamEvent::ThinkingDelta {
                index: 0,
                text: think.to_string(),
            }));
        }
    }

    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            s.text_buf.push_str(text);
            s.pending.push_back(Ok(StreamEvent::TextDelta {
                // 思考块占了 0 号，文本就顺延到 1 号；没有思考时文本仍是 0 号。
                index: usize::from(!s.thinking_buf.is_empty()),
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
    utf8: crate::utf8::Utf8Carry,
    line_buf: String,
    pending: std::collections::VecDeque<Result<StreamEvent>>,
    finished: bool,
    text_buf: String,
    /// 推理正文的累积缓冲。它单占一个内容块，排在文本之前，所以下面算块下标
    /// 时先看它有没有东西——空的时候一切照旧（文本仍是 0 号块）。
    thinking_buf: String,
    tool_calls: Vec<PartialToolCall>,
    stop_reason: StopReason,
    usage: Usage,
    announced_message_start: bool,
}

fn finalise(s: &mut StreamState) {
    let mut content: Vec<ContentBlock> = Vec::new();
    let mut truncated_tool_ids = Vec::new();
    // 块下标就是这里数出来的：思考（有才有）→ 文本（有才有）→ 工具。底下三处
    // 发 `BlockStop` 的顺序必须和 `content` 的顺序一致，否则前端会拿错块。
    let mut next_index = 0usize;
    if !s.thinking_buf.trim().is_empty() {
        content.push(ContentBlock::Thinking {
            thinking: std::mem::take(&mut s.thinking_buf),
            signature: None,
        });
        s.pending
            .push_back(Ok(StreamEvent::BlockStop { index: next_index }));
        next_index += 1;
    }
    if !s.text_buf.is_empty() {
        content.push(ContentBlock::Text {
            text: std::mem::take(&mut s.text_buf),
        });
        s.pending
            .push_back(Ok(StreamEvent::BlockStop { index: next_index }));
        next_index += 1;
    }
    for c in s.tool_calls.drain(..) {
        let block_index = next_index;
        next_index += 1;
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
    // 缓存命中：OpenAI 标准放在 `prompt_tokens_details.cached_tokens`，
    // DeepSeek 另有自家的 `prompt_cache_hit_tokens`。两条都读——没有这个数，
    // 「每轮重发整段历史到底花了多少」就只能靠猜（原来这里写死 0）。
    let cached = v
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|n| n.as_u64())
        .or_else(|| v.get("prompt_cache_hit_tokens").and_then(|n| n.as_u64()))
        .unwrap_or(0) as u32;
    let (input_tokens, cache_read_tokens) = split_input(prompt, cached);
    Some(Usage {
        input_tokens,
        output_tokens: completion,
        cache_read_tokens,
        cache_creation_tokens: 0,
    })
}

/// 唯一的换算点：把线上的「输入总数 + 其中命中缓存的数」拆成
/// [`Usage::input_tokens`]（新读进来的）与 [`Usage::cache_read_tokens`]（命中）。
///
/// 为什么要拆：`Usage` 的口径与 Anthropic 线对齐——那边 `input_tokens` 本来就不含
/// 缓存命中。OpenAI 兼容线的 `prompt_tokens` 是**含缓存**的总数，不减就会出现
/// 「输入翻了十倍、缓存命中 0」这种谁也读不懂的账。流式与非流式两条路都走这里，
/// 免得只有一条路算对。
fn split_input(prompt_tokens: u32, cached: u32) -> (u32, u32) {
    let cached = cached.min(prompt_tokens);
    (prompt_tokens - cached, cached)
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
            at: None,
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
            speaker: None,
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
            at: None,
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
            speaker: None,
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
            at: None,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "think...".into(),
                    signature: None,
                },
                ContentBlock::Text {
                    text: "hello".into(),
                },
            ],
            speaker: None,
        };
        let out = translate_outbound(&m);
        assert_eq!(out[0].content.as_deref(), Some("hello"));
        assert!(out[0].tool_calls.is_none());
    }

    /// 空 assistant 不上线：线格式里它只剩 `{"role":"assistant"}`，端点 400。
    #[test]
    fn an_empty_assistant_message_never_reaches_the_wire() {
        let empty = Message {
            role: Role::Assistant,
            at: None,
            content: Vec::new(),
            speaker: None,
        };
        assert!(translate_outbound(&empty).is_empty());

        let thinking_only = Message {
            role: Role::Assistant,
            at: None,
            content: vec![ContentBlock::Thinking {
                thinking: "想".into(),
                signature: None,
            }],
            speaker: None,
        };
        assert!(
            translate_outbound(&thinking_only).is_empty(),
            "思考块落线时被丢掉，整条就没有内容了"
        );
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

    /// 缓存命中必须被读出来：没有这个数，「每轮重发整段历史到底贵不贵」只能靠猜
    /// （这里原来写死 0，于是命中率永远是 0%，谁也看不见缓存有没有生效）。
    #[test]
    fn cache_hits_are_read_from_both_wire_shapes() {
        // OpenAI 标准：prompt_tokens 含缓存，input 要减掉。
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"x"}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":20,
            "prompt_tokens_details":{"cached_tokens":900}}}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let u = parsed.into_completion().usage;
        assert_eq!(u.cache_read_tokens, 900);
        assert_eq!(u.input_tokens, 100, "input_tokens 是新读进来的那部分");
        assert_eq!(u.output_tokens, 20);

        // DeepSeek 自家字段，且没有 prompt_tokens_details。
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"x"}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":20,
            "prompt_cache_hit_tokens":750,"prompt_cache_miss_tokens":250}}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let u = parsed.into_completion().usage;
        assert_eq!(u.cache_read_tokens, 750);
        assert_eq!(u.input_tokens, 250);

        // 没有缓存字段（老端点）→ 命中 0，输入就是全部，不许出现下溢。
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"x"}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":20}}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let u = parsed.into_completion().usage;
        assert_eq!(u.cache_read_tokens, 0);
        assert_eq!(u.input_tokens, 1000);
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

    /// 推理模型的思考放在 `reasoning_content` 里。2026-09-20 之前这里根本不解析，
    /// 于是「一句话不说卡了八分钟」的那一轮，推理整段被丢掉：用户既看不到它在想
    /// 什么，也看不到它在干活，只有一个不动的界面。
    #[test]
    fn reasoning_content_decodes_to_a_thinking_block_ahead_of_the_text() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "先看窗口，再定板块……",
                    "content": "【A】新华财经｜2026-09-20 09:27｜LPR 不变"
                },
                "finish_reason": "stop"
            }]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let comp = parsed.into_completion();
        assert_eq!(comp.content.len(), 2, "{:?}", comp.content);
        assert!(
            matches!(&comp.content[0], ContentBlock::Thinking { thinking, .. }
                if thinking == "先看窗口，再定板块……")
        );
        assert!(
            matches!(&comp.content[1], ContentBlock::Text { text } if text.starts_with("【A】"))
        );
        let msg = Message {
            role: Role::Assistant,
            at: None,
            content: comp.content,
            speaker: None,
        };
        assert!(msg.has_sendable_content(), "有正文就算能发");
        assert_eq!(
            translate_outbound(&msg)[0].content.as_deref(),
            Some("【A】新华财经｜2026-09-20 09:27｜LPR 不变"),
            "思考块不许上线"
        );
    }

    /// 只有推理、一个字正文都没有（推理把预算吃光）——这条**不算内容**：
    /// 放上线端点就 400，留在历史里会让整个会话之后每次请求都 400。
    #[test]
    fn a_reasoning_only_response_is_not_sendable_content() {
        let raw = r#"{"choices":[{"message":{"role":"assistant",
            "reasoning_content":"想了很久很久"},"finish_reason":"length"}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let comp = parsed.into_completion();
        assert_eq!(comp.stop_reason, StopReason::MaxTokens);
        let msg = Message {
            role: Role::Assistant,
            at: None,
            content: comp.content,
            speaker: None,
        };
        assert!(!msg.has_sendable_content());
        assert!(translate_outbound(&msg).is_empty());
    }

    async fn collect_stream(sse: &'static str) -> Vec<StreamEvent> {
        use futures::StreamExt;
        let bytes: futures::stream::BoxStream<
            'static,
            std::result::Result<bytes::Bytes, reqwest::Error>,
        > = Box::pin(futures::stream::iter(vec![Ok(bytes::Bytes::from_static(
            sse.as_bytes(),
        ))]));
        let mut s = Box::pin(parse_openai_stream(bytes));
        let mut out = Vec::new();
        while let Some(ev) = s.next().await {
            out.push(ev.expect("stream event"));
        }
        out
    }

    /// 流式里的 `delta.reasoning_content` 要变成 ThinkingDelta，正文块下标顺延。
    #[tokio::test]
    async fn streaming_reasoning_becomes_thinking_deltas() {
        let events = collect_stream(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"想一\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"想二\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"content\":\"答案\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
             data: [DONE]\n\n",
        )
        .await;

        let thinking: Vec<&StreamEvent> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::ThinkingDelta { .. }))
            .collect();
        assert_eq!(thinking.len(), 2, "{events:?}");

        let text_index = events.iter().find_map(|e| match e {
            StreamEvent::TextDelta { index, .. } => Some(*index),
            _ => None,
        });
        assert_eq!(
            text_index,
            Some(1),
            "思考占用 0 号块，文本要顺延到 1 号：{events:?}"
        );

        let final_resp = events.iter().find_map(|e| match e {
            StreamEvent::Final(r) => Some(r),
            _ => None,
        });
        let content = &final_resp.expect("Final").content;
        assert_eq!(content.len(), 2, "{content:?}");
        assert!(
            matches!(&content[0], ContentBlock::Thinking { thinking, .. } if thinking == "想一想二")
        );
        assert!(matches!(&content[1], ContentBlock::Text { text } if text == "答案"));
    }

    /// 没有推理的普通流不该被这条改动碰到：文本仍是 0 号块，工具还是跟在它后面。
    #[tokio::test]
    async fn a_stream_without_reasoning_keeps_its_old_numbering() {
        let events = collect_stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"答案\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\
             \"function\":{\"name\":\"web_fetch\",\"arguments\":\"{}\"}}]}}]}\n\n\
             data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        )
        .await;

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::ThinkingDelta { .. })),
            "没有 reasoning_content 就不该冒思考块"
        );
        let text_index = events.iter().find_map(|e| match e {
            StreamEvent::TextDelta { index, .. } => Some(*index),
            _ => None,
        });
        assert_eq!(text_index, Some(0));
        let final_resp = events.iter().find_map(|e| match e {
            StreamEvent::Final(r) => Some(r),
            _ => None,
        });
        let content = &final_resp.expect("Final").content;
        assert!(
            matches!(&content[0], ContentBlock::Text { .. }),
            "{content:?}"
        );
        assert!(
            matches!(&content[1], ContentBlock::ToolUse { .. }),
            "{content:?}"
        );
    }
}
