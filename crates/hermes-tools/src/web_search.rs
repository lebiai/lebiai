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

    // Cascade: preferred backend → other configured APIs → Brave HTML → DuckDuckGo HTML.
    let result = search_with_fallback(ctx, &a.query, a.limit).await;

    match result {
        Ok((content, via)) if !content.trim().is_empty() => {
            web_cache::put(cache_key, content.clone());
            let prefix = if via == "primary" {
                String::new()
            } else {
                format!("(via {via} fallback)\n")
            };
            Ok(ToolCallOutcome {
                content: format!("{prefix}{content}"),
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

async fn search_with_fallback(
    ctx: Option<&WebToolsContext>,
    query: &str,
    limit: usize,
) -> Result<(String, &'static str)> {
    let mut errors: Vec<String> = Vec::new();

    // 1) Preferred backend
    if let Some(c) = ctx {
        let primary = match c.effective_backend() {
            SearchBackend::Tavily if !c.tavily_api_key.trim().is_empty() => {
                tavily_search(query, limit, &c.tavily_api_key)
                    .await
                    .map(|s| (s, "tavily"))
            }
            SearchBackend::BraveApi if !c.brave_api_key.trim().is_empty() => {
                brave_api_search(query, limit, &c.brave_api_key)
                    .await
                    .map(|s| (s, "brave_api"))
            }
            SearchBackend::Searxng if !c.searxng_url.trim().is_empty() => {
                searxng_search(query, limit, &c.searxng_url)
                    .await
                    .map(|s| (s, "searxng"))
            }
            SearchBackend::Scraper
            | SearchBackend::Searxng
            | SearchBackend::Tavily
            | SearchBackend::BraveApi => scraper_search(query, limit)
                .await
                .map(|s| (s, "brave_html")),
        };
        match primary {
            Ok(pair) if !pair.0.trim().is_empty() => return Ok(pair),
            Ok(_) => errors.push("primary returned empty".into()),
            Err(e) => errors.push(format!("primary: {e}")),
        }

        // 2) Other configured APIs / SearXNG
        if !c.searxng_url.trim().is_empty() {
            match searxng_search(query, limit, &c.searxng_url).await {
                Ok(s) if !s.trim().is_empty() => return Ok((s, "searxng")),
                Ok(_) => {}
                Err(e) => errors.push(format!("searxng: {e}")),
            }
        }
        if !c.tavily_api_key.trim().is_empty() {
            match tavily_search(query, limit, &c.tavily_api_key).await {
                Ok(s) if !s.trim().is_empty() => return Ok((s, "tavily")),
                Ok(_) => {}
                Err(e) => errors.push(format!("tavily: {e}")),
            }
        }
        if !c.brave_api_key.trim().is_empty() {
            match brave_api_search(query, limit, &c.brave_api_key).await {
                Ok(s) if !s.trim().is_empty() => return Ok((s, "brave_api")),
                Ok(_) => {}
                Err(e) => errors.push(format!("brave_api: {e}")),
            }
        }
    } else {
        match scraper_search(query, limit).await {
            Ok(s) if !s.trim().is_empty() => return Ok((s, "brave_html")),
            Ok(_) => errors.push("brave_html empty".into()),
            Err(e) => errors.push(format!("brave_html: {e}")),
        }
    }

    // 3) Free HTML scrapers (no key)
    match duckduckgo_search(query, limit).await {
        Ok(s) if !s.trim().is_empty() => return Ok((s, "duckduckgo")),
        Ok(_) => errors.push("duckduckgo empty".into()),
        Err(e) => errors.push(format!("duckduckgo: {e}")),
    }
    match bing_search(query, limit).await {
        Ok(s) if !s.trim().is_empty() => return Ok((s, "bing_html")),
        Ok(_) => errors.push("bing_html empty".into()),
        Err(e) => errors.push(format!("bing_html: {e}")),
    }

    // 4) Last resort: system `curl` (when reqwest/TLS path is blocked or rate-limited)
    match duckduckgo_search_curl(query, limit).await {
        Ok(s) if !s.trim().is_empty() => return Ok((s, "curl+duckduckgo")),
        Ok(_) => errors.push("curl+ddg empty".into()),
        Err(e) => errors.push(format!("curl+ddg: {e}")),
    }
    match bing_search_curl(query, limit).await {
        Ok(s) if !s.trim().is_empty() => return Ok((s, "curl+bing")),
        Ok(_) => errors.push("curl+bing empty".into()),
        Err(e) => errors.push(format!("curl+bing: {e}")),
    }

    Err(hermes_core::Error::ToolHost(format!(
        "all search backends failed: {}",
        errors.join(" | ")
    )))
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

/// A result is usable only if it is a real page a person could read.
/// Stylesheets, scripts, fonts, trackers, and CDN asset URLs are not hits.
fn is_usable_result(r: &SearchResult) -> bool {
    if r.url.trim().is_empty() {
        return false;
    }
    let url = r.url.to_ascii_lowercase();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return false;
    }
    if is_junk_url(&url) {
        return false;
    }
    if r.title.trim().is_empty() && r.snippet.trim().is_empty() {
        return false;
    }
    true
}

fn is_junk_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    const BAD_EXT: &[&str] = &[
        ".css", ".js", ".mjs", ".cjs", ".map", ".woff", ".woff2", ".ttf", ".eot", ".ico", ".less",
        ".scss", ".sass",
    ];
    if BAD_EXT.iter().any(|e| path.ends_with(e)) {
        return true;
    }
    const BAD_NEEDLES: &[&str] = &[
        "/css/",
        "/static/css",
        "/assets/css",
        "/gtm.js",
        "googletagmanager.com",
        "google-analytics.com",
        "doubleclick.net",
        "scorecardresearch.com",
        "pagead2.googlesyndication",
        "r.bing.com/",
        "www.bing.com/th?",
        "www.bing.com/rp/",
        "bing.com/ck/",
        "bing.com/dict",
        "hanyu.baidu.com",
        "dict.youdao.com",
        "zdic.net",
        "dict.cn/",
        "iciba.com",
        "statics.teams.cdn",
        "browser.events.data.microsoft.com",
        "cdn.search.brave.com/",
    ];
    BAD_NEEDLES.iter().any(|n| url.contains(n))
}

fn keep_usable(results: Vec<SearchResult>) -> Vec<SearchResult> {
    results.into_iter().filter(is_usable_result).collect()
}

fn keep_relevant(results: Vec<SearchResult>, query: &str) -> Vec<SearchResult> {
    keep_on_topic(keep_usable(results), query)
}

fn format_usable(results: Vec<SearchResult>, query: &str) -> Result<String> {
    let kept = keep_relevant(results, query);
    if kept.is_empty() {
        return Err(hermes_core::Error::ToolHost(
            "no usable on-topic results".into(),
        ));
    }
    Ok(format_results(&kept))
}

/// A hit must share the query's content, not just parse as a page.
/// Bing/DDG HTML often returns dictionary stubs for one CJK character
/// (「具」「人形」「银河系」) that would otherwise count as success.
fn keep_on_topic(results: Vec<SearchResult>, query: &str) -> Vec<SearchResult> {
    let tokens = query_content_tokens(query);
    if tokens.is_empty() {
        return results;
    }
    results
        .into_iter()
        .filter(|r| result_on_topic(r, &tokens))
        .collect()
}

struct QueryTokens {
    english: Vec<String>,
    bigrams: Vec<String>,
    trigrams: Vec<String>,
}

impl QueryTokens {
    fn is_empty(&self) -> bool {
        self.english.is_empty() && self.bigrams.is_empty() && self.trigrams.is_empty()
    }
}

fn query_content_tokens(query: &str) -> QueryTokens {
    const STOP_EN: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
        "our", "out", "has", "how", "what", "when", "who", "why", "from", "with", "this", "that",
        "have", "been", "they", "will", "about", "which", "their",
    ];
    const STOP_CJK: &[&str] = &[
        "最近", "有哪", "哪些", "什么", "怎么", "如何", "帮我", "查询", "一下", "今天", "这个",
        "那个", "可以", "一个", "我们", "他们", "自己", "进行", "相关", "关于", "以及", "或者",
        "如果", "因为", "所以", "但是", "还是", "不是", "没有", "就是", "请问", "看看", "搜搜",
        "找找", "下今", "的事", "有没", "没有", "一些", "这些", "那些", "是否", "需要", "帮查",
    ];

    let mut english = Vec::new();
    for w in query.split(|c: char| !c.is_ascii_alphanumeric()) {
        let w = w.to_ascii_lowercase();
        if w.len() >= 3 && !STOP_EN.contains(&w.as_str()) {
            english.push(w);
        }
    }

    let mut bigrams = Vec::new();
    let mut trigrams = Vec::new();
    let mut run = String::new();
    let flush = |run: &str, bigrams: &mut Vec<String>, trigrams: &mut Vec<String>| {
        let chars: Vec<char> = run.chars().collect();
        if chars.len() < 2 {
            return;
        }
        for w in chars.windows(2) {
            let s: String = w.iter().collect();
            if !STOP_CJK.contains(&s.as_str()) {
                bigrams.push(s);
            }
        }
        for w in chars.windows(3) {
            let s: String = w.iter().collect();
            trigrams.push(s);
        }
    };
    for c in query.chars() {
        if is_cjk(c) {
            run.push(c);
        } else if !run.is_empty() {
            flush(&run, &mut bigrams, &mut trigrams);
            run.clear();
        }
    }
    if !run.is_empty() {
        flush(&run, &mut bigrams, &mut trigrams);
    }

    QueryTokens {
        english,
        bigrams,
        trigrams,
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF)
}

fn result_on_topic(r: &SearchResult, tokens: &QueryTokens) -> bool {
    if tokens.english.is_empty() && tokens.bigrams.is_empty() && tokens.trigrams.is_empty() {
        return true;
    }
    let hay = format!("{} {} {}", r.title, r.snippet, r.url);
    let hay_l = hay.to_ascii_lowercase();

    let en = tokens
        .english
        .iter()
        .filter(|t| hay_l.contains(t.as_str()))
        .count();
    let tri = tokens
        .trigrams
        .iter()
        .filter(|t| hay.contains(t.as_str()))
        .count();
    let bi = tokens
        .bigrams
        .iter()
        .filter(|t| hay.contains(t.as_str()))
        .count();

    if en > 0 || tri > 0 {
        return true;
    }
    if bi >= 2 {
        return true;
    }
    // Short query: a single leftover 2-gram ("具身") is the whole topic.
    if tokens.trigrams.is_empty()
        && tokens.english.is_empty()
        && tokens.bigrams.len() <= 2
        && bi >= 1
    {
        return true;
    }
    if tokens.bigrams.is_empty()
        && tokens.trigrams.is_empty()
        && tokens.english.len() == 1
        && en >= 1
    {
        return true;
    }
    false
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

// ── Free backends & curl last-resort ────────────────────────────────────────

const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// Fetch URL body via reqwest; on hard failure callers may try [`curl_get`].
async fn http_get_text(url: &str) -> Result<String> {
    let resp = HTTP_CLIENT
        .get(url)
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("http get: {e}")))?;
    if !resp.status().is_success() {
        return Err(hermes_core::Error::ToolHost(format!(
            "HTTP {}",
            resp.status()
        )));
    }
    resp.text()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("body: {e}")))
}

