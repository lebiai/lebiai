//! Per-turn execution: streaming, tool-call rendering, confirmation prompts,
//! and Ctrl-C cancellation.
//!
//! `run_one_turn` is the single entry; everything else is render formatting.

use anyhow::Result;
use hermes_core::{LlmProvider, Session, SessionEvent, ToolHost};
use hermes_store::SessionWriter;

use super::system_prompt::inject_time_header;
use crate::commands::{style, toolfmt};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_one_turn(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[hermes_core::ToolSpec],
    model: &str,
    turn_system: &str,
    max_tokens: u32,
    workspace: &std::path::Path,
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
                    "{}",
                    style::faint("  (y = yes, a = always allow this tool, N = deny, or type a reason to deny with feedback)")
                );
                first_prompt = false;
            }
            eprint!(
                "{} {}: {}  {} ",
                style::bold_yellow("  ⚠ confirm"),
                req.tool_name,
                req.summary,
                style::bold("[y/a/N/...]"),
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
                        eprintln!("{}", style::dim("  💭 ──────"));
                        for line in buf.lines() {
                            eprintln!("{}", style::dim(&format!("  │ {line}")));
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
                    eprint!("\r\x1b[K{}", style::dim(&format!("  💭 {preview}")));
                    std::io::stderr().flush().ok();
                    thinking_started.store(true, Relaxed);
                }
            }
            TurnEvent::ToolUseStart { name, .. } => {
                if thinking_started.load(Relaxed) {
                    eprint!("\r\x1b[K");
                    let mut buf = thinking_buf.lock().unwrap();
                    if !buf.is_empty() {
                        eprintln!("{}", style::dim("  💭 ──────"));
                        for line in buf.lines() {
                            eprintln!("{}", style::dim(&format!("  │ {line}")));
                        }
                    }
                    buf.clear();
                    thinking_started.store(false, Relaxed);
                }
                eprint!("{}", style::yellow(&format!("  {}", toolfmt::friendly_tool_desc(&name))));
                std::io::stderr().flush().ok();
            }
            TurnEvent::ToolExecStart { name, input, .. } => {
                eprint!("\r\x1b[K");
                eprintln!("{}", style::yellow(&format!("  {}", toolfmt::friendly_tool_result(&name, &input, workspace))));
            }
            TurnEvent::ToolUseResult { content, is_error, .. } => {
                if is_error {
                    eprintln!("{}", style::red(&format!("  ✗ {}", content.lines().next().unwrap_or(""))));
                }
            }
            TurnEvent::ToolConfirmPending { tool_name, summary, .. } => {
                // The spawned confirm_task handles the actual stdin prompt.
                // This event is for frontends that render their own UI.
                tracing::debug!(tool_name, summary, "tool confirmation pending");
            }
            TurnEvent::Usage { .. } => {}
            TurnEvent::Error(msg) => {
                eprintln!("{}", style::red(&format!("  error: {msg}")));
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
