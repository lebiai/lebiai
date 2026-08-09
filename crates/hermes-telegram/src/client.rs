//! Telegram Bot API client: long-polling `getUpdates` + HTTP `sendMessage`.
//!
//! All methods POST to `https://api.telegram.org/bot{token}/{method}`.
//! `getUpdates` uses Telegram's server-side long-polling (`timeout=30`), so
//! the HTTP client timeout sits a few seconds above that to absorb jitter —
//! the same shape as `hermes-weixin`'s iLink long-poll.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

const API_BASE: &str = "https://api.telegram.org";
/// Server-side long-poll window for `getUpdates`, in seconds.
const LONG_POLL_TIMEOUT_SECS: u64 = 30;
/// HTTP client timeout: long-poll window + jitter headroom.
const HTTP_TIMEOUT_SECS: u64 = 35;

/// Telegram surface adapter for the shared channel driver
/// (`hermes_channel::Channel`): a reply is addressed by the `chat_id` (the
/// `Client` here is aliased `TgClient` by callers).
#[async_trait]
impl hermes_channel::Channel for Client {
    type Reply = i64;

    fn name(&self) -> &str {
        "telegram"
    }

    async fn send(&self, reply: &i64, text: &str) -> Result<()> {
        self.send_message(*reply, text)
            .await
            .context("telegram send_message")?;
        Ok(())
    }
}

// ---- response envelope ----------------------------------------------------

/// Telegram's standard `{ ok, result, description, error_code }` envelope.
/// `result` defaults to `T::default()` so a failed response (which omits
/// `result`) still deserializes — we bail before reading it in that case.
#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    error_code: Option<i32>,
    #[serde(default)]
    result: T,
}

// ---- wire types -----------------------------------------------------------

/// One update from `getUpdates`. Non-text events (stickers, edits, callbacks)
/// carry `message = None` or `message.text = None`; callers skip them.
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    #[allow(dead_code)]
    pub message_id: i64,
    pub chat: Chat,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
}

/// Bot identity from `getMe` — used to validate the token during `auth`.
#[derive(Debug, Default, Deserialize)]
pub struct BotUser {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub is_bot: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
}

// ---- client ---------------------------------------------------------------

#[derive(Clone)]
pub struct Client {
    base_url: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(bot_token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .context("building Telegram HTTP client")?;
        Ok(Self {
            base_url: format!("{API_BASE}/bot{bot_token}"),
            http,
        })
    }

    /// Validate the token by fetching the bot's identity.
    pub async fn get_me(&self) -> Result<BotUser> {
        let resp: TgResponse<BotUser> = self.call("getMe", &[]).await?;
        Ok(resp.result)
    }

    /// Long-poll for updates. Pass `offset = Some(last_update_id + 1)` to
    /// acknowledge prior updates; `None` returns only unconfirmed ones.
    pub async fn get_updates(&self, offset: Option<i64>) -> Result<Vec<Update>> {
        let mut form: Vec<(&str, String)> = vec![("timeout", LONG_POLL_TIMEOUT_SECS.to_string())];
        if let Some(off) = offset {
            form.push(("offset", off.to_string()));
        }
        let resp: TgResponse<Vec<Update>> = self.call("getUpdates", &form).await?;
        Ok(resp.result)
    }

    /// Send a text message to a chat.
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<()> {
        let form = vec![("chat_id", chat_id.to_string()), ("text", text.to_string())];
        let _: TgResponse<serde_json::Value> = self.call("sendMessage", &form).await?;
        Ok(())
    }

    async fn call<T>(&self, method: &str, form: &[(&str, String)]) -> Result<TgResponse<T>>
    where
        T: for<'de> Deserialize<'de> + Default,
    {
        let url = format!("{}/{}", self.base_url, method);
        let resp = self
            .http
            .post(&url)
            .form(form)
            .send()
            .await
            .with_context(|| format!("POST {method}"))?;
        let tg: TgResponse<T> = resp
            .json()
            .await
            .with_context(|| format!("decoding {method} response"))?;
        if !tg.ok {
            bail!(
                "Telegram {method} failed: {} (code {:?})",
                tg.description.unwrap_or_else(|| "unknown error".into()),
                tg.error_code,
            );
        }
        Ok(tg)
    }
}
