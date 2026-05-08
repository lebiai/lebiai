//! `web_search` — search the web via DuckDuckGo HTML (no API key needed).

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
        description: "Search the web for current information. Returns titles and URLs.".into(),
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
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(&a.query)
    );
    let body = reqwest::get(&url)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search fetch: {e}")))?
        .text()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_search body: {e}")))?;

    let results = parse_ddg_html(&body, a.limit);
    if results.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no results for: {}", a.query),
            is_error: false,
        });
    }
    let out = results
        .iter()
        .enumerate()
        .map(|(i, (title, href))| format!("{}. {} — {}", i + 1, title, href))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

fn parse_ddg_html(html: &str, limit: usize) -> Vec<(String, String)> {
    let mut results = Vec::new();
    for chunk in html.split("class=\"result__a\"") {
        if results.len() >= limit {
            break;
        }
        if let Some(href_start) = chunk.find("href=\"") {
            let rest = &chunk[href_start + 6..];
            if let Some(href_end) = rest.find('"') {
                let href = &rest[..href_end];
                let title = extract_text_between(rest, ">", "</a>")
                    .unwrap_or_else(|| "untitled".into());
                if !href.is_empty() && !href.starts_with('#') {
                    let decoded_href = href.replace("//duckduckgo.com/l/?uddg=", "");
                    let decoded_href = urlencoding::decode(&decoded_href)
                        .map(|s| s.into_owned())
                        .unwrap_or_else(|_| decoded_href.to_string());
                    let clean_href = decoded_href.split('&').next().unwrap_or(&decoded_href);
                    results.push((title.trim().to_string(), clean_href.to_string()));
                }
            }
        }
    }
    results
}

fn extract_text_between(s: &str, start: &str, end: &str) -> Option<String> {
    let i = s.find(start)? + start.len();
    let rest = &s[i..];
    let j = rest.find(end)?;
    let raw = &rest[..j];
    Some(strip_html_tags(raw))
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
    out
}