/// Last-resort GET using system `curl` (often less blocked than embedded TLS).
async fn curl_get(url: &str) -> Result<String> {
    let out = tokio::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "25",
            "-A",
            BROWSER_UA,
            "-H",
            "Accept-Language: zh-CN,zh;q=0.9,en;q=0.8",
            url,
        ])
        .output()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("curl not available: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(hermes_core::Error::ToolHost(format!(
            "curl exit {:?}: {err}",
            out.status.code()
        )));
    }
    let body = String::from_utf8_lossy(&out.stdout).into_owned();
    if body.trim().is_empty() {
        return Err(hermes_core::Error::ToolHost("curl empty body".into()));
    }
    Ok(body)
}

/// SearXNG JSON API: `{base}/search?q=...&format=json`
async fn searxng_search(query: &str, limit: usize, base_url: &str) -> Result<String> {
    let base = base_url.trim().trim_end_matches('/');
    let url = format!(
        "{base}/search?q={}&format=json&categories=general",
        urlencoding::encode(query)
    );
    let body = http_get_text(&url)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("searxng: {e}")))?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| hermes_core::Error::ToolHost(format!("searxng json: {e}")))?;
    let results = v
        .get("results")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .take(limit)
                .filter_map(|r| {
                    let title = r.get("title")?.as_str()?.to_string();
                    let url = r.get("url")?.as_str()?.to_string();
                    let snippet = r
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(SearchResult {
                        title,
                        url,
                        snippet,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    format_usable(results, query)
        .map_err(|_| hermes_core::Error::ToolHost("searxng: no results".into()))
}

async fn duckduckgo_search(query: &str, limit: usize) -> Result<String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );
    let body = http_get_text(&url).await?;
    let results = parse_duckduckgo_html(&body, limit);
    format_usable(results, query)
        .map_err(|_| hermes_core::Error::ToolHost("duckduckgo: no results".into()))
}

async fn duckduckgo_search_curl(query: &str, limit: usize) -> Result<String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );
    let body = curl_get(&url).await?;
    let results = parse_duckduckgo_html(&body, limit);
    format_usable(results, query)
        .map_err(|_| hermes_core::Error::ToolHost("curl+duckduckgo: no results".into()))
}

