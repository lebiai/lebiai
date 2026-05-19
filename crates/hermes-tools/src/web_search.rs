//! `web_search` — search the web via Brave Search HTML (no API key needed).

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::http_defaults::HTTP_CLIENT;

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
        description: "Search the web for current information. Returns titles, URLs, and snippets. \
            Always search first before fetching — the snippet alone is often enough. \
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
    }
}

pub async fn run(_workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search: bad args: {e}")))?;

    let url = format!(
        "https://search.brave.com/search?q={}",
        urlencoding::encode(&a.query)
    );
    let resp = HTTP_CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search fetch: {e}")))?;

    if !resp.status().is_success() {
        return Ok(ToolCallOutcome {
            content: format!("web_search: HTTP {}", resp.status()),
            is_error: true,
        });
    }

    let body = resp
        .text()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search body: {e}")))?;

    let results = parse_brave_html(&body, a.limit);
    if results.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!(
                "No results for: \"{}\". Try rephrasing the query or using different keywords.",
                a.query
            ),
            is_error: true,
        });
    }
    let out = results
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
        .join("\n");
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

/// Parse Brave Search HTML results.
///
/// Brave wraps each organic result in a block marked with `data-type="web"`.
/// Within each block:
/// - URL: first `<a href="https://...">` element
/// - Title: `<div class="title ...">...</div>`
/// - Snippet: `<div class="generic-snippet ...">...</div>`
fn parse_brave_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    // Split on data-type="web" to isolate each result block
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

/// Extract the first `href="https://..."` from a chunk.
fn extract_first_https_href(chunk: &str) -> Option<String> {
    let marker = "href=\"https://";
    let pos = chunk.find(marker)?;
    let start = pos + 6; // skip `href="`
    let rest = &chunk[start..];
    let end = rest.find('"')?;
    let raw = &rest[..end];
    Some(decode_html_entities(raw))
}

/// Extract text content from a `<div class="PREFIX ...">...</div>`.
fn extract_div_text(chunk: &str, class_prefix: &str) -> Option<String> {
    let pat = format!("class=\"{class_prefix}");
    let pos = chunk.find(&pat)?;
    let after = &chunk[pos..];
    // Find closing > of the opening tag
    let gt = after.find('>')?;
    let rest = &after[gt + 1..];
    // Find closing </div>
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
    // Strip real HTML tags first, then decode entities (so &lt; doesn't get
    // misinterpreted as an HTML tag).
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
        assert_eq!(
            results[0].snippet,
            "Latest technology news from Reuters."
        );
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
}
