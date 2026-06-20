//! Feishu long-connection (WebSocket) client + HTTP messaging API.
//!
//! The WS protocol follows the Go SDK (`oapi-sdk-go/ws/client.go`):
//!   1. POST `/callback/ws/endpoint` with `{AppID, AppSecret}` → get WS URL
//!   2. Connect to that URL via WebSocket
//!   3. Receive binary frames (protobuf `Frame`), dispatch control/data
//!   4. Send ping frames on a timer, auto-reconnect on disconnect
//!
//! The HTTP API handles:
//!   - `tenant_access_token` acquisition + refresh
//!   - Sending text replies via `/im/v1/messages`

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::Message,
};

use crate::frame::{self, Frame};

// ---- constants ------------------------------------------------------------

pub const DEFAULT_DOMAIN: &str = "https://open.feishu.cn";
const ENDPOINT_PATH: &str = "/callback/ws/endpoint";
const TOKEN_PATH: &str = "/open-apis/auth/v3/tenant_access_token/internal";
const SEND_MESSAGE_PATH: &str = "/open-apis/im/v1/messages";

// ---- bootstrap types ------------------------------------------------------

#[derive(Debug, Serialize)]
struct BootstrapRequest {
    #[serde(rename = "AppID")]
    app_id: String,
    #[serde(rename = "AppSecret")]
    app_secret: String,
}

#[derive(Debug, Deserialize)]
struct EndpointResp {
    code: i32,
    msg: String,
    data: Option<EndpointData>,
}

#[derive(Debug, Deserialize)]
struct EndpointData {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: Option<ClientConfig>,
}

#[derive(Debug, Deserialize, Clone)]
struct ClientConfig {
    #[serde(rename = "ReconnectCount")]
    #[allow(dead_code)]
    reconnect_count: Option<i32>,
    #[serde(rename = "ReconnectInterval")]
    reconnect_interval: Option<i32>,
    #[serde(rename = "ReconnectNonce")]
    reconnect_nonce: Option<i32>,
    #[serde(rename = "PingInterval")]
    ping_interval: Option<i32>,
}

// ---- tenant_access_token types -------------------------------------------

#[derive(Debug, Serialize)]
struct TokenRequest {
    app_id: String,
    app_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResp {
    code: i32,
    msg: String,
    tenant_access_token: String,
    expire: i32,
}

// ---- send message types ---------------------------------------------------

#[derive(Debug, Serialize)]
struct SendMessageReq {
    receive_id: String,
    msg_type: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageResp {
    code: i32,
    msg: String,
}

// ---- event payload (simplified) -------------------------------------------

/// The top-level event wrapper that Feishu pushes over the WS data frame.
#[derive(Debug, Deserialize)]
pub struct EventPayload {
    pub header: Option<EventHeader>,
    pub event: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct EventHeader {
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub token: Option<String>,
}

/// The `im.message.receive_v1` event body.
#[derive(Debug, Deserialize)]
pub struct MessageReceiveEvent {
    pub message: MessageBody,
    pub sender: Sender,
}

#[derive(Debug, Deserialize)]
pub struct MessageBody {
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String,
    pub message_type: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct Sender {
    pub sender_id: SenderId,
}

#[derive(Debug, Deserialize)]
pub struct SenderId {
    pub open_id: Option<String>,
    pub user_id: Option<String>,
    pub union_id: Option<String>,
}

// ---- WS client ------------------------------------------------------------

/// Callback for inbound Feishu events.
pub type EventHandler = Box<dyn Fn(EventPayload) + Send + Sync>;

/// Feishu long-connection client.
///
/// Usage:
/// ```ignore
/// let client = FeishuClient::new(app_id, app_secret);
/// client.start(event_handler).await?;
/// ```
pub struct FeishuClient {
    app_id: String,
    app_secret: String,
    domain: String,
    http: reqwest::Client,
    /// Cached tenant_access_token + expiry epoch-seconds.
    token_state: Arc<RwLock<TokenState>>,
    /// Service ID extracted from the WS URL query params.
    service_id: Arc<Mutex<String>>,
    /// Connection ID extracted from the WS URL query params.
    conn_id: Arc<Mutex<String>>,
    /// Server-pushed client config (ping interval, reconnect params).
    client_config: Arc<RwLock<ClientConfig>>,
    /// Whether auto-reconnect is enabled.
    auto_reconnect: bool,
}

impl Clone for FeishuClient {
    fn clone(&self) -> Self {
        Self {
            app_id: self.app_id.clone(),
            app_secret: self.app_secret.clone(),
            domain: self.domain.clone(),
            http: self.http.clone(),
            token_state: self.token_state.clone(),
            service_id: self.service_id.clone(),
            conn_id: self.conn_id.clone(),
            client_config: self.client_config.clone(),
            auto_reconnect: self.auto_reconnect,
        }
    }
}

struct TokenState {
    token: Option<String>,
    expires_at: i64, // epoch seconds
}

impl FeishuClient {
    pub fn new(app_id: impl Into<String>, app_secret: impl Into<String>) -> Self {
        Self::with_domain(app_id, app_secret, DEFAULT_DOMAIN)
    }

