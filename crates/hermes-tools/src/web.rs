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
            SearchBackend::Searxng if self.searxng_url.trim().is_empty() => {
                SearchBackend::Scraper
            }
            other => other,
        }
    }
}

/// Resolve the cache TTL from an optional context, falling back to the default.
pub fn ttl_or_default(ctx: Option<&WebToolsContext>) -> Duration {
    ctx.map(|c| c.cache_ttl())
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_CACHE_TTL_SECS))
}