async fn bing_search(query: &str, limit: usize) -> Result<String> {
    let url = format!(
        "https://www.bing.com/search?q={}",
        urlencoding::encode(query)
    );
    let body = http_get_text(&url).await?;
    let results = parse_bing_html(&body, limit);
    format_usable(results, query)
        .map_err(|_| hermes_core::Error::ToolHost("bing: no results".into()))
}

async fn bing_search_curl(query: &str, limit: usize) -> Result<String> {
    let url = format!(
        "https://www.bing.com/search?q={}",
        urlencoding::encode(query)
    );
    let body = curl_get(&url).await?;
    let results = parse_bing_html(&body, limit);
    format_usable(results, query)
        .map_err(|_| hermes_core::Error::ToolHost("curl+bing: no results".into()))
}

fn parse_bing_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut out = Vec::new();
    let mut rest = html;
    while out.len() < limit {
        let Some(idx) = rest.find("class=\"b_algo\"") else {
            break;
        };
        let slice = &rest[idx..];
        let href = slice
            .find("href=\"")
            .and_then(|i| {
                let s = &slice[i + 6..];
                s.find('"').map(|e| s[..e].to_string())
            })
            .unwrap_or_default();
        let title = slice
            .find("<h2")
            .and_then(|i| {
                let s = &slice[i..];
                s.find('>').and_then(|j| {
                    s[j + 1..]
                        .find("</h2>")
                        .map(|e| strip_tags(&s[j + 1..j + 1 + e]))
                })
            })
            .unwrap_or_default();
        let snippet = slice
            .find("class=\"b_caption\"")
            .or_else(|| slice.find("class=\"b_lineclamp"))
            .and_then(|i| {
                let s = &slice[i..];
                s.find('>').and_then(|j| {
                    s[j + 1..]
                        .find("</p>")
                        .map(|e| strip_tags(&s[j + 1..j + 1 + e]))
                })
            })
            .unwrap_or_default();
        if !title.is_empty() && href.starts_with("http") {
            out.push(SearchResult {
                title,
                url: href,
                snippet,
            });
        }
        rest = &rest[idx + 12..];
    }
    out
}

