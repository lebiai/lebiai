//! HTTP client for the WeChat iLink Bot API.
//!
//! Auth note: `bot_token` is a long-lived bearer credential. It must NEVER
//! be logged. Request bodies may contain user message text; only summaries
//! go to tracing at INFO/DEBUG levels.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::types::*;

pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const AUTH_TYPE: &str = "ilink_bot_token";
/// Server holds long-poll connections for ~35s; allow a few extra seconds
/// of network jitter. Matches the value used by the official client.
const LONG_POLL_HTTP_TIMEOUT: Duration = Duration::from_secs(38);
/// Per-call default for non-long-poll endpoints (login, sendmessage…).
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// HTTP client. Construct with [`Client::new`] (anonymous, only for the QR
/// login flow) or [`Client::with_token`] (authenticated).
#[derive(Clone)]
pub struct Client {
    inner: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl Client {
    /// Anonymous client — only `get_bot_qrcode` and `get_qrcode_status` work
    /// without a token.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let inner = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            inner,
            base_url: base_url.into(),
            token: None,
        })
    }

    pub fn with_token(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self> {
        let mut c = Self::new(base_url)?;
        c.token = Some(token.into());
        Ok(c)
    }

    /// Per-request headers. `X-WECHAT-UIN` is regenerated on each call as
    /// the spec describes (anti-replay).
    fn headers(&self) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(
            HeaderName::from_static("authorizationtype"),
            HeaderValue::from_static(AUTH_TYPE),
        );
        h.insert(
            HeaderName::from_static("x-wechat-uin"),
            HeaderValue::from_str(&fresh_uin())?,
        );
        if let Some(token) = &self.token {
            let val = format!("Bearer {token}");
            let mut v = HeaderValue::from_str(&val).context("building Authorization header")?;
            v.set_sensitive(true);
            h.insert(reqwest::header::AUTHORIZATION, v);
        }
        Ok(h)
    }

    fn url(&self, path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        format!("{}/{trimmed}", self.base_url.trim_end_matches('/'))
    }

    async fn get_json<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let resp = self
            .inner
            .get(self.url(path))
            .headers(self.headers()?)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        decode_json(resp, path).await
    }

    /// POST a JSON body. `base_info` is injected automatically into the
    /// top-level object so callers don't have to track it.
    async fn post_json<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        timeout: Option<Duration>,
    ) -> Result<R> {
        let mut value = serde_json::to_value(body).context("serializing request body")?;
        inject_base_info(&mut value);
        let mut req = self
            .inner
            .post(self.url(path))
            .headers(self.headers()?)
            .json(&value);
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        let resp = req.send().await.with_context(|| format!("POST {path}"))?;
        decode_json(resp, path).await
    }

    // --- Public endpoints ----------------------------------------------

    pub async fn get_bot_qrcode(&self) -> Result<GetQrCodeResp> {
        self.get_json("/ilink/bot/get_bot_qrcode?bot_type=3").await
    }

    pub async fn get_qrcode_status(&self, qrcode: &str) -> Result<QrCodeStatusResp> {
        let path = format!("/ilink/bot/get_qrcode_status?qrcode={}", urlencode(qrcode));
        self.get_json(&path).await
    }

    /// Long-poll for new messages. The server holds the connection up to
    /// ~35s; we pass a slightly larger HTTP timeout to absorb network jitter.
    pub async fn get_updates(&self, cursor: &str) -> Result<GetUpdatesResp> {
        let body = GetUpdatesReq {
            get_updates_buf: cursor.to_string(),
        };
        let env: ApiEnvelope<GetUpdatesResp> = self
            .post_json("/ilink/bot/getupdates", &body, Some(LONG_POLL_HTTP_TIMEOUT))
            .await?;
        check_ret(env)
    }

    pub async fn send_message(&self, msg: WeixinMessage) -> Result<SendMessageResp> {
        let body = SendMessageReq { msg };
        let path = "/ilink/bot/sendmessage";
        // Retry once on transport-level failures (timeout / connect drop).
        // Application-level errors (HTTP 4xx, ret≠0, decode) propagate
        // immediately — those are not transient.
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            match self
                .post_json::<_, ApiEnvelope<SendMessageResp>>(path, &body, None)
                .await
            {
                Ok(env) => return check_ret(env),
                Err(e) if is_transient_transport_error(&e) => {
                    tracing::warn!(
                        attempt,
                        error = %format!("{e:#}"),
                        "sendmessage transient transport failure; retrying"
                    );
                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("sendmessage: exhausted retries")))
    }

    /// Send a typing indicator. Best-effort: errors are non-fatal for callers.
    pub async fn send_typing(&self, to_user_id: &str, context_token: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            to_user_id: &'a str,
            context_token: &'a str,
        }
        let env: ApiEnvelope<serde_json::Value> = self
            .post_json(
                "/ilink/bot/sendtyping",
                &Body {
                    to_user_id,
                    context_token,
                },
                None,
            )
            .await?;
        let _ = check_ret(env)?;
        Ok(())
    }
}

