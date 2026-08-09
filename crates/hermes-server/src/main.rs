//! Standalone `hermes-server` binary. The primary entry point is `hermes serve`
//! (in `hermes-cli`); this binary exists for `cargo run -p hermes-server`
//! during development. Configure via env vars: `PORT` (default 8765),
//! `HOST` (default `127.0.0.1`), and `HERMES_SERVER_TOKEN` (else a token is
//! read/generated at `~/.lebi-ai/server.token`).

use std::net::IpAddr;
use std::sync::Arc;

use hermes_server::auth::{resolve_token, TokenOpts};
use hermes_server::serve;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("HERMES_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hermes_server=info")),
        )
        .init();
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8765);
    let host: IpAddr = std::env::var("HOST")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]));
    let token = Arc::new(resolve_token(&TokenOpts::default())?);
    serve(host, port, token).await
}
