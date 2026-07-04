//! `hermes wechat` — WeChat (iLink Bot) bridge.
//!
//! Two subcommands:
//!   - `login` — render a terminal QR; user scans it in WeChat; we persist
//!     the resulting `bot_token` to `~/.small-rust-hermes/wechat.toml`.
//!   - `run`   — long-poll the iLink Bot endpoint and reply to inbound text
//!     messages via the shared [`serve_inbound`] driver. Each WeChat user
//!     gets their own session JSONL under
//!     `~/.small-rust-hermes/sessions/wechat/{user_id}/`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use hermes_weixin::auth::{LoginSession, QrPollState, StoredCreds};
use hermes_weixin::client::{Client as WxClient, DEFAULT_BASE_URL};
use hermes_weixin::types::WeixinMessage;

use super::channel::{Channel, ServeCtx, UserState, serve_inbound};

// ===== login =================================================================

pub async fn login() -> Result<()> {
    let mut session = LoginSession::start(DEFAULT_BASE_URL)
        .await
        .context("starting QR login")?;

    println!("📱 用微信扫描下面的二维码：\n");
    println!("{}", session.render_terminal()?);
    println!("等待确认中… (Ctrl-C 中止)\n");

    let creds = session
        .await_confirmation(3, |state| match state {
            QrPollState::Waiting => {}
            QrPollState::Scanned => eprintln!("  已扫码，请在手机上确认…"),
            QrPollState::Confirmed(_) => eprintln!("  已确认。"),
            QrPollState::Refreshed(qr) => {
                eprintln!("  二维码已过期，已刷新：\n");
                println!("{qr}");
            }
        })
        .await
        .context("awaiting QR confirmation")?;

    let path = StoredCreds::default_path()?;
    creds
        .save(&path)
        .with_context(|| format!("saving creds to {}", path.display()))?;

    if let Some(bot) = &creds.bot_id {
        println!(
            "✓ 登录成功 (bot_id={bot})。Token 已保存到 {}",
            path.display()
        );
    } else {
        println!("✓ 登录成功。Token 已保存到 {}", path.display());
    }
    println!("接着执行: hermes wechat run");
    Ok(())
}

// ===== Channel impl ==========================================================

/// A WeChat reply needs to echo the inbound message's `context_token` /
/// `client_id`, so the [`Channel::Reply`] handle is the inbound message
/// itself (cheaply cloneable).
#[async_trait]
impl Channel for WxClient {
    type Reply = WeixinMessage;

    fn name(&self) -> &str {
        "wechat"
    }

    async fn send(&self, reply: &WeixinMessage, text: &str) -> Result<()> {
        let out = WeixinMessage::reply_text(reply, text);
        self.send_message(out).await.context("wechat send_message")?;
        Ok(())
    }
}

// ===== cursor persistence ====================================================

/// Path where we persist the long-poll cursor between `wechat run` invocations.
fn cursor_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    Ok(home.join(".small-rust-hermes").join("wechat-cursor.txt"))
}

fn read_cursor() -> String {
    cursor_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn save_cursor(buf: &str) {
    if let Ok(p) = cursor_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&p, buf) {
            tracing::warn!(error = %e, path = %p.display(), "saving cursor failed");
        }
    }
}

// ===== run ===================================================================

pub async fn run() -> Result<()> {
    // ----- credentials & WeChat client ---------------------------------
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)
        .with_context(|| format!("reading {}", creds_path.display()))?
        .ok_or_else(|| {
            anyhow!(
                "no WeChat credentials at {} — run `hermes wechat login` first",
                creds_path.display()
            )
        })?;
    let wx = WxClient::with_token(creds.base_url.clone(), creds.bot_token.clone())
        .context("building WeChat client")?;
    eprintln!(
        "✓ WeChat client ready{}",
        creds
            .bot_id
            .as_deref()
            .map(|b| format!(" (bot_id={b})"))
            .unwrap_or_default()
    );

    // ----- shared serve context (provider/host/tools/memory/skills) ----
    let ctx = ServeCtx::build().await?;

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

    // ----- main loop ---------------------------------------------------
    let mut cursor = read_cursor();
    let mut users: HashMap<String, UserState> = HashMap::new();
    eprintln!(
        "📡 监听中（cursor={} bytes）。在微信里给 bot 发消息即可对话。",
        cursor.len()
    );

    while !shutdown.load(Ordering::SeqCst) {
        let resp = match wx.get_updates(&cursor).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("ret=-14") || msg.contains("token expired") {
                    bail!("token 已失效，请重新运行 `hermes wechat login`");
                }
                tracing::warn!(error = %msg, "getupdates failed; retrying in 3s");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        cursor = resp.get_updates_buf.clone();
        save_cursor(&cursor);

        for inbound in resp.msgs {
            let from = inbound.from_user_id.clone();
            if from.is_empty() {
                continue;
            }

            let Some(text) = inbound.first_text() else {
                // Non-text payload (image/voice/file/video). MVP: politely decline.
                if let Err(e) = wx.send(&inbound, "目前只支持文本消息。").await {
                    tracing::warn!(error = %e, "send (non-text refusal) failed");
                }
                continue;
            };
            let text = text.to_string();
            eprintln!("📩 {from}: {text}");

            let state = match users.get_mut(&from) {
                Some(s) => s,
                None => {
                    let s = UserState::new(wx.name(), &from, ctx.model(), ctx.provider_name())?;
                    users.insert(from.clone(), s);
                    users.get_mut(&from).unwrap()
                }
            };

            if let Err(e) = serve_inbound(ctx.as_ref(), &wx, state, &from, &text, inbound).await {
                tracing::warn!(error = format!("{e:#}"), "handling inbound message failed");
            }
        }
    }

    eprintln!("✓ 已退出。cursor 已保存。");
    Ok(())
}