/// WeChat surface adapter for the shared channel driver
/// (`hermes_channel::Channel`): a reply is addressed by the inbound message
/// itself (its `context_token` / `client_id` must be echoed verbatim).
#[async_trait]
impl hermes_channel::Channel for Client {
    type Reply = WeixinMessage;

    fn name(&self) -> &str {
        "wechat"
    }

    async fn send(&self, reply: &WeixinMessage, text: &str) -> Result<()> {
        let out = WeixinMessage::reply_text(reply, text);
        self.send_message(out)
            .await
            .context("wechat send_message")?;
        Ok(())
    }
}

/// Insert `"base_info": {"channel_version": CHANNEL_VERSION}` at the top
/// level of an object-shaped request body. No-op for non-object bodies.
fn inject_base_info(v: &mut Value) {
    if let Value::Object(map) = v {
        let mut base_info = Map::new();
        base_info.insert(
            "channel_version".to_string(),
            Value::String(CHANNEL_VERSION.to_string()),
        );
        map.insert("base_info".to_string(), Value::Object(base_info));
    }
}

async fn decode_json<R: DeserializeOwned>(resp: reqwest::Response, path: &str) -> Result<R> {
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("reading response body for {path}"))?;
    if !status.is_success() {
        let snippet: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        return Err(anyhow!("{path} HTTP {status}: {snippet}"));
    }
    serde_json::from_slice(&bytes).with_context(|| {
        let snippet: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        format!("decoding response from {path}: {snippet}")
    })
}

fn check_ret<T>(env: ApiEnvelope<T>) -> Result<T> {
    if env.ret != 0 {
        return Err(anyhow!(
            "api error ret={} msg={}",
            env.ret,
            env.err_msg.unwrap_or_default()
        ));
    }
    Ok(env.data)
}

/// True if the error looks like a transport-level failure worth retrying.
/// Conservative: only matches reqwest's well-known transport markers, so
/// application-level errors (4xx, ret≠0, JSON decode) don't get retried.
fn is_transient_transport_error(e: &anyhow::Error) -> bool {
    // First try a clean downcast through the chain — `with_context` wraps
    // `reqwest::Error`, which preserves the source.
    for cause in e.chain() {
        if let Some(re) = cause.downcast_ref::<reqwest::Error>() {
            return re.is_timeout() || re.is_connect() || re.is_request();
        }
    }
    // Fallback: string-match the rendered chain. Covers cases where the
    // reqwest error has already been converted to a plain anyhow message.
    let s = format!("{e:#}").to_lowercase();
    s.contains("timed out")
        || s.contains("operation timed out")
        || s.contains("connection")
        || s.contains("error sending request")
}

/// `X-WECHAT-UIN`: base64(decimal-string(random u32)).
fn fresh_uin() -> String {
    let mut rng = rand::thread_rng();
    let n: u32 = rng.next_u32();
    B64.encode(n.to_string().as_bytes())
}

/// Minimal URL-encoder for query values we control (qrcode tokens are
/// ASCII alphanumerics + `-_`; this avoids pulling in `urlencoding`).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uin_is_valid_base64() {
        let uin = fresh_uin();
        let decoded = B64.decode(uin.as_bytes()).expect("valid base64");
        let s = std::str::from_utf8(&decoded).expect("decimal string");
        s.parse::<u32>().expect("decodes to u32 decimal");
    }

    #[test]
    fn url_join_drops_double_slash() {
        let c = Client::new("https://example.com/").unwrap();
        assert_eq!(c.url("/foo"), "https://example.com/foo");
        assert_eq!(c.url("foo"), "https://example.com/foo");
    }

    #[test]
    fn base_info_is_injected_into_object_bodies() {
        let mut v = serde_json::json!({"get_updates_buf": ""});
        inject_base_info(&mut v);
        assert_eq!(
            v["base_info"]["channel_version"],
            serde_json::Value::String(CHANNEL_VERSION.to_string())
        );
    }
}
