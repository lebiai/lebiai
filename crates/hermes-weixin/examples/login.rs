//! QR-login example.
//!
//! Run:
//!     cargo run -p hermes-weixin --example login
//!
//! Steps:
//!   1. Fetch a QR from `ilinkai.weixin.qq.com`.
//!   2. Render it directly in this terminal as a unicode QR. Open WeChat
//!      on your phone and scan it.
//!   3. On confirmation, persist `bot_token` to `~/.lebi-ai/wechat.toml`.
//!
//! The token value is NEVER printed to stdout/stderr.

use anyhow::{Context, Result};
use hermes_weixin::auth::{LoginSession, QrPollState, StoredCreds};
use hermes_weixin::client::DEFAULT_BASE_URL;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let base_url = DEFAULT_BASE_URL;
    let mut session = LoginSession::start(base_url)
        .await
        .context("starting QR login")?;

    println!("📱 用微信扫描下面的二维码：\n");
    println!("{}", session.render_terminal()?);
    println!("等待确认中… (Ctrl-C 中止)\n");

    let creds: StoredCreds = session
        .await_confirmation(3, |state| match state {
            QrPollState::Waiting => {}
            QrPollState::Scanned => eprintln!("  已扫码，请在手机上确认…"),
            QrPollState::Confirmed(_) => eprintln!("  已确认。"),
            QrPollState::Refreshed(qr) => {
                eprintln!("  二维码已过期，刷新如下：\n");
                println!("{qr}");
            }
        })
        .await?;

    let creds_path = StoredCreds::default_path()?;
    creds
        .save(&creds_path)
        .with_context(|| format!("saving creds to {}", creds_path.display()))?;
    if let Some(bot) = &creds.bot_id {
        println!(
            "✓ 登录成功 (bot_id={bot})。Token 已保存到 {}",
            creds_path.display()
        );
    } else {
        println!("✓ 登录成功。Token 已保存到 {}", creds_path.display());
    }
    Ok(())
}
