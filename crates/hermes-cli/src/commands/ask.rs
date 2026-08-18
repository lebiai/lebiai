//! `hermes ask` — one-shot engine prompt (not the companion product).
//!
//! No session identity, no memory/skill injection, no reflection.
//! Tools are off unless `--tools`. Confirms fail-closed unless `--auto-allow`.

use std::io::Write;

use anyhow::Result;
use hermes_core::Message;
use hermes_turn::{TurnConfig, TurnEvent};

use super::util::{build_active_provider, build_web_ctx, load_tool_host};
use super::{style, toolfmt};

pub async fn run(
    prompt: String,
    system: Option<String>,
    enable_tools: bool,
    auto_allow: bool,
) -> Result<()> {
    let cfg = super::util::load_config_or_hint()?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let host = load_tool_host(
        &workspace_root,
        None,
        None,
        None,
        None,
        Some(build_web_ctx(&cfg, provider.clone())),
    )
    .await?;
    let tools = if enable_tools {
        host.list_tools()
            .await
            .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?
    } else {
        Vec::new()
    };

    let turn_config = TurnConfig {
        model: provider_cfg.model.clone(),
        system,
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: hermes_turn::PermissionChecker::new(
            &cfg.permissions.allow,
            &cfg.permissions.deny,
        ),
    };

    let history = vec![Message::user_text(prompt)];
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let confirm_tx = if auto_allow {
        let (tx, mut confirm_rx) =
            tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);
        tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                let _ = req.reply.send(hermes_turn::ConfirmAction::Allow);
            }
        });
        Some(tx)
    } else {
        None
    };

    let text_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_buf = std::sync::Mutex::new(String::new());
    use std::sync::atomic::Ordering::Relaxed;

    let on_event = |event: TurnEvent| match event {
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
                let preview: String = buf
                    .chars()
                    .rev()
                    .take(60)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
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
            eprint!(
                "{}",
                style::yellow(&format!("  {}", toolfmt::friendly_tool_desc(&name)))
            );
            std::io::stderr().flush().ok();
        }
        TurnEvent::ToolExecStart { name, input, .. } => {
            eprint!("\r\x1b[K");
            eprintln!(
                "{}",
                style::yellow(&format!(
                    "  {}",
                    toolfmt::friendly_tool_result(&name, &input, &workspace_root)
                ))
            );
        }
        TurnEvent::ToolUseResult {
            content, is_error, ..
        } => {
            if is_error {
                eprintln!(
                    "{}",
                    style::red(&format!("  ✗ {}", content.lines().next().unwrap_or("")))
                );
            }
        }
        TurnEvent::Usage { .. }
        | TurnEvent::Done
        | TurnEvent::Cancelled
        | TurnEvent::ToolConfirmPending { .. } => {}
        TurnEvent::Error(msg) => {
            eprintln!("{}", style::red(&format!("  error: {msg}")));
        }
    };

    let output = hermes_turn::run_turn(
        provider.as_ref(),
        host.as_ref(),
        &tools,
        &history,
        &turn_config,
        confirm_tx,
        on_event,
        cancel_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

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
