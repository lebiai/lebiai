//! `hermes feishu` — Feishu (Lark) long-connection bridge.
//!
//! Two subcommands:
//!   - `auth` — validate app_id/app_secret and persist them to
//!     `~/.lebi-ai/feishu.toml`.
//!   - `run`  — establish the WS long-connection, receive events, reply to
//!     inbound text messages via the shared [`serve_inbound`] driver. Each
//!     Feishu user gets their own session JSONL under
//!     `~/.lebi-ai/sessions/feishu/{user_id}/`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use hermes_feishu::auth::StoredCreds;
use hermes_feishu::client::{EventPayload, FeishuClient, MessageReceiveEvent};

use hermes_channel::{serve_inbound, Channel, ServeCtx, UserState};

// ===== auth =================================================================

pub async fn auth() -> Result<()> {
    // Interactive: prompt for app_id and app_secret
    println!("🔑 飞书应用凭证配置");
    println!("请在飞书开放平台 (https://open.feishu.cn) 创建应用，获取 App ID 和 App Secret。");
    println!();

    let app_id = prompt_secret("App ID")?;
    let app_secret = prompt_secret("App Secret")?;

    if app_id.is_empty() || app_secret.is_empty() {
        anyhow::bail!("app_id 和 app_secret 不能为空");
    }

    // Validate by attempting to get a tenant_access_token
    println!("正在验证凭证…");
    let client = FeishuClient::new(&app_id, &app_secret);
    let token = client.get_tenant_token_for_validation().await?;

    let creds = StoredCreds {
        app_id,
        app_secret,
        domain: hermes_feishu::client::DEFAULT_DOMAIN.to_string(),
    };
    let path = StoredCreds::default_path()?;
    creds
        .save(&path)
        .with_context(|| format!("saving creds to {}", path.display()))?;

    println!(
        "✓ 凭证验证成功 (tenant_access_token 前8位: {}…)",
        &token[..8.min(token.len())]
    );
    println!("Token 已保存到 {}", path.display());
    println!("接着执行: hermes feishu run");
    Ok(())
}

/// Prompt for a secret value from stdin (no echo if possible).
fn prompt_secret(label: &str) -> Result<String> {
    eprint!("{label}: ");
    // Try rpassword for no-echo; fall back to plain stdin.
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

// ===== run ==================================================================

pub async fn run() -> Result<()> {
    // ----- credentials & Feishu client ---------------------------------
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)
        .with_context(|| format!("reading {}", creds_path.display()))?
        .ok_or_else(|| {
            anyhow!(
                "no Feishu credentials at {} — run `hermes feishu auth` first",
                creds_path.display()
            )
        })?;

    let feishu = FeishuClient::with_domain(&creds.app_id, &creds.app_secret, &creds.domain);
    eprintln!(
        "✓ Feishu client ready (app_id={}…)",
        &creds.app_id[..8.min(creds.app_id.len())]
    );

    // ----- shared serve context (provider/host/tools/memory/skills) ----
    let ctx = super::chat::build_channel_ctx().await?;

    // ----- ctrl-c handling ---------------------------------------------
    let shutdown = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    {
        let s = shutdown.clone();
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\n收到 Ctrl-C，准备退出…");
                s.store(true, Ordering::SeqCst);
                let _ = tx.send(()).await;
            }
        });
    }

    // ----- main loop: WS long-connection -------------------------------
    let users: Arc<tokio::sync::Mutex<HashMap<String, UserState>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let ctx_clone = ctx.clone();
    let users_clone = users.clone();
    let feishu_clone = feishu.clone();
    let shutdown_clone = shutdown.clone();

    let on_event = Box::new(move |payload: EventPayload| {
        let ctx = ctx_clone.clone();
        let users = users_clone.clone();
        let feishu = feishu_clone.clone();
        let shutdown = shutdown_clone.clone();

        tokio::spawn(async move {
            if shutdown.load(Ordering::SeqCst) {
                return;
            }
            if let Err(e) = handle_event(ctx.as_ref(), &feishu, &users, payload).await {
                tracing::warn!(error = format!("{e:#}"), "handling Feishu event failed");
            }
        });
    });

    eprintln!("📡 正在连接飞书长连接…");
    if let Err(e) = feishu.start(on_event, shutdown_rx).await {
        if !shutdown.load(Ordering::SeqCst) {
            return Err(e);
        }
    }

    eprintln!("✓ 已退出。");
    Ok(())
}

// ===== handle_event =========================================================

async fn handle_event(
    ctx: &ServeCtx,
    feishu: &FeishuClient,
    users: &Arc<tokio::sync::Mutex<HashMap<String, UserState>>>,
    payload: EventPayload,
) -> Result<()> {
    // Only handle im.message.receive_v1 events
    let header = payload.header.as_ref();
    let event_type = header.and_then(|h| h.event_type.as_deref()).unwrap_or("");
    if event_type != "im.message.receive_v1" {
        return Ok(());
    }

    let event_data = payload
        .event
        .as_ref()
        .ok_or_else(|| anyhow!("event body missing"))?;
    let msg_event: MessageReceiveEvent =
        serde_json::from_value(event_data.clone()).context("parsing im.message.receive_v1")?;

    let open_id = msg_event
        .sender
        .sender_id
        .open_id
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let chat_id = msg_event.message.chat_id.clone();
    let message_id = msg_event.message.message_id.clone();

    // Only handle text messages
    if msg_event.message.message_type != "text" {
        let _ = feishu.send(&open_id, "目前只支持文本消息。").await;
        return Ok(());
    }

    // Extract text content
    let text_content: serde_json::Value =
        serde_json::from_str(&msg_event.message.content).unwrap_or_default();
    let text = text_content
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        return Ok(());
    }

    eprintln!("📩 {open_id} (chat={chat_id}): {text}");

    // Look up / create per-user session
    let mut users_guard = users.lock().await;
    let state = match users_guard.get_mut(&open_id) {
        Some(s) => s,
        None => {
            let s = UserState::new(feishu.name(), &open_id, ctx.model(), ctx.provider_name())?;
            users_guard.insert(open_id.clone(), s);
            users_guard.get_mut(&open_id).unwrap()
        }
    };

    // Reply handle is the sender's open_id (Channel::Reply = String).
    let result = serve_inbound(ctx, feishu, state, &open_id, &text, open_id.clone()).await;

    let _ = (chat_id, message_id); // available for future "reply in thread" support
    result
}
