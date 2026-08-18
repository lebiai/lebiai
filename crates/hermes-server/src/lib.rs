//! HTTP/WebSocket server for lebi-AI agents — the Flutter client backend.
//!
//! `serve(port)` builds an [`AppState`] (mirroring `hermes-gui`), wires axum
//! routes, and serves. The chat WebSocket drives `hermes_turn::run_turn` and
//! streams [`events::ChatStreamEvent`]s back, alongside REST endpoints for
//! sessions/skills/memory/config/reflect/uploads.

pub mod auth;
pub mod context;
pub mod error;
pub mod events;
pub mod routes;
pub mod state;
pub mod tickets;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use axum::Router;

/// Initialize state, build the router behind the bearer-token auth layer, and
/// serve on `host:port`. `token` gates every `/api/v1/*` request (REST header
/// or `?token=` on the WS upgrade). Default `host` should be `127.0.0.1` —
/// expose on the LAN/internet with `--host 0.0.0.0` (token still required).
///
/// Tracing is left to the caller (`main.rs` or `hermes-cli`), which already
/// installs a subscriber before reaching here.
pub async fn serve(host: IpAddr, port: u16, token: Arc<String>) -> Result<()> {
    let state = Arc::new(state::AppState::init().await?);
    let tickets = state.ws_tickets.clone();
    let app: Router = routes::build(state, token.clone(), tickets);
    let addr = SocketAddr::from((host, port));
    tracing::info!(%addr, "hermes-server listening");
    // Never log the full secret — only a short fingerprint for operators.
    let fingerprint = token_fingerprint(token.as_str());
    tracing::info!(
        token_fp = %fingerprint,
        "auth required (REST: Authorization Bearer; WS prefer POST /api/v1/ws-ticket then ?ticket=). Full token is in ~/.lebi-ai/server.token — not logged."
    );
    // One-time human-readable hint on stdout only (not structured logs).
    eprintln!(
        "hermes-server auth token fingerprint: {fingerprint} (full value: ~/.lebi-ai/server.token)"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    if !addr.ip().is_loopback() {
        tracing::warn!(
            %addr,
            "binding non-loopback address — ensure TLS (reverse proxy) and protect server.token; traffic is HTTP cleartext"
        );
    }
    axum::serve(listener, app).await?;
    Ok(())
}

/// First 4 + last 4 chars of the token (or shorter mask) for safe logs.
fn token_fingerprint(token: &str) -> String {
    let t = token.trim();
    if t.len() <= 8 {
        return "****".into();
    }
    format!("{}…{}", &t[..4], &t[t.len() - 4..])
}
