//! Per-turn execution: streaming, tool-call rendering, confirmation prompts,
//! and Ctrl-C cancellation.
//!
//! `run_one_turn` is the single entry; everything else is render formatting.

use anyhow::Result;
use hermes_core::{LlmProvider, Session, SessionEvent, ToolHost};
use hermes_store::SessionWriter;

use super::system_prompt::inject_time_header;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_one_turn(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[hermes_core::ToolSpec],
    model: &str,
    turn_system: &str,
    max_tokens: u32,
    _workspace: &std::path::Path,
    session: &mut Session,
    writer: &mut SessionWriter,
    permissions_cfg: &hermes_llm::PermissionsConfig,
    max_tool_rounds: usize,
) -> Result<()> {
    use hermes_turn::{ConfirmAction, PermissionChecker, TurnConfig, TurnEvent};
    use std::io::Write as _;

    let permissions = PermissionChecker::new(&permissions_cfg.allow, &permissions_cfg.deny);
    let config = TurnConfig {
        model: model.to_string(),
        system: if turn_system.is_empty() { None } else { Some(turn_system.to_string()) },
        max_tokens,
        max_tool_rounds,
        permissions,
    };

    let history = inject_time_header(session.messages.clone());
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Ctrl-C cancels the current turn (not the whole REPL). A fresh handler
    // is installed each turn and aborted on completion, so it can't leak.
    let signal_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\n(cancelling turn — Ctrl-C)");
            let _ = cancel_tx.send(());
        }
    });

    // Confirmation channel: the turn loop sends ConfirmRequest, a spawned
    // task reads from confirm_rx and prompts the user on stdin.
    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);

    // Spawn a task that reads confirmation requests and prompts the user.
    let confirm_task = tokio::spawn(async move {
        let mut always_allow: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut first_prompt = true;
        while let Some(req) = confirm_rx.recv().await {
            if always_allow.contains(&req.tool_name) {
                let _ = req.reply.send(ConfirmAction::Allow);
                continue;
            }
            if first_prompt {
                eprintln!(
                    "\x1b[2m  (y = yes, a = always allow this tool, N = deny, or type a reason to deny with feedback)\x1b[0m"
                );
                first_prompt = false;
            }
            eprint!(
                "\x1b[1m\x1b[33m  ⚠ confirm\x1b[0m {}: {}  \x1b[1m[y/a/N/...]\x1b[0m ",
                req.tool_name, req.summary,
            );
            std::io::stderr().flush().ok();
            let mut input = String::new();
            let action = if std::io::stdin().read_line(&mut input).is_ok() {
                match input.trim().to_ascii_lowercase().as_str() {
                    "y" => ConfirmAction::Allow,
                    "a" => {
                        always_allow.insert(req.tool_name.clone());
                        ConfirmAction::AlwaysAllow
                    }
                    "" | "n" => ConfirmAction::Deny { reason: None },
                    other => ConfirmAction::Deny {
                        reason: Some(other.to_string()),
                    },
                }
            } else {
                ConfirmAction::Deny { reason: None }
            };
            let _ = req.reply.send(action);
        }
    });

    let text_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_buf = std::sync::Mutex::new(String::new());
    use std::sync::atomic::Ordering::Relaxed;

    let on_event = |event: TurnEvent| {
        match event {
            TurnEvent::TextDelta(text) => {
                if thinking_started.load(Relaxed) {
                    eprint!("\r\x1b[K");
                    let mut buf = thinking_buf.lock().unwrap();
                    if !buf.is_empty() {
                        eprintln!("\x1b[90m  💭 ──────\x1b[0m");
                        for line in buf.lines() {
                            eprintln!("\x1b[90m  │ {line}\x1b[0m");
                        }
                    }
                    buf.clear();
                    thinking_started.store(false, Relaxed);
                }
                text_started.store(true, Relaxed);
                print!("{text}");
                std::io::stdout().flush().ok();
            }
            TurnEvent::ThinkingDelta(text) => {
                if !text_started.load(Relaxed) {
                    let mut buf = thinking_buf.lock().unwrap();
                    buf.push_str(&text);
                    let preview: String = buf.chars().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect();
                    let preview = preview.replace('\n', " ");
                    drop(buf);
                    eprint!("\r\x1b[K\x1b[90m  💭 {preview}\x1b[0m");
                    std::io::stderr().flush().ok();
                    thinking_started.store(true, Relaxed);
                }
            }
            TurnEvent::ToolUseStart { name, .. } => {
                if thinking_started.load(Relaxed) {
                    eprint!("\r\x1b[K");
                    let mut buf = thinking_buf.lock().unwrap();
                    if !buf.is_empty() {
                        eprintln!("\x1b[90m  💭 ──────\x1b[0m");
                        for line in buf.lines() {
                            eprintln!("\x1b[90m  │ {line}\x1b[0m");
                        }
                    }
                    buf.clear();
                    thinking_started.store(false, Relaxed);
                }
                eprint!("\x1b[33m  🔧 {name} …\x1b[0m");
                std::io::stderr().flush().ok();
            }
            TurnEvent::ToolExecStart { summary, .. } => {
                eprint!("\r\x1b[K");
                eprintln!("\x1b[33m  🔧 {summary}\x1b[0m");
            }
            TurnEvent::ToolUseResult { content, is_error, .. } => {
                if is_error {
                    eprintln!("\x1b[31m  ✗ {}\x1b[0m", content.lines().next().unwrap_or(""));
                }
            }
            TurnEvent::ToolConfirmPending { tool_name, summary, .. } => {
                // The spawned confirm_task handles the actual stdin prompt.
                // This event is for frontends that render their own UI.
                tracing::debug!(tool_name, summary, "tool confirmation pending");
            }
            TurnEvent::Usage { .. } => {}
            TurnEvent::Error(msg) => {
                eprintln!("\x1b[31m  error: {msg}\x1b[0m");
            }
            TurnEvent::Done => {}
        }
    };

    let result = hermes_turn::run_turn(
        provider, host, tools, &history, &config,
        Some(confirm_tx), on_event, cancel_rx,
    )
    .await;

    // Always release the Ctrl-C listener and confirm task before returning,
    // so a fresh pair is installed next turn.
    signal_task.abort();
    confirm_task.abort();

    let output = result.map_err(|e| anyhow::anyhow!("{e}"))?;

    // Apply new messages to session + persist
    for msg in &output.new_messages {
        session.messages.push(msg.clone());
        if let Err(e) = writer.append(&SessionEvent::Message(msg.clone())) {
            tracing::warn!(error=%e, "persist message");
        }
    }
    session.record_usage(output.usage);
    if let Err(e) = writer.append(&SessionEvent::Usage(output.usage)) {
        tracing::warn!(error=%e, "persist usage");
    }

    Ok(())
}

