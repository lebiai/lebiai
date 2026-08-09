//! Shared long-poll serve loop for the WeChat (iLink Bot) channel.
//!
//! One implementation serves every surface — CLI `wechat run`, GUI embedded
//! mode — so no surface forks a parallel poll loop: poll `getupdates`,
//! persist the cursor, retry transient failures, detect token expiry,
//! politely refuse non-text messages, and hand each inbound text message to
//! the surface-provided callback.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use tracing::warn;

use hermes_channel::Channel;

use crate::client::Client;
use crate::types::WeixinMessage;

/// Cursor checkpoint file (survives restarts so already-seen messages are
/// not reprocessed).
pub fn cursor_path() -> Result<std::path::PathBuf> {
    Ok(hermes_core::data_path("wechat-cursor.txt"))
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
            warn!(error = %e, path = %p.display(), "saving cursor failed");
        }
    }
}

/// Long-poll loop: fetch updates, persist the cursor, dispatch messages.
/// `shutdown` (set from Ctrl-C or the hosting surface) stops the loop
/// between polls. `on_message(inbound, text)` handles one text message;
/// non-text messages are politely refused by the loop itself.
pub async fn serve<F, Fut>(
    client: &Client,
    shutdown: Arc<AtomicBool>,
    mut on_message: F,
) -> Result<()>
where
    F: FnMut(WeixinMessage, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut cursor = read_cursor();
    while !shutdown.load(Ordering::SeqCst) {
        let resp = match client.get_updates(&cursor).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("ret=-14") || msg.contains("token expired") {
                    bail!("token 已失效，请重新运行 `hermes wechat login`");
                }
                warn!(error = %msg, "getupdates failed; retrying in 3s");
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
            let text = match inbound.first_text() {
                Some(t) => t.to_string(),
                None => {
                    if let Err(e) = client.send(&inbound, "目前只支持文本消息。").await {
                        warn!(error = %e, "send (non-text refusal) failed");
                    }
                    continue;
                }
            };
            // Best-effort typing indicator before the engine starts working.
            if let Err(e) = client
                .send_typing(&inbound.from_user_id, &inbound.context_token)
                .await
            {
                warn!(error = %e, "sendtyping failed (non-fatal)");
            }
            if let Err(e) = on_message(inbound, text).await {
                warn!(error = format!("{e:#}"), "handling inbound message failed");
            }
        }
    }
    Ok(())
}
