//! `hermes session ...` — list / show transcripts.

use std::path::Path;

use anyhow::{Context, Result};
use hermes_core::{ContentBlock, Role};

pub fn list(limit: usize) -> Result<()> {
    let dir = dirs::home_dir()
        .context("resolving $HOME")?
        .join(".small-rust-hermes")
        .join("sessions");
    let paths = hermes_store::list_sessions(&dir)
        .map_err(|e| anyhow::anyhow!("listing sessions: {e}"))?;
    if paths.is_empty() {
        println!("(no sessions in {})", dir.display());
        return Ok(());
    }
    for p in paths.iter().take(limit) {
        let summary = match hermes_store::read_session(p) {
            Ok(s) => format!(
                "{} turns, {} in / {} out tokens",
                s.messages.len(),
                s.total_input_tokens,
                s.total_output_tokens,
            ),
            Err(e) => format!("(unreadable: {e})"),
        };
        println!("{}  {summary}", p.display());
    }
    Ok(())
}

pub fn show(path: &Path) -> Result<()> {
    let session = hermes_store::read_session(path)
        .map_err(|e| anyhow::anyhow!("reading session: {e}"))?;
    println!("# {}", session.meta.id);
    println!("model:    {}", session.meta.model);
    println!("provider: {}", session.meta.provider);
    println!("created:  {}", session.meta.created_at);
    println!(
        "tokens:   in={}, out={}",
        session.total_input_tokens, session.total_output_tokens
    );
    println!("turns:    {}", session.messages.len());
    println!();
    for (i, m) in session.messages.iter().enumerate() {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        let preview = preview_message(m);
        println!("{:>3}. [{role:>9}] {preview}", i + 1);
    }
    Ok(())
}

fn preview_message(m: &hermes_core::Message) -> String {
    let mut parts: Vec<String> = Vec::new();
    for b in &m.content {
        match b {
            ContentBlock::Text { text } => parts.push(truncate(text, 100)),
            ContentBlock::Thinking { .. } => parts.push("(thinking)".into()),
            ContentBlock::ToolUse { name, .. } => parts.push(format!("[tool_use: {name}]")),
            ContentBlock::ToolResult { is_error, .. } => parts.push(if *is_error {
                "[tool_error]".into()
            } else {
                "[tool_result]".into()
            }),
            ContentBlock::Image { source } => parts.push(format!("[image: {}]", source.media_type)),
        }
    }
    parts.join(" · ")
}

fn truncate(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        if c == '\n' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
