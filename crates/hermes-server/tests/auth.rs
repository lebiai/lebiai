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

use hermes_server::auth::{auth_middleware, AuthState};
use hermes_server::tickets::TicketStore;

const TOKEN: &str = "deadbeef-secret-token";

fn app() -> (Router, Arc<TicketStore>) {
    let tickets = Arc::new(TicketStore::default());
    let auth = AuthState {
        token: Arc::new(TOKEN.to_string()),
        tickets: tickets.clone(),
    };
    let router = Router::new()
        .route("/api/v1/health", get(|| async { "ok" }))
        .route("/api/v1/chat", get(stub_ws))
        .layer(from_fn_with_state(auth, auth_middleware));
    (router, tickets)
}

async fn stub_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move {
        drop(socket);
    })
}

async fn start() -> (String, String, Arc<TicketStore>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (app, tickets) = app();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let base = format!("http://{addr}");
    let host = format!("{addr}");
    (base, host, tickets)
}

#[tokio::test]
async fn no_token_is_unauthorized() {
    let (base, _, _) = start().await;
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
    let (base, _, _) = start().await;
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
    let (base, _, _) = start().await;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/v1/health"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn correct_query_token_is_ok() {
    let (base, _, _) = start().await;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?token={TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn wrong_query_token_is_unauthorized() {
    let (base, _, _) = start().await;
    let status = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?token=nope"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ticket_once_then_invalid() {
    let (base, _, tickets) = start().await;
    let t = tickets.issue();
    let ok = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?ticket={t}"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(ok, reqwest::StatusCode::OK);
    let again = reqwest::Client::new()
        .get(format!("{base}/api/v1/health?ticket={t}"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(again, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_upgrade_without_token_is_401() {
    let (_, host, _) = start().await;
    let mut stream = TcpStream::connect(&host).await.unwrap();
    let req = format!(
        "GET /api/v1/chat HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 512];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(
        resp.starts_with("HTTP/1.1 401"),
        "expected 401, got: {resp}"
    );
}

#[tokio::test]
async fn ws_upgrade_with_token_is_101() {
    let (_, host, _) = start().await;
    let mut stream = TcpStream::connect(&host).await.unwrap();
    let req = format!(
        "GET /api/v1/chat?token={TOKEN} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 512];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(
        resp.starts_with("HTTP/1.1 101"),
        "expected 101, got: {resp}"
    );
}

#[tokio::test]
async fn ws_upgrade_with_ticket_is_101() {
    let (_, host, tickets) = start().await;
    let t = tickets.issue();
    let mut stream = TcpStream::connect(&host).await.unwrap();
    let req = format!(
        "GET /api/v1/chat?ticket={t} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 512];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(
        resp.starts_with("HTTP/1.1 101"),
        "expected 101, got: {resp}"
    );
}
