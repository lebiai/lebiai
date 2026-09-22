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
    /// **这一轮开口的人**（人物 id，如 `lv-lao-shi` / `wang-hai-yan`）。
    ///
    /// 为什么在消息上而不是在会话上：项目组里**每一轮的人会换** —— 棒跟着产物走，
    /// 会话级的字段只能记住"最后一个人"。2026-09-21 实测：不盖这个字段，吕老师
    /// 开口那一轮的落盘消息会和海燕的合并成同一个气泡，界面上**看不出她说过话**
    /// （用户原话：「吕老师消息被埋」）。
    ///
    /// 只有 assistant 侧才有值：user 侧是工具结果与引擎提示，不是"人说的话"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
            speaker: None,
        }
    }

    /// Human tapped send — stamps `at` for the distill ledger.
    pub fn user_sent(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: Some(chrono::Utc::now()),
            speaker: None,
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
            speaker: None,
        }
    }

    /// 盖上这一轮开口的人（人物 id）。
    pub fn set_speaker(&mut self, id: impl Into<String>) {
        self.speaker = Some(id.into());
    }

    /// Strip fake material citations from assistant text (persist + display).
    pub fn sanitize_material_cites(&mut self, allowed_titles: &[String]) {
        if self.role != Role::Assistant {
            return;
        }
        for b in &mut self.content {
            if let ContentBlock::Text { text } = b {
                *text = crate::companion::sanitize_material_citations(text, allowed_titles);
            }
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
            speaker: self.speaker.clone(),
        }
    }

    /// Prepare a message for JSONL append.
    pub fn for_persist(&self, persist_thinking: bool) -> Self {
        if self.is_internal_instruction_only() {
            return Self {
                role: self.role,
                content: Vec::new(),
                at: None,
                speaker: None,
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

    /// 这条消息发出去时还剩东西吗？
    ///
    /// OpenAI 兼容线格式里，一条「既没有文本、也没有 tool_calls」的 assistant 消息会退化成
    /// `{"role":"assistant"}`，DeepSeek 直接 400：`Invalid assistant message: content or
    /// tool_calls must be set`。2026-09-20 实测：一轮输出顶到 `max_tokens` 后留下这样一条
    /// 空 assistant（只在内存里——没内容就不落盘），从此**该会话每一次请求都 400**，
    /// 用户看到的是「吕老师的工作全都报错」。
    ///
    /// 判据只有这一处：修复历史时用它，请求体落线前也用它。
    pub fn has_sendable_content(&self) -> bool {
        self.content.iter().any(|b| match b {
            ContentBlock::Text { text } => {
                !text.trim().is_empty()
                    && !(self.role == Role::User
                        && crate::companion::is_internal_instruction_text(text))
            }
            // 放错边的工具块由配对修复去管，这里只认它自己这一侧。
            ContentBlock::ToolUse { .. } => self.role == Role::Assistant,
            ContentBlock::ToolResult { .. } => self.role == Role::User,
            ContentBlock::Image { .. } => true,
            // 思考块落线时被丢掉（`openai.rs` assistant 分支），不算内容。
            ContentBlock::Thinking { .. } => false,
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

/// 给一轮里新出来的 assistant 消息盖上**说话人**（人物 id）—— 判定只此一处。
///
/// 为什么要有这个函数、而不是让 GUI / CLI / server 各自写一遍：三个入口都要盖同一个章，
/// 谁少盖一处，那个入口的项目组会话就"看不见人说话"。`speaker` 为 `None` 时什么都不做
/// （无人物会话、引擎批处理），保持旧行为。
///
/// 只盖 assistant：user 那一侧装的是工具结果与引擎提示，不是人说的话。
pub fn stamp_speaker(messages: &mut [Message], speaker: Option<&str>) {
    let Some(id) = speaker else {
        return;
    };
    for m in messages.iter_mut() {
        if m.role == Role::Assistant {
            m.set_speaker(id);
        }
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
                // 空 assistant 不许进请求体（见 `has_sendable_content`）：它一进去，
                // 之后每一次请求都会被 provider 拒收。
                if msg.has_sendable_content() {
                    out.push(msg.clone());
                    open_tool_ids = tool_ids;
                }
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
                        speaker: None,
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
        speaker: None,
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
                speaker: None,
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
                speaker: None,
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "ok".into(),
                    is_error: false,
                }],
                at: None,
                speaker: None,
            },
            Message::assistant_text("done"),
        ];
        let fixed = sanitize_history_for_provider(&history);
        assert_eq!(fixed.len(), 4);
    }

    /// 空 assistant（只有思考块 / 什么都没有）不许进请求体：线格式里它只剩
    /// `{"role":"assistant"}`，端点 400「content or tool_calls must be set」。
    /// 2026-09-20 现场：一条这样的消息让整个会话之后每一次请求都报错。
    #[test]
    fn drops_an_assistant_message_that_would_be_sent_empty() {
        let history = vec![
            Message::user_text("干活"),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Thinking {
                    thinking: "想了半天，一个字没写".into(),
                    signature: None,
                }],
                at: None,
                speaker: None,
            },
            Message {
                role: Role::Assistant,
                content: Vec::new(),
                at: None,
                speaker: None,
            },
            Message::assistant_text("真回答"),
        ];
        let fixed = sanitize_history_for_provider(&history);
        assert_eq!(fixed.len(), 2, "空 assistant 要丢掉：{fixed:?}");
        assert_eq!(fixed[1].content[0].as_text(), Some("真回答"));
    }

    /// 判据本身：谁能上、谁不能上。
    #[test]
    fn only_messages_with_something_to_send_count() {
        assert!(Message::user_text("在").has_sendable_content());
        assert!(!Message::user_text("   ").has_sendable_content());
        assert!(Message::assistant_text("答").has_sendable_content());
        assert!(
            !Message {
                role: Role::Assistant,
                content: Vec::new(),
                at: None,
                speaker: None,
            }
            .has_sendable_content(),
            "什么都没有 = 发不出去"
        );
        let thinking_only = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Thinking {
                thinking: "只有思考".into(),
                signature: None,
            }],
            at: None,
            speaker: None,
        };
        assert!(
            !thinking_only.has_sendable_content(),
            "思考块落线时被丢掉，不算内容"
        );
        assert!(
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({}),
                }],
                at: None,
                speaker: None,
            }
            .has_sendable_content(),
            "工具调用算内容"
        );
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
}

