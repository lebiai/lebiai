//! Core message and content-block types shared across providers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
    /// Wall-clock when a **human** sent this. Absent on tool results, Care
    /// nudges, and messages written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
        }
    }

    /// Human tapped send — stamps `at` for the distill ledger.
    pub fn user_sent(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: Some(chrono::Utc::now()),
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
        }
    }

    /// Drop thinking blocks (for compact session logs).
    pub fn without_thinking(&self) -> Self {
        Self {
            role: self.role,
            content: self
                .content
                .iter()
                .filter(|b| !matches!(b, ContentBlock::Thinking { .. }))
                .cloned()
                .collect(),
            at: self.at,
        }
    }

    /// Prepare a message for JSONL append.
    pub fn for_persist(&self, persist_thinking: bool) -> Self {
        if self.is_internal_instruction_only() {
            return Self {
                role: self.role,
                content: Vec::new(),
                at: None,
            };
        }
        if persist_thinking {
            self.clone()
        } else {
            self.without_thinking()
        }
    }

    /// Synthetic engine nudge (Care / time header / tool-budget), not a person.
    pub fn is_internal_instruction_only(&self) -> bool {
        if self.role != Role::User {
            return false;
        }
        let mut saw = false;
        for b in &self.content {
            match b {
                ContentBlock::Text { text } if text.trim().is_empty() => {}
                ContentBlock::Text { text }
                    if crate::companion::is_internal_instruction_text(text) =>
                {
                    saw = true;
                }
                _ => return false,
            }
        }
        saw
    }

    /// A real send from the person (not tool results, not engine nudges).
    pub fn is_human_send(&self) -> bool {
        if self.role != Role::User
            || self.is_tool_result_only()
            || self.is_internal_instruction_only()
        {
            return false;
        }
        self.content.iter().any(|b| {
            matches!(b, ContentBlock::Text { text } if !text.trim().is_empty())
                || matches!(b, ContentBlock::Image { .. })
        })
    }

    /// User message with no human text (only tool results / empty) — hide in chat UI.
    pub fn is_tool_result_only(&self) -> bool {
        if self.role != Role::User {
            return false;
        }
        let mut has_tool_result = false;
        for b in &self.content {
            match b {
                ContentBlock::Text { text } if !text.trim().is_empty() => return false,
                ContentBlock::Image { .. } => return false,
                ContentBlock::ToolResult { .. } => has_tool_result = true,
                ContentBlock::ToolUse { .. } | ContentBlock::Thinking { .. } => {}
                ContentBlock::Text { .. } => {}
            }
        }
        has_tool_result
    }
}

/// Repair transcript so providers (esp. OpenAI-compatible) accept it.
///
/// A common failure mode after crash / cancel / old bugs: an assistant message
/// contains `tool_use` blocks whose matching `tool_result` never landed. APIs
/// then reject the next turn with HTTP 400 ("tool_calls must be followed by
/// tool messages…").
///
/// Strategy:
/// 1. For each assistant tool_use id without a following user tool_result,
///    append synthetic error results on a new user message.
/// 2. Drop orphan tool_result blocks whose tool_use_id was never opened.
/// 3. Drop empty messages after cleanup.
pub fn sanitize_history_for_provider(messages: &[Message]) -> Vec<Message> {
    let mut out: Vec<Message> = Vec::with_capacity(messages.len() + 2);
    let mut open_tool_ids: Vec<String> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::Assistant => {
                // Close any still-open tools before a new assistant turn.
                if !open_tool_ids.is_empty() {
                    out.push(synthetic_tool_results(&open_tool_ids));
                    open_tool_ids.clear();
                }
                let mut tool_ids = Vec::new();
                for b in &msg.content {
                    if let ContentBlock::ToolUse { id, .. } = b {
                        tool_ids.push(id.clone());
                    }
                }
                out.push(msg.clone());
                open_tool_ids = tool_ids;
            }
            Role::User => {
                let mut kept: Vec<ContentBlock> = Vec::new();
                let mut answered: Vec<String> = Vec::new();
                for b in &msg.content {
                    match b {
                        ContentBlock::ToolResult { tool_use_id, .. } => {
                            if open_tool_ids.iter().any(|id| id == tool_use_id) {
                                answered.push(tool_use_id.clone());
                                kept.push(b.clone());
                            }
                            // else: orphan result — drop
                        }
                        ContentBlock::Text { text }
                            if crate::companion::is_internal_instruction_text(text) => {}
                        other => kept.push(other.clone()),
                    }
                }
                open_tool_ids.retain(|id| !answered.iter().any(|a| a == id));
                // If some tool_uses still open after this user message, and this
                // user message had tool results or is purely tool-related, keep
                // going — results may arrive later. If user message is a new
                // human turn (has text) while tools are still open, close them.
                let has_human_text = kept.iter().any(|b| {
                    matches!(b, ContentBlock::Text { text } if !text.trim().is_empty())
                        || matches!(b, ContentBlock::Image { .. })
                });
                if has_human_text && !open_tool_ids.is_empty() {
                    out.push(synthetic_tool_results(&open_tool_ids));
                    open_tool_ids.clear();
                }
                if !kept.is_empty() {
                    out.push(Message {
                        role: Role::User,
                        content: kept,
                        at: msg.at,
                    });
                }
            }
        }
    }
    if !open_tool_ids.is_empty() {
        out.push(synthetic_tool_results(&open_tool_ids));
    }
    out
}