#[allow(dead_code)]
fn summarise_input(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() <= 80 {
        s
    } else {
        let truncated: String = s.chars().take(80).collect();
        format!("{truncated}…")
    }
}

#[allow(dead_code)]
fn friendly_tool_desc(name: &str) -> String {
    match name {
        "read" => "📖 Reading file...".into(),
        "write" => "📝 Writing file...".into(),
        "edit" => "✏️  Editing file...".into(),
        "bash" => "💻 Running command...".into(),
        "glob" => "🔍 Searching files...".into(),
        "grep" => "🔎 Searching content...".into(),
        "web_fetch" => "🌐 Fetching web page...".into(),
        "web_search" => "🔍 Searching the web...".into(),
        "todo_write" => "📋 Updating plan...".into(),
        "todo_list" => "📋 Listing tasks...".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}")
        }
    }
}

#[allow(dead_code)]
fn friendly_tool_result(name: &str, input: &serde_json::Value, workspace: &std::path::Path) -> String {
    let full_path = |rel: &str| -> String {
        if std::path::Path::new(rel).is_absolute() {
            rel.to_string()
        } else {
            workspace.join(rel).to_string_lossy().to_string()
        }
    };

    match name {
        "read" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let offset = input.get("offset").and_then(|o| o.as_u64());
            let limit = input.get("limit").and_then(|l| l.as_u64());
            let mut desc = format!("📖 read {}", full_path(path));
            if let Some(o) = offset {
                desc.push_str(&format!(" (from line {o}"));
                if let Some(l) = limit {
                    desc.push_str(&format!(", {} lines", l));
                }
                desc.push(')');
            }
            desc
        }
        "write" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let len = input
                .get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.len())
                .unwrap_or(0);
            format!("📝 write {} ({len} bytes)", full_path(path))
        }
        "edit" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let old: String = input
                .get("old_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            let new: String = input
                .get("new_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            format!("✏️  edit {}: \"{old}\" → \"{new}\"", full_path(path))
        }
        "bash" => {
            let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("?");
            let short: String = cmd.chars().take(120).collect();
            if cmd.chars().count() > 120 {
                format!("💻 $ {short}…")
            } else {
                format!("💻 $ {short}")
            }
        }
        "glob" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            format!("🔍 glob {}", full_path(pat))
        }
        "grep" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            let path = input.get("path").and_then(|p| p.as_str());
            match path {
                Some(p) => format!("🔎 grep /{pat}/ in {}", full_path(p)),
                None => format!("🔎 grep /{pat}/ in {}", workspace.display()),
            }
        }
        "web_search" => {
            let q = input.get("query").and_then(|q| q.as_str()).unwrap_or("?");
            format!("🌐 search \"{q}\"")
        }
        "web_fetch" => {
            let url = input.get("url").and_then(|u| u.as_str()).unwrap_or("?");
            format!("🌐 fetch {url}")
        }
        "think" => "💭 thinking…".into(),
        "todo_write" => {
            let items = input.get("items").and_then(|i| i.as_array());
            let n = items.map(|a| a.len()).unwrap_or(0);
            // Surface the task currently in progress, if any.
            let active = items
                .and_then(|a| {
                    a.iter().find(|it| {
                        it.get("status").and_then(|s| s.as_str()) == Some("in_progress")
                    })
                })
                .and_then(|it| it.get("content").and_then(|c| c.as_str()));
            match active {
                Some(c) => format!("📋 plan ({n}) → {c}"),
                None => format!("📋 plan ({n} tasks)"),
            }
        }
        "todo_list" => "📋 listing todos".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}({})", summarise_input(input))
        }
    }
}
