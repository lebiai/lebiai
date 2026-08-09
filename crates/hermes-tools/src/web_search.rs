//! `web_search` — search the web. Backends, selected via [`WebToolsContext`]:
//! - `Scraper` (default): scrape Brave Search HTML, no API key required.
//! - `Tavily`: Tavily Search API — returns a synthesised answer plus results.
//! - `BraveApi`: Brave Search API — structured JSON, requires a token.
//!
//! Results are cached for the configured TTL. When an API backend is selected
//! but its key is missing, it transparently falls back to the scraper.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::http_defaults::HTTP_CLIENT;
use crate::web::{ttl_or_default, SearchBackend, WebToolsContext};
use crate::web_cache;

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    5
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_search".into(),
        description: "Search the web for current information. Returns titles, URLs, and snippets \
            (and, on some backends, a synthesised answer). \
            Always search first before fetching — the snippet or answer is often enough, so \
            you may not need web_fetch at all. If you do, fetch at most 1–2 of the returned URLs. \
            Do not construct deep-link URLs yourself; use the URLs returned by this tool."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "limit": {"type": "integer", "description": "Max results (default 5)"}
            },
            "required": ["query"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(
    _workspace: &Path,
    args: serde_json::Value,
    ctx: Option<&WebToolsContext>,
) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search: bad args: {e}")))?;

    let ttl = ttl_or_default(ctx);
    let cache_key = web_cache::search_key(&a.query, a.limit);
    if let Some(cached) = web_cache::get(&cache_key, ttl) {
        return Ok(ToolCallOutcome {
            content: format!("(cached)\n{cached}"),
            is_error: false,
        });
    }

    let result = match ctx {
        Some(c) => match c.effective_backend() {
            SearchBackend::Tavily => tavily_search(&a.query, a.limit, &c.tavily_api_key).await,
            SearchBackend::BraveApi => brave_api_search(&a.query, a.limit, &c.brave_api_key).await,
            SearchBackend::Scraper => scraper_search(&a.query, a.limit).await,
        },
        None => scraper_search(&a.query, a.limit).await,
    };

    match result {
        Ok(content) if !content.trim().is_empty() => {
            web_cache::put(cache_key, content.clone());
            Ok(ToolCallOutcome {
                content,
                is_error: false,
            })
        }
        Ok(_) => Ok(ToolCallOutcome {
            content: format!(
                "No results for: \"{}\". Try rephrasing the query or using different keywords.",
                a.query
            ),
            is_error: true,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("web_search error: {e}"),
            is_error: true,
        }),
    }
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn format_results(results: &[SearchResult]) -> String {
    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if r.snippet.is_empty() {
                format!("{}. {} — {}", i + 1, r.title, r.url)
            } else {
                format!("{}. {} — {}\n   {}", i + 1, r.title, r.url, r.snippet)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drop results whose host already appeared, so a single domain can't fill the
/// list. Preserves order.
fn dedupe_by_domain(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        let host = host_of(&r.url);
        if seen.insert(host) {
            out.push(r);
        }
    }
    out
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

// ── Scraper backend ─────────────────────────────────────────────────────────

async fn scraper_search(query: &str, limit: usize) -> Result<String> {
    let url = format!(
        "https://search.brave.com/search?q={}",
        urlencoding::encode(query)
    );
    let resp = HTTP_CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search fetch: {e}")))?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let hint = if code == 429 {
            "Brave HTML scraper rate-limited. In ~/.lebi-ai/config.toml set \
             [web] search_backend = \"tavily\" and tavily_api_key = \"…\" \
             (or search_backend = \"brave_api\" and brave_api_key = \"…\"), then restart. \
             Or wait a few minutes and retry with fewer searches."
        } else {
            "Brave scraper failed; consider configuring a search API key under [web] in config.toml"
        };
        return Err(hermes_core::Error::ToolHost(format!(
            "HTTP {code} ({hint})"
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search body: {e}")))?;
    // Parse more than `limit` then dedupe by domain down to `limit`.
    let parsed = parse_brave_html(&body, limit * 3);
    let results: Vec<SearchResult> = dedupe_by_domain(parsed).into_iter().take(limit).collect();
    Ok(format_results(&results))
}

// ── Tavily backend ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

async fn tavily_search(query: &str, limit: usize, api_key: &str) -> Result<String> {
    let payload = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": limit,
        "include_answer": true,
        "include_raw_content": false,
    });
    let resp = HTTP_CLIENT
        .post("https://api.tavily.com/search")
        .json(&payload)
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("tavily request: {e}")))?;
    if !resp.status().is_success() {
        return Err(hermes_core::Error::ToolHost(format!(
            "tavily HTTP {}",
            resp.status()
        )));
    }
    let parsed: TavilyResponse = resp
        .json()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("tavily decode: {e}")))?;
    let results: Vec<SearchResult> = parsed
        .results
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content,
        })
        .collect();
    let mut out = String::new();
    if let Some(ans) = parsed.answer.filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("Answer: {ans}\n\nSources:\n"));
    }
    out.push_str(&format_results(&results));
    Ok(out)
}

