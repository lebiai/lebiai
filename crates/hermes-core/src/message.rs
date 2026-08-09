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
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
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
        }
    }

    /// Prepare a message for JSONL append.
    pub fn for_persist(&self, persist_thinking: bool) -> Self {
        if persist_thinking {
            self.clone()
        } else {
            self.without_thinking()
        }
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
