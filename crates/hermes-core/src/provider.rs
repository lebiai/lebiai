//! LLM provider trait and request / response types.

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    /// 这一轮**真正新读进来**的输入 token —— 不含缓存命中的那部分。
    /// 各家线的口径在这里统一：Anthropic 的 `input_tokens` 本来就不含缓存；
    /// OpenAI 兼容线的 `prompt_tokens` 是含缓存的总数，provider 负责先减掉
    /// （见 `hermes-llm/src/openai.rs::parse_usage`）。
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// 命中提示词缓存、没重新计费的那部分输入。用来算命中率，
    /// 也是「每轮重发整段历史到底贵不贵」唯一可核的数。
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
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let resp = self.complete(req).await?;
        let s = futures::stream::iter(vec![Ok(StreamEvent::Final(resp))]);
        Ok(Box::pin(s))
    }

    fn capabilities(&self) -> Capabilities;

    /// Human-readable provider tag for logging/UX, e.g. `"anthropic"` or
    /// `"openai"`. Not stable as a config key.
    fn name(&self) -> &str;
}

/// Refuses to send anything once the license is locked.
///
/// Wrapped on by the single provider assembly point (`hermes_llm::Config::
/// build_active_provider`), so GUI / server / CLI / IM all inherit the same
/// gate: an expired license cannot buy a single token from any surface, and
/// no new surface can forget to add the check.
///
/// The check is read-only (see [`crate::license::can_use_main_readonly`]) —
/// this runs on every request, and must never write `license.json`.
pub struct LicenseGatedProvider {
    inner: Arc<dyn LlmProvider>,
    check: Arc<dyn Fn() -> bool + Send + Sync>,
}

type LicenseCheck = Arc<dyn Fn() -> bool + Send + Sync>;

impl LicenseGatedProvider {
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self::with_check(inner, Arc::new(crate::license::can_use_main_readonly))
    }

    /// Same gate with an injected check — only so tests can prove both
    /// directions without poking the global license file.
    pub fn with_check(inner: Arc<dyn LlmProvider>, check: LicenseCheck) -> Self {
        Self { inner, check }
    }

    fn allowed(&self) -> Result<()> {
        if (self.check)() {
            Ok(())
        } else {
            Err(crate::Error::Config(crate::license::LOCKED_MESSAGE.into()))
        }
    }
}

#[async_trait]
impl LlmProvider for LicenseGatedProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.allowed()?;
        self.inner.complete(req).await
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        self.allowed()?;
        self.inner.stream(req).await
    }

    // Capabilities and name describe the provider, not the permission — the
    // UI still needs them to render the model picker while locked.
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
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
    TextDelta {
        index: usize,
        text: String,
    },
    ThinkingDelta {
        index: usize,
        text: String,
    },
    ToolUseStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolUseInputDelta {
        index: usize,
        partial_json: String,
    },
    BlockStop {
        index: usize,
    },
    Final(CompletionResponse),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Records whether the inner provider was actually reached.
    struct CountingProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for CountingProvider {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: "sent".into(),
                }],
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
                truncated_tool_ids: Vec::new(),
            })
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: true,
            }
        }

        fn name(&self) -> &str {
            "counting"
        }
    }

    fn req() -> CompletionRequest {
        CompletionRequest {
            model: "m".into(),
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: Vec::new(),
            max_tokens: 16,
            temperature: None,
            enable_caching: false,
        }
    }

    fn gated(licensed: bool) -> (LicenseGatedProvider, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(CountingProvider {
            calls: calls.clone(),
        });
        let check: LicenseCheck = Arc::new(move || licensed);
        (LicenseGatedProvider::with_check(inner, check), calls)
    }

    #[tokio::test]
    async fn locked_license_never_reaches_the_provider() {
        let (p, calls) = gated(false);

        let err = p.complete(req()).await.unwrap_err();

        assert!(
            err.to_string().contains(crate::license::LOCKED_MESSAGE),
            "锁定时必须抛出可与前端对齐的授权错误，实际：{err}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "锁定还发请求 = 门禁是假的");

        // stream 也必须被拦（默认实现会调 complete，但守卫不能用默认实现兜底）
        assert!(p.stream(req()).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0, "stream 绕过了门禁");
    }

    #[tokio::test]
    async fn licensed_license_passes_through_untouched() {
        let (p, calls) = gated(true);

        let resp = p.complete(req()).await.unwrap();

        assert_eq!(resp.text(), "sent");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(p.name(), "counting");
        assert!(p.capabilities().tool_use);
    }
}
