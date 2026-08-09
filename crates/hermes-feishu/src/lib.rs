//! Feishu (Lark) long-connection protocol client for lebi-AI.
//!
//! Provides a WebSocket-based event receiver (mirroring the Go SDK's
//! `oapi-sdk-go/ws` package) and an HTTP messaging API for sending
//! text replies.

pub mod auth;
pub mod client;
pub mod frame;

pub use auth::StoredCreds;
pub use client::{EventPayload, FeishuClient, MessageBody, MessageReceiveEvent, Sender, SenderId};