    pub fn with_domain(
        app_id: impl Into<String>,
        app_secret: impl Into<String>,
        domain: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("building reqwest client");
        Self {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            domain: domain.into(),
            http,
            token_state: Arc::new(RwLock::new(TokenState {
                token: None,
                expires_at: 0,
            })),
            service_id: Arc::new(Mutex::new(String::new())),
            conn_id: Arc::new(Mutex::new(String::new())),
            client_config: Arc::new(RwLock::new(ClientConfig {
                reconnect_count: None,
                reconnect_interval: None,
                reconnect_nonce: None,
                ping_interval: None,
            })),
            auto_reconnect: true,
        }
    }

    // ---- public API -------------------------------------------------------

    /// Validate credentials by obtaining a tenant_access_token.
    /// Returns the token string on success.
    pub async fn get_tenant_token_for_validation(&self) -> Result<String> {
        self.get_tenant_token().await
    }

    /// Start the long-connection loop. Blocks until a fatal error or the
    /// shutdown signal fires.
    pub async fn start(&self, on_event: EventHandler, mut shutdown: mpsc::Receiver<()>) -> Result<()> {
        loop {
            match self.connect_and_run(&on_event, &mut shutdown).await {
                Ok(()) => {
                    // clean shutdown
                    return Ok(());
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg.contains("514") || msg.contains("AuthFailed") {
                        bail!("飞书鉴权失败 (app_id/app_secret 可能不正确): {msg}");
                    }
                    if !self.auto_reconnect {
                        bail!("连接断开且未启用自动重连: {msg}");
                    }
                    tracing::warn!(error = %msg, "WS disconnected; reconnecting…");
                    let config = self.client_config.read().await;
                    let nonce = config.reconnect_nonce.unwrap_or(30);
                    let interval = config.reconnect_interval.unwrap_or(120);
                    // First reconnect: random jitter up to `nonce` seconds
                    let jitter = (nonce as u64 * 1000)
                        * (uuid::Uuid::new_v4().as_u128() as u64 % 1000) as u64
                        / 1000;
                    tokio::time::sleep(Duration::from_millis(jitter)).await;
                    // Subsequent: fixed interval
                    let _ = interval; // we just retry immediately after jitter for simplicity
                }
            }
        }
    }

    /// Send a text reply to a Feishu chat.
    pub async fn send_text(&self, receive_id: &str, text: &str) -> Result<()> {
        let token = self.get_tenant_token().await?;
        let content = serde_json::to_string(&serde_json::json!({ "text": text }))
            .context("serializing message content")?;

        let url = format!(
            "{}{}?receive_id_type=open_id",
            self.domain.trim_end_matches('/'),
            SEND_MESSAGE_PATH
        );

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| anyhow!("invalid token header"))?,
        );

        let body = SendMessageReq {
            receive_id: receive_id.to_string(),
            msg_type: "text".to_string(),
            content,
        };

        // Retry once on transient transport errors (mirrors weixin pattern).
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            match self
                .http
                .post(&url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    let bytes = resp.bytes().await.context("reading send-message response")?;
                    if !status.is_success() {
                        let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
                        last_err = Some(anyhow!("send-message HTTP {status}: {snippet}"));
                        continue;
                    }
                    let sr: SendMessageResp = serde_json::from_slice(&bytes)
                        .with_context(|| {
                            let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
                            format!("decoding send-message response: {snippet}")
                        })?;
                    if sr.code != 0 {
                        last_err = Some(anyhow!("send-message api error code={} msg={}", sr.code, sr.msg));
                        // Token expired? Force refresh on next attempt.
                        if sr.code == 99991663 || sr.code == 99991664 {
                            let mut ts = self.token_state.write().await;
                            ts.token = None;
                        }
                        continue;
                    }
                    return Ok(());
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        tracing::warn!(attempt, error = %e, "send-message transient failure; retrying");
                        last_err = Some(e.into());
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("send-message: exhausted retries")))
    }

    // ---- internal: WS connection ------------------------------------------

