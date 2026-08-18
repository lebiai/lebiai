//! `hermes telegram` — Telegram Bot bridge.
//!
//! Two subcommands:
//!   - `auth` — validate the bot token (from @BotFather) and persist it to
//!     `~/.lebi-ai/telegram.toml`.
//!   - `run`  — long-poll the Telegram Bot API and reply to inbound text
//!     messages via the shared [`serve_inbound`] driver. Each Telegram chat
//!     gets its own session JSONL under
//!     `~/.lebi-ai/sessions/telegram/{chat_id}/`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use hermes_telegram::auth::StoredCreds;
use hermes_telegram::client::Client as TgClient;

use hermes_channel::{serve_inbound, Channel, UserState};

// ===== auth =================================================================

pub async fn auth() -> Result<()> {
    println!("🔑 Telegram Bot 配置");
    println!("请在 Telegram 找 @BotFather 创建 bot，获取 Bot Token。");
    println!();

    let token = prompt_secret("Bot Token")?;
    if token.is_empty() {
        anyhow::bail!("bot token 不能为空");
    }

    println!("正在验证凭证…");
    let client = TgClient::new(&token).context("building Telegram client")?;
    let me = client.get_me().await?;

    let creds = StoredCreds { bot_token: token };
    let path = StoredCreds::default_path()?;
    creds
        .save(&path)
        .with_context(|| format!("saving creds to {}", path.display()))?;

    let name = me.username.as_deref().unwrap_or("(unknown)");
    println!("✓ 凭证验证成功 (@{name}, id={})", me.id);
    println!("Token 已保存到 {}", path.display());
    println!("接着执行: hermes telegram run");
    Ok(())
}

/// Prompt for a secret value from stdin (no echo if possible).
fn prompt_secret(label: &str) -> Result<String> {
    eprint!("{label}: ");
    match rpassword::read_password() {
        Ok(s) => Ok(s.trim().to_string()),
        Err(_) => {
            // Fallback: just read a line (visible, but works in non-tty)
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .context("reading from stdin")?;
            Ok(buf.trim().to_string())
        }
    }
}

// ===== offset persistence ====================================================
//
// Mirror of the WeChat cursor pattern (`wechat-cursor.txt`): the getUpdates
// offset survives restarts so acknowledged updates are never re-processed.

/// Path where we persist the long-poll offset between `telegram run` invocations.
fn offset_path() -> Result<PathBuf> {
    Ok(hermes_core::data_path("telegram-offset.txt"))
}

fn read_offset() -> Option<i64> {
    offset_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<i64>().ok())
}

fn save_offset(offset: i64) {
    if let Ok(p) = offset_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&p, offset.to_string()) {
            tracing::warn!(error = %e, path = %p.display(), "saving offset failed");
        }
    }
}

// ===== run ==================================================================

pub async fn run() -> Result<()> {
    // ----- credentials & Telegram client -------------------------------
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)
        .with_context(|| format!("reading {}", creds_path.display()))?
        .ok_or_else(|| {
            anyhow!(
                "no Telegram credentials at {} — run `hermes telegram auth` first",
                creds_path.display()
            )
        })?;

    let tg = TgClient::new(&creds.bot_token).context("building Telegram client")?;
    let me = tg.get_me().await.context("fetching bot identity")?;
    let bot_name = me.username.as_deref().unwrap_or("(unknown)");
    eprintln!("✓ Telegram client ready (@{bot_name})");

    // ----- shared serve context (provider/host/tools/memory/skills) ----
    let ctx = super::chat::build_channel_ctx().await?;

    // ----- ctrl-c handling ---------------------------------------------
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let s = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\n收到 Ctrl-C，准备退出…");
                s.store(true, Ordering::SeqCst);
            }
        });
    }

    // ----- main loop: getUpdates long-poll -----------------------------
    // Offset is persisted after each acknowledged batch so a restart resumes
    // from the last update instead of re-delivering old ones.
    let mut offset = read_offset();
    let mut users: HashMap<i64, UserState> = HashMap::new();
    eprintln!("📡 监听中（offset={offset:?}）。在 Telegram 里给 bot 发消息即可对话。");

    while !shutdown.load(Ordering::SeqCst) {
        let updates = match tg.get_updates(offset).await {
            Ok(u) => u,
            Err(e) => {
                let msg = format!("{e:#}");
                // 401 / Unauthorized ⇒ the bot token is gone (revoked or
                // mistyped). Don't retry forever; surface it and exit.
                if msg.contains("Unauthorized") || msg.contains("401") {
                    bail!("token 已失效，请重新运行 `hermes telegram auth`");
                }
                tracing::warn!(error = %msg, "getUpdates failed; retrying in 3s");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };

        for upd in updates {
            // Acknowledge this update so Telegram won't redeliver it.
            offset = Some(upd.update_id + 1);

            let Some(msg) = upd.message else {
                continue;
            };
            let chat_id = msg.chat.id;
            let Some(text) = msg.text else {
                // Non-text (sticker/photo/voice/...). MVP: politely decline.
                if let Err(e) = tg.send(&chat_id, "目前只支持文本消息。").await {
                    tracing::warn!(error = %e, "send (non-text refusal) failed");
                }
                continue;
            };

            eprintln!("📩 {chat_id}: {text}");

            let key = chat_id.to_string();
            let state = match users.get_mut(&chat_id) {
                Some(s) => s,
                None => {
                    let s = UserState::new(tg.name(), &key, ctx.model(), ctx.provider_name())?;
                    users.insert(chat_id, s);
                    users.get_mut(&chat_id).unwrap()
                }
            };

            // Allowlist enforced inside serve_inbound (shared with all IM surfaces).
            if let Err(e) = serve_inbound(ctx.as_ref(), &tg, state, &key, &text, chat_id).await {
                tracing::warn!(error = format!("{e:#}"), "handling inbound message failed");
            }
        }

        if let Some(o) = offset {
            save_offset(o);
        }
    }

    if let Some(o) = offset {
        save_offset(o);
    }
    eprintln!("✓ 已退出。offset 已保存。");
    Ok(())
}
