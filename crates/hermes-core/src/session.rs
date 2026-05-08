//! Session: a transcript of one chat run.
//!
//! Sessions are persisted as JSONL — one [`SessionEvent`] per line — so a
//! crashed run leaves a partially-recoverable file. The reflection pipeline
//! reads sessions back to produce skill / memory candidates.

use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::provider::Usage;

/// Top-level metadata for a session. Written as the first JSONL line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub model: String,
    pub provider: String,
}

impl SessionMeta {
    pub fn new(model: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            created_at: chrono::Utc::now(),
            model: model.into(),
            provider: provider.into(),
        }
    }
}

/// One line in a session's JSONL transcript.
///
/// Externally tagged so each line is `{"meta": {...}}` / `{"message": {...}}`
/// / `{"usage": {...}}` — easy to grep and to parse incrementally.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEvent {
    Meta(SessionMeta),
    Message(Message),
    Usage(Usage),
}

/// In-memory view of a session. Persistence is decoupled (see hermes-store).
#[derive(Debug, Clone)]
pub struct Session {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl Session {
    pub fn new(meta: SessionMeta) -> Self {
        Self {
            meta,
            messages: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    pub fn push_user(&mut self, text: impl Into<String>) -> &Message {
        self.messages.push(Message::user_text(text));
        self.messages.last().unwrap()
    }

    pub fn push_assistant(&mut self, content: Vec<crate::ContentBlock>) -> &Message {
        use crate::Role;
        self.messages.push(Message {
            role: Role::Assistant,
            content,
        });
        self.messages.last().unwrap()
    }

    pub fn record_usage(&mut self, usage: Usage) {
        self.total_input_tokens = self.total_input_tokens.saturating_add(usage.input_tokens);
        self.total_output_tokens = self.total_output_tokens.saturating_add(usage.output_tokens);
    }
}