/// Minimal DDG HTML result parser (result__a + result__snippet).
fn parse_duckduckgo_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut out = Vec::new();
    // Very small parser: look for result__a href + text
    let mut rest = html;
    while out.len() < limit {
        let Some(a_idx) = rest.find("result__a") else {
            break;
        };
        let slice = &rest[a_idx..];
        let href = slice
            .find("href=\"")
            .and_then(|i| {
                let s = &slice[i + 6..];
                s.find('"').map(|e| s[..e].to_string())
            })
            .unwrap_or_default();
        let title = slice
            .find('>')
            .and_then(|i| {
                let s = &slice[i + 1..];
                s.find("</a>").map(|e| strip_tags(&s[..e]))
            })
            .unwrap_or_default();
        let snippet = slice
            .find("result__snippet")
            .and_then(|i| {
                let s = &slice[i..];
                s.find('>').and_then(|j| {
                    s[j + 1..]
                        .find("</")
                        .map(|e| strip_tags(&s[j + 1..j + 1 + e]))
                })
            })
            .unwrap_or_default();
        let url = decode_ddg_redirect(&href);
        if !title.is_empty() && !url.is_empty() {
            out.push(SearchResult {
                title,
                url,
                snippet,
            });
        }
        rest = &rest[a_idx + 10..];
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    html_entities(&out).trim().to_string()
}

