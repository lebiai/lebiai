//! Shared runtime context for the `web_fetch` and `web_search` tools.
//!
//! Constructed once at startup and injected into [`crate::BuiltinToolHost`] via
//! `with_web_ctx()`, mirroring how [`crate::SubagentContext`] is wired. Carries:
//! - the LLM provider used for `web_fetch` prompt-extraction (typically the
//!   main provider, reused — see [`WebToolsContext::extract_model`]),
//! - the selected `web_search` backend and its API keys,
//! - the cache TTL applied to both tools.
//!
//! When no context is injected, both tools still work: `web_fetch` returns
//! cleaned markdown (no LLM extraction) and `web_search` uses the scraper
//! backend, both with a default cache TTL.

use std::sync::Arc;
use std::time::Duration;

use hermes_core::LlmProvider;

/// Default cache TTL when no context is configured.
pub const DEFAULT_CACHE_TTL_SECS: u64 = 900;

/// `max_tokens` budget for `web_fetch` prompt-extraction answers.
///
/// 2048 was too small once the configured模型开始输出推理：预算会被看不见的
/// `reasoning_content` 整段吃掉，正文回空串、`finish_reason=length`，工具于是
/// 静默退回「整页 20 000 字原文」。2026-09-20 实测（新华财经首页，正文 48 000 字
/// 截断喂入）：2048 → 正文 0 字（2048 tokens 全是推理）；8192 → 正文 6 620 字。
pub const DEFAULT_EXTRACT_MAX_TOKENS: u32 = 8192;

/// 抽取回空串后的重试倍率：预算 ×2（8192 → 16384），**只重试一次**。
///
/// 2026-09-21 一轮实跑：上证报 / 证券时报 / e公司 / 券商中国 / 第一财经 / 财新
/// 六个站当轮全栽在「首页抽取预算被推理吃光、正文回空串」上，翻到 16384 就把
/// 正文拿回来了。同一条规矩在主循环里已经立过，见
/// `docs/records/20260920-reasoning-visible-and-budget.md`。
pub const EXTRACT_RETRY_FACTOR: u32 = 2;

/// 抽取重试预算的封顶。重试只有一次，这条线只防「配置里写了个离谱的大数还往上
/// 翻」—— 推理吃掉的那点预算是有上限的，翻过 32 768 只是烧钱。
pub const EXTRACT_RETRY_MAX_TOKENS: u32 = 32_768;

/// Which backend `web_search` uses first (failures cascade to free fallbacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchBackend {
    /// Scrape Brave Search HTML (no API key required). Default.
    #[default]
    Scraper,
    /// Tavily Search API — returns a synthesised answer plus clean results.
    Tavily,
    /// Brave Search API (structured JSON; requires a subscription token).
    BraveApi,
    /// Self-hosted or public [SearXNG](https://docs.searxng.org/) instance (JSON).
    /// Free & open source; best long-term default for local-first installs.
    Searxng,
}

impl SearchBackend {
    /// Parse a config string. Unknown values fall back to [`SearchBackend::Scraper`].
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "tavily" => SearchBackend::Tavily,
            "brave_api" | "braveapi" | "brave" => SearchBackend::BraveApi,
            "searxng" | "searx" => SearchBackend::Searxng,
            _ => SearchBackend::Scraper,
        }
    }
}

/// Runtime wiring for the web tools. Cheap to clone the `Arc<dyn LlmProvider>`.
pub struct WebToolsContext {
    /// Provider used for `web_fetch` prompt-extraction. Normally the same
    /// provider instance the main loop uses.
    pub extract_provider: Arc<dyn LlmProvider>,
    /// Model id for extraction requests. Empty string → the provider's own
    /// default model (both providers honour a non-empty `req.model` and fall
    /// back to their constructed model when it is empty).
    pub extract_model: String,
    /// `max_tokens` budget for the extracted answer.
    pub extract_max_tokens: u32,
    /// Selected search backend.
    pub search_backend: SearchBackend,
    /// Tavily API key (empty → fall back to scraper).
    pub tavily_api_key: String,
    /// Brave Search API subscription token (empty → fall back to scraper).
    pub brave_api_key: String,
    /// SearXNG base URL, e.g. `http://127.0.0.1:8080` or a trusted public instance.
    /// Empty → SearXNG backend is skipped unless URL is set.
    pub searxng_url: String,
    /// Cache TTL in seconds for fetch/search results.
    pub cache_ttl_secs: u64,
}

impl WebToolsContext {
    /// Cache TTL as a [`Duration`].
    pub fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttl_secs)
    }

    /// The effective search backend, downgraded to [`SearchBackend::Scraper`]
    /// when the selected API backend has no key configured.
    pub fn effective_backend(&self) -> SearchBackend {
        match self.search_backend {
            SearchBackend::Tavily if self.tavily_api_key.trim().is_empty() => {
                SearchBackend::Scraper
            }
            SearchBackend::BraveApi if self.brave_api_key.trim().is_empty() => {
                SearchBackend::Scraper
            }
            SearchBackend::Searxng if self.searxng_url.trim().is_empty() => SearchBackend::Scraper,
            other => other,
        }
    }
}

/// Resolve the cache TTL from an optional context, falling back to the default.
pub fn ttl_or_default(ctx: Option<&WebToolsContext>) -> Duration {
    ctx.map(|c| c.cache_ttl())
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_CACHE_TTL_SECS))
}
