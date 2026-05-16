//! `web_search` — search the web via Bing HTML (no API key needed).

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

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
        description: "Search the web for current information. Returns titles, URLs, and snippets.".into(),
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
        "https://www.bing.com/search?q={}",
        urlencoding::encode(&a.query)
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
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

    let results = parse_bing_html(&body, a.limit);
    if results.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no results for: {}", a.query),
            is_error: false,
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

fn parse_bing_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for chunk in html.split("<li class=\"b_algo\">") {
        if results.len() >= limit {
            break;
        }
        // Extract title + URL from <h2><a href="URL">TITLE</a></h2>
        let title_url = extract_text_between(chunk, "<h2", "</h2>");
        let (title, url) = match title_url {
            Some(tu) => {
                let href = extract_href(&tu).unwrap_or_default();
                let title = strip_html_tags(
                    extract_text_between(&tu, ">", "</a>").as_deref().unwrap_or("untitled"),
                );
                (title, href)
            }
            None => continue,
        };
        if url.is_empty() || url.starts_with('#') || url.starts_with("javascript:") {
            continue;
        }
        // Extract snippet from <p class="b_lineclamp2">...</p> or <div class="b_caption"><p>...</p>
        let snippet = extract_snippet(chunk);
        results.push(SearchResult { title, url, snippet });
    }
    results
}

fn extract_snippet(chunk: &str) -> String {
    // Try <p class="b_lineclamp2"> first (most common)
    if let Some(s) = extract_text_between(chunk, "b_lineclamp2\"", "</p>") {
        return strip_html_tags(&s);
    }
    // Fallback: <div class="b_caption"><p>...</p>
    if let Some(cap) = extract_text_between(chunk, "b_caption", "</div>") {
        if let Some(s) = extract_text_between(&cap, "<p>", "</p>") {
            return strip_html_tags(&s);
        }
    }
    String::new()
}

fn extract_href(s: &str) -> Option<String> {
    let i = s.find("href=\"")? + 6;
    let rest = &s[i..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

fn extract_text_between(s: &str, start: &str, end: &str) -> Option<String> {
    let i = s.find(start)? + start.len();
    let rest = &s[i..];
    let j = rest.find(end)?;
    Some(rest[..j].to_string())
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
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bing_results() {
        let html = r#"<li class="b_algo"><h2><a href="https://rust-lang.org/">Rust</a></h2><div class="b_caption"><p class="b_lineclamp2">A systems programming language.</p></div></li><li class="b_algo"><h2><a href="https://example.com/">Example</a></h2></li>"#;
        let results = parse_bing_html(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(results[0].snippet, "A systems programming language.");
        assert_eq!(results[1].title, "Example");
        assert_eq!(results[1].url, "https://example.com/");
    }

    #[test]
    fn empty_html() {
        let results = parse_bing_html("no results here", 5);
        assert!(results.is_empty());
    }
}
