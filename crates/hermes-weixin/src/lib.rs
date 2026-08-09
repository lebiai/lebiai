//! WeChat (iLink Bot) protocol client for lebi-AI.
//! Upstream protocol details live in the iLink Bot API docs (external).

pub mod auth;
pub mod client;
pub mod service;
pub mod types;

pub use auth::{LoginSession, QrPollState, StoredCreds};
pub use client::{Client, DEFAULT_BASE_URL};
pub use types::{MessageItem, WeixinMessage, CHANNEL_VERSION};
