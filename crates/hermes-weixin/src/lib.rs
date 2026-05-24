//! WeChat (iLink Bot) protocol client for Hermes.
//!
//! See `weixin-bot-api.md` for the upstream protocol description.

pub mod auth;
pub mod client;
pub mod types;

pub use auth::{LoginSession, QrPollState, StoredCreds};
pub use client::{Client, DEFAULT_BASE_URL};
pub use types::{CHANNEL_VERSION, MessageItem, WeixinMessage};
