//! `web_fetch` — fetch a URL, clean it to markdown, and (optionally) extract a
//! prompt-focused answer with a cheap LLM.
//!
//! Pipeline:
//! 1. Fetch the page (cached cleaned markdown reused within the TTL).
//! 2. Strip boilerplate blocks (`<script>`/`<style>`/`<head>`/`<nav>`/… and
//!    HTML comments) so JS/CSS/JSON noise never reaches the model.
//! 3. Convert the remaining HTML to markdown (structure preserved).
//! 4. If a `prompt` is supplied and an extraction provider is wired, ask a
//!    cheap model to answer the prompt against the page and return only that
//!    answer. Otherwise return the cleaned markdown, truncated to `max_chars`.
//!
//! Step 4 mirrors Claude Code's WebFetch: the tool does the extraction so the
//! main loop never has to read raw HTML.

use std::path::Path;

use hermes_core::{CompletionRequest, Message, Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::http_defaults::HTTP_CLIENT;
use crate::web::{ttl_or_default, WebToolsContext};
use crate::web_cache;

/// Upper bound on cleaned-markdown chars fed to the extraction model, to keep
/// the extraction request's token count bounded regardless of `max_chars`.
const EXTRACT_INPUT_CAP: usize = 48_000;

const EXTRACT_SYSTEM: &str =
    "You extract information from a web page to answer a specific question. \
Use ONLY the page content provided — do not add outside knowledge. \
Answer directly and concisely, quoting the relevant facts, figures, or quotes. \
If the page does not contain the answer, say so explicitly rather than guessing.";

#[derive(Deserialize)]
struct Args {
    url: String,
    #[serde(default = "default_max_chars")]
    max_chars: usize,
    /// Optional question. When provided (and an extraction model is wired), the
    /// tool returns a focused answer instead of the raw page markdown.
    #[serde(default)]
    prompt: Option<String>,
}

fn default_max_chars() -> usize {
    20_000
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_fetch".into(),
        description: "Fetch a web page and return its main content as clean markdown. \
            Pass a `prompt` to get a focused answer extracted from the page instead of the \
            full text — strongly preferred when you are looking for something specific, as \
            it is far more token-efficient. \
            Efficient research flow: web_search first, read the snippets, then fetch AT MOST \
            1–2 of the most promising URLs (with a `prompt`) — avoid fetching many pages. \
            Only fetch URLs from web_search results or that the user provided; do not guess URLs."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch (must come from search results or user input)"},
                "max_chars": {"type": "integer", "description": "Max characters of markdown to return when no prompt is given (default 20000)"},
                "prompt": {"type": "string", "description": "Optional question to answer from the page. When set, returns a concise extracted answer instead of the full page."}
            },
            "required": ["url"]
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
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: bad args: {e}")))?;

    let ttl = ttl_or_default(ctx);
    let cache_key = web_cache::fetch_key(&a.url);

    // Reuse cleaned markdown from a prior fetch of the same URL when fresh.
    let (markdown, from_cache) = match web_cache::get(&cache_key, ttl) {
        Some(md) => (md, true),
        None => {
            let resp = HTTP_CLIENT
                .get(&a.url)
                .send()
                .await
                .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: {e}")))?;

            let status = resp.status();
            if !status.is_success() {
                let hint = match status.as_u16() {
                    403 => "This site blocks automated access. Try a different source from your search results, or use the web_search snippet directly.",
                    404 => "Page not found. The URL may be wrong — use web_search to find the correct page instead of guessing URLs.",
                    429 => "Rate limited. Wait before retrying, or use the web_search snippet instead.",
                    _ => "Try a different URL from your search results.",
                };
                return Ok(ToolCallOutcome {
                    content: format!("HTTP {status} for {}\n{hint}", a.url),
                    is_error: true,
                });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch body: {e}")))?;

            let md = clean_html_to_markdown(&body);
            web_cache::put(cache_key, md.clone());
            (md, false)
        }
    };

    if markdown.chars().count() < 80 {
        return Ok(ToolCallOutcome {
            content: format!(
                "(Page returned very little text — likely a JS-rendered site. \
                 Use the web_search snippet instead of fetching this URL.)\n{markdown}"
            ),
            is_error: false,
        });
    }

    // Prompt-focused extraction path (Claude Code WebFetch style).
    if let (Some(prompt), Some(ctx)) = (a.prompt.as_deref().filter(|p| !p.trim().is_empty()), ctx) {
        let page: String = markdown.chars().take(EXTRACT_INPUT_CAP).collect();
        let user_msg = format!(
            "Web page content (markdown) from {}:\n\n{page}\n\n---\nQuestion: {prompt}\n\nAnswer using only the content above.",
            a.url
        );
        let req = CompletionRequest {
            model: ctx.extract_model.clone(),
            system: Some(EXTRACT_SYSTEM.to_string()),
            messages: vec![Message::user_text(user_msg)],
            tools: Vec::new(),
            max_tokens: ctx.extract_max_tokens,
            temperature: Some(0.1),
            enable_caching: false,
        };
        match ctx.extract_provider.complete(req).await {
            Ok(resp) => {
                let answer = resp.text();
                if !answer.trim().is_empty() {
                    let tag = if from_cache { " (page cached)" } else { "" };
                    return Ok(ToolCallOutcome {
                        content: format!("{answer}\n\n[extracted from {}{tag}]", a.url),
                        is_error: false,
                    });
                }
                // Empty extraction → fall through to returning the markdown.
            }
            Err(e) => {
                tracing::warn!(error=%e, "web_fetch extraction failed; returning raw markdown");
            }
        }
    }

    // Default path: return cleaned markdown, truncated to max_chars.
    let prefix = if from_cache { "(cached)\n" } else { "" };
    let content = if markdown.chars().count() > a.max_chars {
        let t: String = markdown.chars().take(a.max_chars).collect();
        format!("{prefix}{t}\n... (truncated at {} chars)", a.max_chars)
    } else {
        format!("{prefix}{markdown}")
    };

    Ok(ToolCallOutcome {
        content,
        is_error: false,
    })
}

/// Strip boilerplate blocks, then convert to markdown.
fn clean_html_to_markdown(html: &str) -> String {
    let mut h = remove_comments(html);
    for tag in [
        "script", "style", "head", "noscript", "svg", "template", "iframe", "form", "nav",
        "header", "footer", "aside",
    ] {
        h = remove_tag_blocks(&h, tag);
    }
    let md = match htmd::convert(&h) {
        Ok(md) => md,
        Err(_) => strip_tags_fallback(&h),
    };
    collapse_blank_lines(&md)
}

/// Lowercase ASCII letters only, preserving byte length so offsets computed on
/// the result map back onto the original string.
fn ascii_lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// Remove every `<tag …>…</tag>` block (case-insensitive). For void/unclosed
/// tags the rest of the document from the opening tag is dropped.
fn remove_tag_blocks(html: &str, tag: &str) -> String {
    let lower = ascii_lower(html);
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < html.len() {
        let Some(rel) = lower[i..].find(&open) else {
            out.push_str(&html[i..]);
            break;
        };
        let start = i + rel;
        // Confirm the match is the whole tag name, not a prefix (e.g. <nav> vs
        // <navigation>): the next char must be a tag-name boundary.
        let after = lower[start + open.len()..].chars().next();
        let is_boundary = match after {
            Some(c) => matches!(c, ' ' | '>' | '/' | '\t' | '\n' | '\r'),
            None => true,
        };
        if !is_boundary {
            out.push_str(&html[i..start + 1]);
            i = start + 1;
            continue;
        }
        out.push_str(&html[i..start]);
        match lower[start..].find(&close) {
            Some(crel) => i = start + crel + close.len(),
            None => break, // no closing tag — drop the remainder
        }
    }
    out
}

fn remove_comments(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of 3+ blank lines down to a single blank line and trim.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in s.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Fallback used only if htmd fails to parse: strip remaining tags to plain
/// text. Operates on already-boilerplate-stripped HTML, so it's clean text.
fn strip_tags_fallback(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut last_was_space = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            _ if c.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ => {
                out.push(c);
                last_was_space = false;
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_and_style_content() {
        let html = r#"<html><head><title>T</title><style>body{color:red;font-size:14px}</style></head>
            <body><h1>Real Heading</h1><script>var x=1;function f(){return 42;}window.__NEXT_DATA__={a:1}</script>
            <p>Actual article text here.</p></body></html>"#;
        let md = clean_html_to_markdown(html);
        assert!(md.contains("Real Heading"), "heading missing: {md}");
        assert!(md.contains("Actual article text"), "body missing: {md}");
        // None of the JS/CSS noise should survive.
        assert!(!md.contains("color:red"), "css leaked: {md}");
        assert!(!md.contains("font-size"), "css leaked: {md}");
        assert!(!md.contains("__NEXT_DATA__"), "js json leaked: {md}");
        assert!(!md.contains("function f"), "js leaked: {md}");
        assert!(!md.contains("var x"), "js leaked: {md}");
    }

    #[test]
    fn removes_comments_and_nav_footer() {
        let html = "<body><nav><a href=#>menu junk</a></nav><!-- tracking pixel -->\
            <main><p>Content paragraph.</p></main><footer>copyright junk</footer></body>";
        let md = clean_html_to_markdown(html);
        assert!(md.contains("Content paragraph"), "{md}");
        assert!(!md.contains("menu junk"), "nav leaked: {md}");
        assert!(!md.contains("copyright junk"), "footer leaked: {md}");
        assert!(!md.contains("tracking pixel"), "comment leaked: {md}");
    }

    #[test]
    fn tag_name_boundary_not_prefix() {
        // <navigation> must NOT be removed by the `nav` rule.
        let out = remove_tag_blocks("<navigation>keep me</navigation>", "nav");
        assert!(out.contains("keep me"), "over-matched prefix: {out}");
    }

    #[test]
    fn collapse_blank_lines_works() {
        assert_eq!(collapse_blank_lines("a\n\n\n\nb"), "a\n\nb");
    }
}