    async fn connect_and_run(
        &self,
        on_event: &EventHandler,
        shutdown: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        // 1. Bootstrap: get WS URL
        let ws_url = self.get_ws_url().await?;
        tracing::info!(url = %ws_url, "bootstrapped WS endpoint");

        // Extract service_id and device_id from URL query params
        let url_parsed = url::Url::parse(&ws_url).context("parsing WS URL")?;
        let service_id_val = url_parsed
            .query_pairs()
            .find(|(k, _)| k == "service_id")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();
        let device_id_val = url_parsed
            .query_pairs()
            .find(|(k, _)| k == "device_id")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();
        {
            let mut sid = self.service_id.lock().await;
            *sid = service_id_val.clone();
            let mut cid = self.conn_id.lock().await;
            *cid = device_id_val;
        }

        // 2. Connect WebSocket
        // Ensure rustls crypto provider is installed (required since rustls 0.23)
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .context("connecting to Feishu WS")?;
        tracing::info!("connected to Feishu WS");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // 3. Spawn ping loop
        let ping_interval = {
            let config = self.client_config.read().await;
            Duration::from_secs(config.ping_interval.unwrap_or(120) as u64)
        };
        let service_id_for_ping: i32 = service_id_val.parse().unwrap_or(0);
        let ping_shutdown = Arc::new(AtomicI64::new(0));
        let ping_handle = {
            let ps = ping_shutdown.clone();
            let write = write.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(ping_interval).await;
                    if ps.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    let frame = Frame::new_ping(service_id_for_ping);
                    let bytes = frame.encode();
                    let mut w = write.lock().await;
                    if let Err(e) = w.send(Message::Binary(bytes.into())).await {
                        tracing::warn!(error = %e, "ping send failed");
                        break;
                    }
                }
            })
        };

        // 4. Receive loop
        let result = loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            if let Err(e) = self.handle_frame(&data, on_event, &write).await {
                                tracing::warn!(error = %e, "handling WS frame failed");
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("WS close frame received");
                            break Err(anyhow!("WS closed by server"));
                        }
                        Some(Ok(Message::Ping(_))) => {
                            // tungstenite auto-responds to pings
                        }
                        Some(Ok(other)) => {
                            tracing::debug!(?other, "ignoring non-binary WS message");
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = %e, "WS read error");
                            break Err(anyhow!("WS read error: {e}"));
                        }
                        None => {
                            break Err(anyhow!("WS stream ended"));
                        }
                    }
                }
                _ = shutdown.recv() => {
                    tracing::info!("shutdown signal received");
                    break Ok(());
                }
            }
        };

        // Cleanup
        ping_shutdown.store(1, Ordering::SeqCst);
        let _ = ping_handle.await;
        {
            let mut w = write.lock().await;
            let _ = w.close().await;
        }

        result
    }

    async fn handle_frame(
        &self,
        data: &[u8],
        on_event: &EventHandler,
        write: &Arc<Mutex<futures_util::stream::SplitSink<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            Message,
        >>>,
    ) -> Result<()> {
        let frame = Frame::decode(data).context("decoding WS frame")?;

        match frame.method {
            frame::method::CONTROL => {
                // Pong or other control
                let msg_type = frame.header(frame::header_key::TYPE).unwrap_or("");
                if msg_type == frame::message_type::PONG {
                    tracing::debug!("received pong");
                    // Server may push updated ClientConfig in pong payload
                    if !frame.payload.is_empty() {
                        if let Ok(conf) = serde_json::from_slice::<ClientConfig>(&frame.payload) {
                            let mut current = self.client_config.write().await;
                            *current = conf;
                        }
                    }
                }
            }
            frame::method::DATA => {
                self.handle_data_frame(&frame, on_event, write).await?;
            }
            _ => {
                tracing::debug!(method = frame.method, "unknown frame method");
            }
        }
        Ok(())
    }

    async fn handle_data_frame(
        &self,
        frame: &Frame,
        on_event: &EventHandler,
        write: &Arc<Mutex<futures_util::stream::SplitSink<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            Message,
        >>>,
    ) -> Result<()> {
        let msg_type = frame.header(frame::header_key::TYPE).unwrap_or("");
        let sum: i32 = frame
            .header(frame::header_key::SUM)
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let seq: i32 = frame
            .header(frame::header_key::SEQ)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        // For MVP we don't implement multi-frame reassembly (sum > 1).
        // Feishu event payloads are typically small enough to fit in one frame.
        if sum > 1 {
            tracing::warn!(sum, seq, "multi-frame messages not yet supported; dropping");
            // Still ACK
            self.send_ack(frame, 200, write).await?;
            return Ok(());
        }

        let payload_str = String::from_utf8_lossy(&frame.payload);

        match msg_type {
            frame::message_type::EVENT => {
                // Parse the event payload
                let event: EventPayload = serde_json::from_str(&payload_str)
                    .with_context(|| format!("parsing event payload: {}", &payload_str[..payload_str.len().min(200)]))?;

                // Dispatch to handler
                on_event(event);

                // ACK success
                self.send_ack(frame, 200, write).await?;
            }
            frame::message_type::CARD => {
                // Card callbacks not handled in MVP
                self.send_ack(frame, 200, write).await?;
            }
            other => {
                tracing::debug!(r#type = other, "ignoring unknown data frame type");
                self.send_ack(frame, 200, write).await?;
            }
        }
        Ok(())
    }

    /// Send an ACK response frame back to the server.
    async fn send_ack(
        &self,
        original: &Frame,
        status_code: i32,
        write: &Arc<Mutex<futures_util::stream::SplitSink<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            Message,
        >>>,
    ) -> Result<()> {
        let mut resp_frame = Frame {
            seq_id: original.seq_id,
            log_id: original.log_id,
            service: original.service,
            method: frame::method::DATA,
            headers: original.headers.clone(),
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: original.log_id_new.clone(),
        };

        // Build the Response JSON payload
        let ack = serde_json::json!({
            "code": status_code,
            "headers": {},
            "data": []
        });
        resp_frame.payload = serde_json::to_vec(&ack).context("serializing ACK payload")?;

        let bytes = resp_frame.encode();
        let mut w = write.lock().await;
        w.send(Message::Binary(bytes.into()))
            .await
            .context("sending ACK frame")?;
        Ok(())
    }

    // ---- internal: bootstrap ----------------------------------------------

    async fn get_ws_url(&self) -> Result<String> {
        let url = format!(
            "{}{}",
            self.domain.trim_end_matches('/'),
            ENDPOINT_PATH
        );
        let body = BootstrapRequest {
            app_id: self.app_id.clone(),
            app_secret: self.app_secret.clone(),
        };

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("locale", "zh")
            .json(&body)
            .send()
            .await
            .context("POST /callback/ws/endpoint")?;

        let status = resp.status();
        let bytes = resp.bytes().await.context("reading endpoint response")?;
        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
            bail!("endpoint HTTP {status}: {snippet}");
        }

        let ep: EndpointResp = serde_json::from_slice(&bytes).with_context(|| {
            let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
            format!("decoding endpoint response: {snippet}")
        })?;

        match ep.code {
            0 => {}
            1 => bail!("飞书系统繁忙，请稍后重试"),
            1000040343 => bail!("飞书内部错误: {}", ep.msg),
            code => bail!("飞书 endpoint 错误 code={code} msg={}", ep.msg),
        }

        let data = ep.data.ok_or_else(|| anyhow!("endpoint response missing data"))?;
        if data.url.is_empty() {
            bail!("endpoint returned empty URL");
        }

        // Apply server-pushed config
        if let Some(conf) = data.client_config {
            let mut current = self.client_config.write().await;
            *current = conf;
        }

        Ok(data.url)
    }

    // ---- internal: tenant_access_token ------------------------------------

    async fn get_tenant_token(&self) -> Result<String> {
        {
            let ts = self.token_state.read().await;
            if let Some(token) = &ts.token {
                let now = chrono::Utc::now().timestamp();
                if now < ts.expires_at - 60 {
                    return Ok(token.clone());
                }
            }
        }

        // Refresh
        let url = format!(
            "{}{}",
            self.domain.trim_end_matches('/'),
            TOKEN_PATH
        );
        let body = TokenRequest {
            app_id: self.app_id.clone(),
            app_secret: self.app_secret.clone(),
        };

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("POST tenant_access_token")?;

        let status = resp.status();
        let bytes = resp.bytes().await.context("reading token response")?;
        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
            bail!("token HTTP {status}: {snippet}");
        }

        let tr: TokenResp = serde_json::from_slice(&bytes).with_context(|| {
            let snippet = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
            format!("decoding token response: {snippet}")
        })?;

        if tr.code != 0 {
            bail!("token api error code={} msg={}", tr.code, tr.msg);
        }

        let expires_at = chrono::Utc::now().timestamp() + tr.expire as i64;
        let token = tr.tenant_access_token;

        {
            let mut ts = self.token_state.write().await;
            ts.token = Some(token.clone());
            ts.expires_at = expires_at;
        }

        Ok(token)
    }
}
