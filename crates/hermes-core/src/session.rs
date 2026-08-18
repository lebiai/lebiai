//! Session: a transcript of one chat run.
//!
//! Sessions are persisted as JSONL — one [`SessionEvent`] per line — so a
//! crashed run leaves a partially-recoverable file. The reflection pipeline
//! reads sessions back to produce skill / memory candidates.

use serde::{Deserialize, Serialize};

use crate::message::{ContentBlock, Message};
use crate::provider::Usage;

/// Top-level metadata for a session. Written as the first JSONL line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub model: String,
    pub provider: String,
    /// Display title for sidebars. Optional for backward-compatible JSONL.
    /// Updated when the first *meaningful* user message is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl SessionMeta {
    pub fn new(model: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            created_at: chrono::Utc::now(),
            model: model.into(),
            provider: provider.into(),
            title: None,
        }
    }
}

/// Default placeholder when no user text exists yet.
pub const DEFAULT_SESSION_TITLE: &str = "New Chat";

/// Short greetings / empty chatter — not useful as a permanent session title.
pub fn is_trivial_user_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    // Pure punctuation / emoji-ish very short
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 1 {
        return true;
    }
    let lower = t.to_lowercase();
    const GREETINGS: &[&str] = &[
        "hi",
        "hi!",
        "hey",
        "hey!",
        "hello",
        "hello!",
        "你好",
        "你好啊",
        "你好!",
        "你好！",
        "您好",
        "您好！",
        "哈喽",
        "嗨",
        "在吗",
        "在吗?",
        "在吗？",
        "早上好",
        "晚上好",
        "下午好",
        "早",
        "晚安",
        "谢谢",
        "thanks",
        "thank you",
        "ok",
        "okay",
        "好的",
        "嗯",
        "嗯嗯",
        "哦",
        "噢",
    ];
    if GREETINGS.iter().any(|g| lower == *g || t == *g) {
        return true;
    }
    // Very short pure greeting-like (≤4 CJK/latin words without structure)
    if chars.len() <= 4 && !t.contains(['?', '？', '。', '.', '!', '！', '，', ',']) {
        // still allow short real queries like "推荐二" (3 chars, meaningful)
        // only treat as trivial if matches greeting set or is pure "你好*"
        if t.starts_with("你好") && chars.len() <= 4 {
            return true;
        }
    }
    false
}

/// Truncate for sidebar display (Unicode-aware).
pub fn truncate_title(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.trim().chars().collect();
    if chars.is_empty() {
        return DEFAULT_SESSION_TITLE.to_string();
    }
    if chars.len() <= max_chars {
        return chars.into_iter().collect();
    }
    let head: String = chars
        .into_iter()
        .take(max_chars.saturating_sub(3))
        .collect();
    format!("{head}...")
}

/// Prefer first non-trivial user text; else first any user text; else default.
pub fn derive_title_from_messages(messages: &[Message]) -> String {
    let mut first_any: Option<String> = None;
    for m in messages {
        if !m.is_human_send() {
            continue;
        }
        for block in &m.content {
            let ContentBlock::Text { text } = block else {
                continue;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if first_any.is_none() {
                first_any = Some(trimmed.to_string());
            }
            if !is_trivial_user_text(trimmed) {
                return truncate_title(trimmed, 60);
            }
        }
    }
    first_any
        .map(|s| truncate_title(&s, 60))
        .unwrap_or_else(|| DEFAULT_SESSION_TITLE.to_string())
}

/// True if the session has at least one real human send (not Care / tool results).
pub fn session_has_user_text(messages: &[Message]) -> bool {
    messages.iter().any(|m| m.is_human_send())
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
        self.messages.push(Message::user_sent(text));
        self.messages.last().unwrap()
    }

    pub fn push_assistant(&mut self, content: Vec<crate::ContentBlock>) -> &Message {
        use crate::Role;
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            at: None,
        });
        self.messages.last().unwrap()
    }

    /// Count of human sends and the timestamp of the last one (`at` may be
    /// missing on sessions written before send times were recorded).
    pub fn last_human_send(&self) -> (u32, Option<chrono::DateTime<chrono::Utc>>) {
        last_human_send(&self.messages)
    }

    pub fn record_usage(&mut self, usage: Usage) {
        self.total_input_tokens = self.total_input_tokens.saturating_add(usage.input_tokens);
        self.total_output_tokens = self.total_output_tokens.saturating_add(usage.output_tokens);
    }
}

/// See [`Session::last_human_send`].
pub fn last_human_send(messages: &[Message]) -> (u32, Option<chrono::DateTime<chrono::Utc>>) {
    let mut seq = 0u32;
    let mut at = None;
    for m in messages {
        if m.is_human_send() {
            seq += 1;
            if m.at.is_some() {
                at = m.at;
            }
        }
    }
    (seq, at)
}

#[cfg(test)]
mod title_tests {
    use super::*;
    use crate::message::Message;

    #[test]
    fn skips_greetings_for_title() {
        let msgs = vec![
            Message::user_text("你好啊"),
            Message::user_text("我最近在做短视频"),
        ];
        assert_eq!(derive_title_from_messages(&msgs), "我最近在做短视频");
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(derive_title_from_messages(&[]), DEFAULT_SESSION_TITLE);
    }

    #[test]
    fn trivial_detection() {
        assert!(is_trivial_user_text("你好啊"));
        assert!(is_trivial_user_text("hi"));
        assert!(!is_trivial_user_text("推荐二"));
        assert!(!is_trivial_user_text("我其实是一个抖音达人"));
    }

    #[test]
    fn last_human_send_skips_tools_and_care() {
        let sent = Message::user_sent("查一下");
        let at = sent.at;
        let msgs = vec![
            sent,
            Message::assistant_text("ok"),
            Message {
                role: crate::Role::User,
                content: vec![crate::ContentBlock::ToolResult {
                    tool_use_id: "t".into(),
                    content: "x".into(),
                    is_error: false,
                }],
                at: None,
            },
            Message::user_text(crate::companion::care_after_tools_nudge()),
        ];
        let (seq, got) = last_human_send(&msgs);
        assert_eq!(seq, 1);
        assert_eq!(got, at);
    }

    #[test]
    fn last_human_send_keeps_earlier_stamp() {
        let sent = Message::user_sent("先");
        let at = sent.at;
        let later = Message::user_text("后");
        assert!(later.at.is_none());
        let (seq, got) = last_human_send(&[sent, later]);
        assert_eq!(seq, 2);
        assert_eq!(got, at);
    }

    #[test]
    fn care_only_session_has_no_user_text() {
        let msgs = vec![Message::user_text(crate::companion::care_after_tools_nudge())];
        assert!(!session_has_user_text(&msgs));
        assert_eq!(derive_title_from_messages(&msgs), DEFAULT_SESSION_TITLE);
    }
}
