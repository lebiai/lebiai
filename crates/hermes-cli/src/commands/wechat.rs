//! `hermes wechat` — WeChat (iLink Bot) bridge.
//!
//! Two subcommands:
//!   - `login` — render a terminal QR; user scans it in WeChat; we persist
//!     the resulting `bot_token` to `~/.lebi-ai/wechat.toml`.
//!   - `run`   — long-poll the iLink Bot endpoint via the shared
//!     [`hermes_weixin::service::serve`] loop and reply to inbound text
//!     messages through the shared channel driver ([`handle_text_message`]).
//!     Each WeChat user gets their own session JSONL under
//!     `~/.lebi-ai/sessions/wechat/{user_id}/`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use hermes_channel::{handle_text_message, UserState};
use hermes_weixin::auth::{LoginSession, QrPollState, StoredCreds};
use hermes_weixin::client::{Client as WxClient, DEFAULT_BASE_URL};
use hermes_weixin::types::WeixinMessage;

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

    // ----- shared poll loop --------------------------------------------
    let users: Arc<tokio::sync::Mutex<HashMap<String, UserState>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    eprintln!("📡 监听中。在微信里给 bot 发消息即可对话。");

    let ctx_loop = ctx.clone();
    let wx_loop = wx.clone();
    let users_loop = users.clone();
    hermes_weixin::service::serve(
        &wx,
        shutdown,
        move |inbound: WeixinMessage, text: String| {
            let ctx = ctx_loop.clone();
            let wx = wx_loop.clone();
            let users = users_loop.clone();
            async move {
                let from = inbound.from_user_id.clone();
                eprintln!("📩 {from}: {text}");
                let mut guard = users.lock().await;
                handle_text_message(ctx.as_ref(), &wx, &mut guard, &from, text, inbound).await
            }
        },
    )
    .await?;

    eprintln!("✓ 已退出。cursor 已保存。");
    Ok(())
}
