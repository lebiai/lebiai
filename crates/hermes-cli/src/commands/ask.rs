//! `hermes ask` — one-shot prompt: send a single user message, print reply.
//!
//! Supports MCP tool use: if the LLM requests tool calls, they are executed
//! automatically (up to `cfg.limits.max_tool_rounds` rounds) before the final
//! text is printed.

use std::io::Write;

use anyhow::{Context, Result};
use hermes_core::Message;
use hermes_llm::Config;
use hermes_turn::{TurnConfig, TurnEvent};

use super::util::{build_active_provider, load_tool_host};

pub async fn run(prompt: String, system: Option<String>) -> Result<()> {
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let host = load_tool_host(&workspace_root, None, None, None, None).await?;
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?;

    let turn_config = TurnConfig {
        model: provider_cfg.model.clone(),
        system,
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: hermes_turn::PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
    };

    let history = vec![Message::user_text(prompt)];
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);

    // Spawn a task that auto-approves tool calls in one-shot mode.
    let confirm_task = tokio::spawn(async move {
        while let Some(req) = confirm_rx.recv().await {
            let _ = req.reply.send(hermes_turn::ConfirmAction::Allow);
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
            TurnEvent::Usage { .. } | TurnEvent::Done | TurnEvent::ToolConfirmPending { .. } => {}
            TurnEvent::Error(msg) => {
                eprintln!("\x1b[31m  error: {msg}\x1b[0m");
            }
        }
    };

    let output = hermes_turn::run_turn(
        provider.as_ref(),
        host.as_ref(),
        &tools,
        &history,
        &turn_config,
        Some(confirm_tx),
        on_event,
        cancel_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    confirm_task.abort();

    if !text_started.load(Relaxed) {
        // If no streaming text was printed (e.g. only tool calls), print the
        // final assistant text from the output messages.
        for msg in &output.new_messages {
            if matches!(msg.role, hermes_core::Role::Assistant) {
                for block in &msg.content {
                    if let hermes_core::ContentBlock::Text { text } = block {
                        println!("{text}");
                    }
                }
            }
        }
    }

    tracing::info!(
        input_tokens = output.usage.input_tokens,
        output_tokens = output.usage.output_tokens,
        "completion done"
    );
    Ok(())
}
