//! Auth middleware integration tests.
//!
//! Builds a minimal router (no full AppState) with the real auth middleware
//! and a stub WebSocket upgrade route, then exercises it over HTTP (reqwest)
//! and a raw TCP WebSocket handshake.

use std::sync::Arc;

use axum::extract::WebSocketUpgrade;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use hermes_server::auth::auth_middleware;

const TOKEN: &str = "deadbeef-secret-token";

fn app() -> Router {
    let token = Arc::new(TOKEN.to_string());
    Router::new()
        .route("/api/v1/health", get(|| async { "ok" }))
        .route("/api/v1/chat", get(stub_ws))
        .layer(from_fn_with_state(token, auth_middleware))
}

async fn stub_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    // Accept the upgrade; we don't exchange frames — the test only checks the
    // 101 vs 401 status line.
    ws.on_upgrade(|socket| async move {
        drop(socket);
    })
}

/// Bind the app on an ephemeral port; returns the base URL + host:port.
async fn start() -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let base = format!("http://{addr}");
    let host = format!("{addr}");
    (base, host)
}

#[tokio::test]
async fn no_token_is_unauthorized() {
    let (base, _) = start().await;
    let status = reqwest::Client::new()
        .get(format!("{base}/api/v1/health"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_is_unauthorized() {
    let (base, _) = start().await;
    let status = reqwest::Client::new()
        .get(format!("{base}/api/v1/health"))
        .header("Authorization", "Bearer nope")
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn correct_header_is_ok() {
    let (base, _) = start().await;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/v1/health"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn correct_query_token_is_ok() {
    let (base, _) = start().await;
    let status = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?token={TOKEN}"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::OK);
}

#[tokio::test]
async fn wrong_query_token_is_unauthorized() {
    let (base, _) = start().await;
    let status = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?token=wrong"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

/// Raw TCP WebSocket handshake: the auth middleware sees the upgrade GET and
/// gates it on `?token=`. Correct token → `101 Switching Protocols`; none →
/// `401`. (A WS client can't set headers on the browser handshake, so the
/// query param is the transport-level auth path.)
async fn ws_status(host: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(host).await.unwrap();
    let key = base64_key();
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: {key}\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf[..n])
        .lines()
        .next()
        .unwrap()
        .to_string()
}

fn base64_key() -> String {
    // 16 zero bytes → fixed valid base64 key; content is irrelevant here.
    String::from("AAAAAAAAAAAAAAAAAAAAAA==")
}

#[tokio::test]
async fn ws_upgrade_without_token_is_401() {
    let (_, host) = start().await;
    let status = ws_status(&host, "/api/v1/chat").await;
    assert!(status.contains(" 401 "), "got: {status}");
}

#[tokio::test]
async fn ws_upgrade_with_token_is_101() {
    let (_, host) = start().await;
    let status = ws_status(&host, &format!("/api/v1/chat?token={TOKEN}")).await;
    assert!(status.contains(" 101 "), "got: {status}");
}