// ── Brave Search API backend ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct BraveApiResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveApiResult>,
}

#[derive(Deserialize)]
struct BraveApiResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

async fn brave_api_search(query: &str, limit: usize, api_key: &str) -> Result<String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencoding::encode(query),
        limit
    );
    let resp = HTTP_CLIENT
        .get(&url)
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("brave api request: {e}")))?;
    if !resp.status().is_success() {
        return Err(hermes_core::Error::ToolHost(format!(
            "brave api HTTP {}",
            resp.status()
        )));
    }
    let parsed: BraveApiResponse = resp
        .json()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("brave api decode: {e}")))?;
    let results: Vec<SearchResult> = parsed
        .web
        .map(|w| w.results)
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.description,
        })
        .collect();
    Ok(format_results(&results))
}

// ── Brave HTML parsing (scraper backend) ──────────────────────────────────────

/// Parse Brave Search HTML results.
///
/// Brave wraps each organic result in a block marked with `data-type="web"`.
/// Within each block:
/// - URL: first `<a href="https://...">` element
/// - Title: `<div class="title ...">...</div>`
/// - Snippet: `<div class="generic-snippet ...">...</div>`
fn parse_brave_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let blocks: Vec<&str> = html.split("data-type=\"web\"").collect();

    for chunk in blocks.iter().skip(1) {
        if results.len() >= limit {
            break;
        }
        let url = match extract_first_https_href(chunk) {
            Some(u) => u,
            None => continue,
        };
        let title = extract_div_text(chunk, "title").unwrap_or_default();
        let snippet = extract_div_text(chunk, "generic-snippet").unwrap_or_default();
        if title.is_empty() && snippet.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }
    results
}

fn extract_first_https_href(chunk: &str) -> Option<String> {
    let marker = "href=\"https://";
    let pos = chunk.find(marker)?;
    let start = pos + 6; // skip `href="`
    let rest = &chunk[start..];
    let end = rest.find('"')?;
    let raw = &rest[..end];
    Some(decode_html_entities(raw))
}

fn extract_div_text(chunk: &str, class_prefix: &str) -> Option<String> {
    let pat = format!("class=\"{class_prefix}");
    let pos = chunk.find(&pat)?;
    let after = &chunk[pos..];
    let gt = after.find('>')?;
    let rest = &after[gt + 1..];
    let end = rest.find("</div>")?;
    let inner = &rest[..end];
    let text = strip_html_tags(inner);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    let decoded = decode_html_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_brave_results_basic() {
        let html = r#"<div data-type="web"><a href="https://www.reuters.com/technology/" target="_self"><div class="title search-snippet-title">Tech News | Reuters</div></a><div class="generic-snippet svelte-abc">Latest technology news from Reuters.</div></div><div data-type="web"><a href="https://apnews.com/technology" target="_self"><div class="title search-snippet-title">Technology | AP News</div></a><div class="generic-snippet svelte-abc">AP tech coverage.</div></div>"#;
        let results = parse_brave_html(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Tech News | Reuters");
        assert_eq!(results[0].url, "https://www.reuters.com/technology/");
        assert_eq!(results[0].snippet, "Latest technology news from Reuters.");
        assert_eq!(results[1].title, "Technology | AP News");
        assert_eq!(results[1].url, "https://apnews.com/technology");
    }

    #[test]
    fn empty_html() {
        let results = parse_brave_html("no results here", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn respects_limit() {
        let block = r#"<div data-type="web"><a href="https://example.com/" target="_self"><div class="title t">Example</div></a><div class="generic-snippet s">Desc.</div></div>"#;
        let html = block.repeat(10);
        let results = parse_brave_html(&html, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn html_entities_decoded() {
        let html = r#"<div data-type="web"><a href="https://example.com/a?b=1&amp;c=2" target="_self"><div class="title t">Tom &amp; Jerry</div></a><div class="generic-snippet s">A &lt;classic&gt; show.</div></div>"#;
        let results = parse_brave_html(html, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/a?b=1&c=2");
        assert_eq!(results[0].title, "Tom & Jerry");
        assert_eq!(results[0].snippet, "A <classic> show.");
    }

    #[test]
    fn dedupe_keeps_first_per_domain() {
        let results = vec![
            SearchResult {
                title: "a".into(),
                url: "https://x.com/1".into(),
                snippet: String::new(),
            },
            SearchResult {
                title: "b".into(),
                url: "https://x.com/2".into(),
                snippet: String::new(),
            },
            SearchResult {
                title: "c".into(),
                url: "https://y.com/1".into(),
                snippet: String::new(),
            },
        ];
        let deduped = dedupe_by_domain(results);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].url, "https://x.com/1");
        assert_eq!(deduped[1].url, "https://y.com/1");
    }
}
