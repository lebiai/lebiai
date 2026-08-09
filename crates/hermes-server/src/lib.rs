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
    let app: Router = routes::build(state, token.clone());
    let addr = SocketAddr::from((host, port));
    tracing::info!(%addr, "hermes-server listening");
    tracing::info!(
        token = %token,
        "auth token (set this in the client's server settings; REST: Authorization header, WS: ?token=)"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