fn synthetic_tool_results(ids: &[String]) -> Message {
    Message {
        role: Role::User,
        at: None,
        content: ids
            .iter()
            .map(|id| ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content: "Tool call was interrupted or never completed (history repair).".into(),
                is_error: true,
            })
            .collect(),
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    #[test]
    fn fills_missing_tool_results() {
        let history = vec![
            Message::user_text("hi"),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                }],
                at: None,
            },
            // missing tool result — user speaks again
            Message::user_text("continue"),
        ];
        let fixed = sanitize_history_for_provider(&history);
        // expect: user, assistant, synthetic tool results, user continue
        assert!(fixed.len() >= 4);
        let synth = &fixed[2];
        assert_eq!(synth.role, Role::User);
        assert!(matches!(
            &synth.content[0],
            ContentBlock::ToolResult { tool_use_id, is_error: true, .. }
                if tool_use_id == "t1"
        ));
        assert_eq!(fixed.last().unwrap().content[0].as_text(), Some("continue"));
    }

    #[test]
    fn keeps_complete_pairs() {
        let history = vec![
            Message::user_text("hi"),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({}),
                }],
                at: None,
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "ok".into(),
                    is_error: false,
                }],
                at: None,
            },
            Message::assistant_text("done"),
        ];
        let fixed = sanitize_history_for_provider(&history);
        assert_eq!(fixed.len(), 4);
    }

    #[test]
    fn persist_and_sanitize_drop_care_nudge() {
        let care = Message::user_text(crate::companion::care_after_tools_nudge());
        assert!(care.is_internal_instruction_only());
        assert!(care.for_persist(false).content.is_empty());
        let history = vec![
            Message::user_text("写成 word"),
            Message::assistant_text("wrote"),
            care,
        ];
        let fixed = sanitize_history_for_provider(&history);
        assert_eq!(fixed.len(), 2);
        assert!(!fixed
            .iter()
            .any(|m| m.content.iter().any(
                |b| matches!(b, ContentBlock::Text { text } if text.contains("[lebi-AI Care]"))
            )));
    }
}

/// A single block within a message. Mirrors the Anthropic Messages API
/// content-block taxonomy because that is the most expressive of the
/// providers we target; OpenAI-style providers translate down.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    /// Anthropic extended-thinking block. Some providers (DeepSeek) emit
    /// this before the final text block; we surface it for transparency
    /// but treat it as auxiliary content (not assistant-visible reply).
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    /// Inline image (Anthropic "base64" source shape). Only valid in user
    /// messages. The Anthropic provider serializes it directly (its request
    /// body serde-encodes `ContentBlock`); OpenAI-style providers currently
    /// drop it to a placeholder (their `content` is plain text).
    Image {
        source: ImageSource,
    },
}

/// Inline image source — Anthropic "base64" shape. Serialized into the
/// session log too, so history replay can render the image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub media_type: String,
    pub data: String,
}

impl ContentBlock {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn as_thinking(&self) -> Option<&str> {
        match self {
            ContentBlock::Thinking { thinking, .. } => Some(thinking),
            _ => None,
        }
    }
}
