//! Telegram Bot API client for Hermes.
//!
//! Mirrors the shape of `hermes-weixin` (HTTP long-poll) rather than
//! `hermes-feishu` (WebSocket) — Telegram's Bot API is plain HTTP: a
//! `getUpdates` long-poll loop for inbound messages and `sendMessage` for
//! replies.

pub mod auth;
pub mod client;

pub use auth::StoredCreds;
pub use client::{BotUser, Chat, Client, Message, Update};
