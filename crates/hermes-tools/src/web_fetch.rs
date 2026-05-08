//! `web_fetch` — fetch a URL and return its text content.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

#[derive(Deserialize)]
struct Args {
    url: String,
    #[serde(default = "default_max_chars")]
    max_chars: usize,
}

fn default_max_chars() -> usize {
    20_000
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_fetch".into(),
        description: "Fetch a web page and return its text content (HTML tags stripped).".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch"},
                "max_chars": {"type": "integer", "description": "Max characters to return (default 20000)"}
            },
            "required": ["url"]
        }),
    }
}

pub async fn run(_workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: bad args: {e}")))?;

    let resp = reqwest::Client::new()
        .get(&a.url)
        .header("User-Agent", "small-rust-hermes/0.1")
        .send()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: {e}")))?;

    if !resp.status().is_success() {
        return Ok(ToolCallOutcome {
            content: format!("HTTP {}", resp.status()),
            is_error: true,
        });
    }

    let body = resp
        .text()
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch body: {e}")))?;

    let text = html_to_text(&body);
    let truncated = if text.chars().count() > a.max_chars {
        let t: String = text.chars().take(a.max_chars).collect();
        format!("{t}\n... (truncated at {} chars)", a.max_chars)
    } else {
        text
    };

    Ok(ToolCallOutcome {
        content: truncated,
        is_error: false,
    })
}

fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut last_was_space = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if in_tag {
            continue;
        }
        if c.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}
