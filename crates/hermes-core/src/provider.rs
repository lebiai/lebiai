//! LLM provider trait and request / response types.

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::message::{ContentBlock, Message};
use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    /// System prompt — single string, applied at the start of context.
    /// Multi-block / cache-control system messages are an Anthropic extension
    /// the provider may use internally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// If true, providers that support prompt caching should attempt to cache
    /// the system + tools + leading user turns. Providers without caching
    /// support silently ignore this.
    #[serde(default)]
    pub enable_caching: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool's input.
    pub input_schema: serde_json::Value,
    /// If true, the turn loop must prompt the user before executing this
    /// tool (subject to the active `PermissionChecker`). Builtin tools
    /// declare their own value; MCP-provided tools default to true since
    /// they typically perform external side-effects.
    #[serde(default)]
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// Tool use IDs whose input JSON was truncated/unparseable (e.g. max_tokens hit mid-call).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncated_tool_ids: Vec<String>,
}

impl CompletionResponse {
    /// Concatenate all text blocks. Returns empty string if none.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| b.as_text())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Other,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_read_tokens: u32,
    #[serde(default)]
    pub cache_creation_tokens: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    pub tool_use: bool,
    pub prompt_caching: bool,
    pub streaming: bool,
}

/// Abstraction over chat-completion providers (Anthropic, OpenAI-compatible, ...).
///
/// Implementations should be cheap to clone (typically wrap an
/// `Arc<reqwest::Client>` plus credentials).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;

    /// Stream a completion. The default implementation falls back to
    /// [`Self::complete`] and synthesises a single [`StreamEvent::Final`]
    /// — providers without native streaming still satisfy the contract.
    async fn stream(&self, req: CompletionRequest) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let resp = self.complete(req).await?;
        let s = futures::stream::iter(vec![Ok(StreamEvent::Final(resp))]);
        Ok(Box::pin(s))
    }

    fn capabilities(&self) -> Capabilities;

    /// Human-readable provider tag for logging/UX, e.g. `"anthropic"` or
    /// `"openai"`. Not stable as a config key.
    fn name(&self) -> &str;
}

/// Events emitted by [`LlmProvider::stream`].
///
/// The stream's contract:
/// - `MessageStart` is always the first event when the provider supports
///   streaming.
/// - `TextDelta` / `ThinkingDelta` carry incremental text. `index`
///   identifies the content block they belong to.
/// - `ToolUseStart` declares a new tool_use block (index, id, name);
///   `ToolUseInputDelta` appends partial JSON for the same index.
/// - `Final` is always the last event and carries the fully-assembled
///   [`CompletionResponse`] for callers that prefer the buffered API.
///   Providers MUST emit exactly one `Final`.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageStart,
    TextDelta { index: usize, text: String },
    ThinkingDelta { index: usize, text: String },
    ToolUseStart { index: usize, id: String, name: String },
    ToolUseInputDelta { index: usize, partial_json: String },
    BlockStop { index: usize },
    Final(CompletionResponse),
}
