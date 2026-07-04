//! `hermes serve` — start the HTTP/WebSocket server.
//!
//! Thin wrapper around [`hermes_server::serve`]; all wiring (incl. token
//! resolution + auth middleware) lives in the `hermes-server` crate so it can
//! also run standalone (`cargo run -p hermes-server`).

use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;

pub async fn run(host: IpAddr, port: u16, token: Arc<String>) -> Result<()> {
    hermes_server::serve(host, port, token).await
}
