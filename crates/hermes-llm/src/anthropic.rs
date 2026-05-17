//! Anthropic Messages-API client.
//!
//! Compatible with both Anthropic's own API and DeepSeek's Anthropic-format
//! endpoint (`https://api.deepseek.com/anthropic`). The only difference at
//! the wire level is the `base_url`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use hermes_core::{
    Capabilities, CompletionRequest, CompletionResponse, ContentBlock, Error, LlmProvider,
    Message, Result, StopReason, StreamEvent, ToolSpec, Usage,
};
use serde::{Deserialize, Serialize};

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Concrete Anthropic-protocol provider. Cheap to clone.
#[derive(Clone)]
pub struct AnthropicProvider {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
    supports_caching: bool,
}

impl AnthropicProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        supports_caching: bool,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(300))
            .user_agent(format!("small-rust-hermes/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Provider(format!("building http client: {e}")))?;

        Ok(Self {
            inner: Arc::new(Inner {
                client,
                base_url: base_url.into().trim_end_matches('/').to_string(),
                api_key: api_key.into(),
                default_model: default_model.into(),
                supports_caching,
            }),
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let model = if req.model.is_empty() {
            self.inner.default_model.clone()
        } else {
            req.model.clone()
        };

        let body = AnthropicRequest {
            model,
            max_tokens: req.max_tokens,
            system: req.system,
            messages: req.messages,
            tools: if req.tools.is_empty() { None } else { Some(req.tools) },
            temperature: req.temperature,
            stream: false,
        };

        let url = format!("{}/v1/messages", self.inner.base_url);
        tracing::debug!(url = %url, model = %body.model, "anthropic request");

        let resp = self
            .inner
            .client
            .post(&url)
            .header("x-api-key", &self.inner.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("http send: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("HTTP {status}: {text}")));
        }

        let parsed: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| Error::Provider(format!("decoding response: {e}")))?;

        Ok(parsed.into())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_use: true,
            prompt_caching: self.inner.supports_caching,
            streaming: true,
        }
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let model = if req.model.is_empty() {
            self.inner.default_model.clone()
        } else {
            req.model.clone()
        };

        let body = AnthropicRequest {
            model,
            max_tokens: req.max_tokens,
            system: req.system,
            messages: req.messages,
            tools: if req.tools.is_empty() { None } else { Some(req.tools) },
            temperature: req.temperature,
            stream: true,
        };

        let url = format!("{}/v1/messages", self.inner.base_url);
        tracing::debug!(url = %url, model = %body.model, "anthropic stream request");

        let resp = self
            .inner
            .client
            .post(&url)
            .header("x-api-key", &self.inner.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("http send: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("HTTP {status}: {text}")));
        }

        Ok(Box::pin(parse_sse_stream(resp.bytes_stream().boxed())))
    }
}

// ---- SSE streaming -----------------------------------------------------

/// Decode an Anthropic SSE byte-stream into [`StreamEvent`]s.
///
/// Buffers lines until a blank line ends an event, then dispatches by the
/// `event:` field. Accumulates content blocks so a final
/// [`StreamEvent::Final`] can be emitted with the full
/// [`CompletionResponse`].
fn parse_sse_stream(
    bytes: BoxStream<'static, std::result::Result<bytes::Bytes, reqwest::Error>>,
) -> impl futures::Stream<Item = Result<StreamEvent>> {
    use futures::stream;

    // We use an `unfold`-style stream that owns mutable buffers between
    // polls. Easiest expressed as an async-stream via `stream::unfold`.
    struct State {
        bytes: BoxStream<'static, std::result::Result<bytes::Bytes, reqwest::Error>>,
        // Pending bytes we haven't yet split into lines.
        line_buf: String,
        // Pending events emitted but not yet drained to the consumer.
        pending: std::collections::VecDeque<Result<StreamEvent>>,
        // Aggregated state for synthesising Final.
        blocks: Vec<BlockBuilder>,
        usage: Usage,
        stop_reason: StopReason,
        finished: bool,
        // The "event:" field of the line we are currently parsing.
        current_event: String,
    }

    let state = State {
        bytes,
        line_buf: String::new(),
        pending: std::collections::VecDeque::new(),
        blocks: Vec::new(),
        usage: Usage::default(),
        stop_reason: StopReason::Other,
        finished: false,
        current_event: String::new(),
    };

    stream::unfold(state, |mut s| async move {
        loop {
            if let Some(ev) = s.pending.pop_front() {
                return Some((ev, s));
            }
            if s.finished {
                return None;
            }

            // Pull more bytes.
            match s.bytes.next().await {
                Some(Ok(chunk)) => match std::str::from_utf8(&chunk) {
                    Ok(text) => s.line_buf.push_str(text),
                    Err(_) => {
                        // Partial UTF-8 at chunk boundary — try lossy decode
                        // rather than aborting the entire stream.
                        let text = String::from_utf8_lossy(&chunk);
                        s.line_buf.push_str(&text);
                    }
                },
                Some(Err(e)) => {
                    // Transient network/decode error on one chunk. Log and
                    // try to continue — the stream may recover on the next
                    // chunk. Only fatal if the stream itself is closed (None).
                    tracing::debug!(error=%e, "stream chunk error (continuing)");
                    continue;
                }
                None => {
                    // EOF — flush whatever line is buffered, then emit Final.
                    if !s.line_buf.is_empty() {
                        let line_buf = std::mem::take(&mut s.line_buf);
                        process_lines(&line_buf, &mut s.current_event, &mut s.pending, &mut s.blocks, &mut s.usage, &mut s.stop_reason);
                    }
                    s.pending.push_back(Ok(StreamEvent::Final(build_response(
                        std::mem::take(&mut s.blocks),
                        s.usage,
                        s.stop_reason,
                    ))));
                    s.finished = true;
                    continue;
                }
            }

            // Drain whole lines from line_buf.
            while let Some(nl) = s.line_buf.find('\n') {
                let mut line = s.line_buf[..nl].to_string();
                s.line_buf.drain(..=nl);
                if line.ends_with('\r') {
                    line.pop();
                }
                handle_sse_line(
                    &line,
                    &mut s.current_event,
                    &mut s.pending,
                    &mut s.blocks,
                    &mut s.usage,
                    &mut s.stop_reason,
                );
            }
        }
    })
}

/// Process a chunk of fully-buffered lines (used at EOF).
fn process_lines(
    chunk: &str,
    current_event: &mut String,
    pending: &mut std::collections::VecDeque<Result<StreamEvent>>,
    blocks: &mut Vec<BlockBuilder>,
    usage: &mut Usage,
    stop_reason: &mut StopReason,
) {
    for line in chunk.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        handle_sse_line(line, current_event, pending, blocks, usage, stop_reason);
    }
}

fn handle_sse_line(
    line: &str,
    current_event: &mut String,
    pending: &mut std::collections::VecDeque<Result<StreamEvent>>,
    blocks: &mut Vec<BlockBuilder>,
    usage: &mut Usage,
    stop_reason: &mut StopReason,
) {
    if line.is_empty() {
        // Blank line ends an event; just clear the current_event tag.
        current_event.clear();
        return;
    }
    if let Some(ev) = line.strip_prefix("event:") {
        *current_event = ev.trim().to_string();
        return;
    }
    if let Some(payload) = line.strip_prefix("data:") {
        let payload = payload.trim_start();
        if payload.is_empty() {
            return;
        }
        let parsed: std::result::Result<serde_json::Value, _> = serde_json::from_str(payload);
        match parsed {
            Ok(v) => dispatch_event(current_event, &v, pending, blocks, usage, stop_reason),
            Err(e) => {
                tracing::warn!(error=%e, line=%payload, "skipping unparseable SSE data");
            }
        }
    }
    // Other prefixes (`id:`, `retry:`, `:` comments) — ignore.
}

fn dispatch_event(
    event_name: &str,
    v: &serde_json::Value,
    pending: &mut std::collections::VecDeque<Result<StreamEvent>>,
    blocks: &mut Vec<BlockBuilder>,
    usage: &mut Usage,
    stop_reason: &mut StopReason,
) {
    // Anthropic puts the type in both `event:` and `data.type`. We trust
    // `data.type` because some proxies omit `event:`.
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or(event_name);

    match ty {
        "message_start" => {
            // Initial usage may be present (input_tokens). Carry it.
            if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                *usage = parse_usage(u);
            }
            pending.push_back(Ok(StreamEvent::MessageStart));
        }
        "content_block_start" => {
            let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let cb = v.get("content_block");
            let block_type = cb.and_then(|b| b.get("type")).and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    ensure_block(blocks, index, BlockBuilder::Text(String::new()));
                }
                "thinking" => {
                    ensure_block(
                        blocks,
                        index,
                        BlockBuilder::Thinking {
                            text: String::new(),
                            signature: None,
                        },
                    );
                }
                "tool_use" => {
                    let id = cb
                        .and_then(|b| b.get("id"))
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = cb
                        .and_then(|b| b.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    ensure_block(
                        blocks,
                        index,
                        BlockBuilder::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            partial_json: String::new(),
                        },
                    );
                    pending.push_back(Ok(StreamEvent::ToolUseStart { index, id, name }));
                }
                _ => {
                    tracing::debug!(block_type, "unhandled content_block_start");
                }
            }
        }
        "content_block_delta" => {
            let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let delta = v.get("delta");
            let dt = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");
            match dt {
                "text_delta" => {
                    let text = delta
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(BlockBuilder::Text(buf)) = blocks.get_mut(index) {
                        buf.push_str(&text);
                    }
                    pending.push_back(Ok(StreamEvent::TextDelta { index, text }));
                }
                "thinking_delta" => {
                    let text = delta
                        .and_then(|d| d.get("thinking"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(BlockBuilder::Thinking { text: buf, .. }) = blocks.get_mut(index) {
                        buf.push_str(&text);
                    }
                    pending.push_back(Ok(StreamEvent::ThinkingDelta { index, text }));
                }
                "input_json_delta" => {
                    let partial = delta
                        .and_then(|d| d.get("partial_json"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(BlockBuilder::ToolUse { partial_json, .. }) = blocks.get_mut(index)
                    {
                        partial_json.push_str(&partial);
                    }
                    pending.push_back(Ok(StreamEvent::ToolUseInputDelta {
                        index,
                        partial_json: partial,
                    }));
                }
                "signature_delta" => {
                    let sig = delta
                        .and_then(|d| d.get("signature"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(BlockBuilder::Thinking { signature, .. }) = blocks.get_mut(index) {
                        *signature = Some(sig);
                    }
                }
                _ => {
                    tracing::debug!(delta_type = dt, "unhandled content_block_delta");
                }
            }
        }
        "content_block_stop" => {
            let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            pending.push_back(Ok(StreamEvent::BlockStop { index }));
        }
        "message_delta" => {
            if let Some(d) = v.get("delta") {
                if let Some(sr) = d.get("stop_reason").and_then(|s| s.as_str()) {
                    *stop_reason = match sr {
                        "end_turn" => StopReason::EndTurn,
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "stop_sequence" => StopReason::StopSequence,
                        _ => StopReason::Other,
                    };
                }
            }
            if let Some(u) = v.get("usage") {
                let new_usage = parse_usage(u);
                // message_delta usage replaces output_tokens, keeps input.
                usage.output_tokens = new_usage.output_tokens;
                if new_usage.cache_read_tokens > 0 {
                    usage.cache_read_tokens = new_usage.cache_read_tokens;
                }
                if new_usage.cache_creation_tokens > 0 {
                    usage.cache_creation_tokens = new_usage.cache_creation_tokens;
                }
            }
        }
        "message_stop" => {
            // Finalisation happens at EOF; nothing to push here.
        }
        "ping" | "" => {}
        other => {
            tracing::debug!(ty = other, "unhandled SSE event type");
        }
    }
}

fn ensure_block(blocks: &mut Vec<BlockBuilder>, index: usize, init: BlockBuilder) {
    while blocks.len() <= index {
        blocks.push(BlockBuilder::Placeholder);
    }
    blocks[index] = init;
}

fn parse_usage(v: &serde_json::Value) -> Usage {
    Usage {
        input_tokens: v
            .get("input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: v
            .get("output_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        cache_read_tokens: v
            .get("cache_read_input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        cache_creation_tokens: v
            .get("cache_creation_input_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
    }
}

fn build_response(
    blocks: Vec<BlockBuilder>,
    usage: Usage,
    stop_reason: StopReason,
) -> CompletionResponse {
    let content = blocks
        .into_iter()
        .filter_map(|b| match b {
            BlockBuilder::Placeholder => None,
            BlockBuilder::Text(text) => Some(ContentBlock::Text { text }),
            BlockBuilder::Thinking { text, signature } => Some(ContentBlock::Thinking {
                thinking: text,
                signature,
            }),
            BlockBuilder::ToolUse {
                id,
                name,
                partial_json,
            } => {
                let input = if partial_json.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(&partial_json).unwrap_or_else(|e| {
                        tracing::warn!(error=%e, "failed to parse tool_use partial_json; using empty object");
                        serde_json::json!({})
                    })
                };
                Some(ContentBlock::ToolUse { id, name, input })
            }
        })
        .collect();
    CompletionResponse {
        content,
        stop_reason,
        usage,
    }
}

#[derive(Debug, Clone)]
enum BlockBuilder {
    Placeholder,
    Text(String),
    Thinking {
        text: String,
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        partial_json: String,
    },
}

// ---- wire types --------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Deserialize, Debug)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize, Debug, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

impl From<AnthropicResponse> for CompletionResponse {
    fn from(r: AnthropicResponse) -> Self {
        let stop_reason = match r.stop_reason.as_deref() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("stop_sequence") => StopReason::StopSequence,
            _ => StopReason::Other,
        };
        let usage = r.usage.map(|u| Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_read_tokens: u.cache_read_input_tokens,
            cache_creation_tokens: u.cache_creation_input_tokens,
        }).unwrap_or_default();

        CompletionResponse {
            content: r.content,
            stop_reason,
            usage,
        }
    }
}
