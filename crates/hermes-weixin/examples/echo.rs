//! WeChat echo bot — long-poll `getupdates` and echo text messages back.
//!
//! Run:
//!     cargo run -p hermes-weixin --example echo
//!
//! Prerequisites:
//!     cargo run -p hermes-weixin --example login   (writes ~/.lebi-ai/wechat.toml)
//!
//! Cursor checkpoint is persisted at
//!     ~/.lebi-ai/wechat-cursor.txt
//! so that restarts don't re-process already-seen messages.
//!
//! The `bot_token` is never printed.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use hermes_weixin::auth::StoredCreds;
use hermes_weixin::client::Client;
use hermes_weixin::types::WeixinMessage;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)?.ok_or_else(|| {
        anyhow!(
            "no creds at {} — run `cargo run -p hermes-weixin --example login` first",
            creds_path.display()
        )
    })?;

    let client = Client::with_token(&creds.base_url, &creds.bot_token)?;
    let cursor_path = creds_path
        .parent()
        .context("resolving cursor path")?
        .join("wechat-cursor.txt");
    let mut cursor = load_cursor(&cursor_path);

    println!("echo bot started (base={})", creds.base_url);
    println!("cursor file: {}", cursor_path.display());
    println!("Ctrl-C to stop.\n");

    loop {
        match client.get_updates(&cursor).await {
            Ok(resp) => {
                if !resp.msgs.is_empty() {
                    println!("got {} msg(s)", resp.msgs.len());
                }
                for msg in &resp.msgs {
                    if let Err(e) = handle_one(&client, msg).await {
                        eprintln!("  reply failed for {}: {e:#}", msg.from_user_id);
                    }
                }
                cursor = resp.get_updates_buf;
                if let Err(e) = std::fs::write(&cursor_path, &cursor) {
                    eprintln!("  cursor write failed: {e:#}");
                }
            }
            Err(e) => {
                eprintln!("getupdates failed: {e:#}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn handle_one(client: &Client, msg: &WeixinMessage) -> Result<()> {
    let Some(text) = msg.first_text() else {
        println!(
            "  ignore non-text from {} (type={})",
            msg.from_user_id, msg.message_type
        );
        return Ok(());
    };
    println!("  <- {}: {}", msg.from_user_id, text);

    let reply = WeixinMessage::reply_text(msg, format!("echo: {text}"));
    let resp = client.send_message(reply).await?;
    println!("  -> echoed (msg_id={:?})", resp.msg_id);
    Ok(())
}

fn load_cursor(path: &PathBuf) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => String::new(),
    }
}