fn html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn decode_ddg_redirect(href: &str) -> String {
    // //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com
    if let Some(idx) = href.find("uddg=") {
        let enc = &href[idx + 5..];
        let enc = enc.split('&').next().unwrap_or(enc);
        return urlencoding::decode(enc)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| enc.to_string());
    }
    if href.starts_with("http") {
        return href.to_string();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    href.to_string()
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
    let results: Vec<SearchResult> = keep_relevant(dedupe_by_domain(parsed), query)
        .into_iter()
        .take(limit)
        .collect();
    format_usable(results, query)
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
    let results = keep_relevant(results, query);
    if let Some(ans) = parsed.answer.filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("Answer: {ans}\n\nSources:\n"));
    }
    if results.is_empty() && out.is_empty() {
        return Err(hermes_core::Error::ToolHost(
            "tavily: no usable results".into(),
        ));
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
    format_usable(results, query)
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

    #[test]
    fn css_and_tracker_urls_are_not_usable() {
        let junk = vec![
            SearchResult {
                title: "style".into(),
                url: "https://r.bing.com/rp/abc.css".into(),
                snippet: "body{}".into(),
            },
            SearchResult {
                title: "script".into(),
                url: "https://cdn.example.com/app.js".into(),
                snippet: String::new(),
            },
            SearchResult {
                title: String::new(),
                url: "https://www.reuters.com/world/".into(),
                snippet: String::new(),
            },
        ];
        assert!(keep_usable(junk).is_empty());
        assert!(is_junk_url("https://statics.teams.cdn.office.net/x.css"));
        assert!(is_junk_url("https://www.bing.com/ck/a?!&&u=a1"));
        assert!(is_junk_url("https://hanyu.baidu.com/zici/s?wd=x"));
        assert!(!is_junk_url(
            "https://www.reuters.com/world/china-2026-08-14/"
        ));
    }

    #[test]
    fn usable_news_url_kept() {
        let kept = keep_usable(vec![SearchResult {
            title: "今日热点".into(),
            url: "https://www.thepaper.cn/newsDetail_forward_1".into(),
            snippet: "一条新闻".into(),
        }]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn dictionary_stub_is_off_topic() {
        let query = "具身智能最近有哪些投资事件";
        let junk = vec![
            SearchResult {
                title: "具 - 汉语词典".into(),
                url: "https://hanyu.baidu.com/zici/s?wd=%E5%85%B7".into(),
                snippet: "具：动词，具备。".into(),
            },
            SearchResult {
                title: "人形 - 释义".into(),
                url: "https://www.zdic.net/hans/%E4%BA%BA%E5%BD%A2".into(),
                snippet: "像人的形状。".into(),
            },
            SearchResult {
                title: "银河系".into(),
                url: "https://baike.baidu.com/item/%E9%93%B6%E6%B2%B3%E7%B3%BB".into(),
                snippet: "太阳系所在的星系。".into(),
            },
        ];
        assert!(keep_on_topic(junk, query).is_empty());
    }

    #[test]
    fn on_topic_news_kept() {
        let query = "具身智能最近有哪些投资事件";
        let kept = keep_on_topic(
            vec![SearchResult {
                title: "银河通用完成具身智能新一轮融资".into(),
                url: "https://www.36kr.com/p/embodied-ai".into(),
                snippet: "具身智能赛道本周再有投资事件。".into(),
            }],
            query,
        );
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn short_cjk_query_still_matches() {
        let kept = keep_on_topic(
            vec![SearchResult {
                title: "具身智能综述".into(),
                url: "https://example.com/embodied".into(),
                snippet: "综述。".into(),
            }],
            "具身",
        );
        assert_eq!(kept.len(), 1);
    }
}