#[cfg(test)]
mod speaker_tests {
    use super::*;

    fn assistant_text(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
            speaker: None,
        }
    }

    /// 只盖 assistant：user 那一侧装的是工具结果与引擎提示，不是人说的话。
    #[test]
    fn stamps_the_assistant_side_only() {
        let mut msgs = vec![Message::user_text("干活"), assistant_text("好")];
        stamp_speaker(&mut msgs, Some("lv-lao-shi"));
        assert_eq!(msgs[0].speaker, None);
        assert_eq!(msgs[1].speaker.as_deref(), Some("lv-lao-shi"));
    }

    /// 没有人物的会话（旧 transcript、IM 渠道）保持原样，不凭空署名。
    #[test]
    fn no_persona_leaves_transcript_untouched() {
        let mut msgs = vec![assistant_text("好")];
        stamp_speaker(&mut msgs, None);
        assert_eq!(msgs[0].speaker, None);
    }

    /// 署名要活过落盘与上下文瘦身，否则重开会话又分不出谁说的。
    #[test]
    fn the_stamp_survives_persist_and_context_strip() {
        let mut m = assistant_text("好");
        stamp_speaker(std::slice::from_mut(&mut m), Some("xiao-song"));
        assert_eq!(m.for_persist(true).speaker.as_deref(), Some("xiao-song"));
        assert_eq!(m.without_thinking().speaker.as_deref(), Some("xiao-song"));
    }
}
