//! Context compaction: summarise old messages when approaching the model's
//! context limit, keeping recent turns verbatim.

use crate::{CompletionRequest, ContentBlock, LlmProvider, Message, Role, Session};

const COMPACTION_SYSTEM: &str = "Summarize the following conversation preserving: key decisions made, tool results and their outcomes, user preferences stated, and any facts the user asked to remember. Be concise — aim for 1/5 the original length. Start with '[Context Summary]'.";

/// Estimate token count for a string. Rough heuristic: mixed CJK+ASCII
/// averages ~2.5 chars per token. Intentionally conservative (over-counts)
/// so compaction triggers early rather than late.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 2.5).ceil() as usize
}

/// Estimate total tokens for a session + system prompt + tools.
pub fn estimate_session_tokens(
    system: &str,
    session: &Session,
    tools_json_approx: usize,
) -> usize {
    let mut total = estimate_tokens(system) + tools_json_approx;
    for msg in &session.messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => total += estimate_tokens(text),
                ContentBlock::Thinking { thinking, .. } => total += estimate_tokens(thinking),
                ContentBlock::ToolUse { input, .. } => {
                    total += estimate_tokens(&serde_json::to_string(input).unwrap_or_default());
                }
                ContentBlock::ToolResult { content, .. } => total += estimate_tokens(content),
            }
        }
    }
    total
}

/// Should we compact? Returns true when estimated usage exceeds
/// `model_limit * (1 - headroom)`.
pub fn should_compact(
    system: &str,
    session: &Session,
    tools_json_approx: usize,
    model_limit: usize,
    headroom: f64,
) -> bool {
    if session.messages.len() <= 8 {
        return false;
    }
    let threshold = (model_limit as f64 * (1.0 - headroom)) as usize;
    estimate_session_tokens(system, session, tools_json_approx) > threshold
}

/// Compact the session: summarise all messages except the most recent
/// `keep_recent_pairs * 2` messages. Replaces the old messages with a
/// single summary message at position 0.
///
/// Returns the number of messages that were compacted.
pub async fn compact_session(
    provider: &dyn LlmProvider,
    session: &mut Session,
    keep_recent_pairs: usize,
) -> crate::Result<usize> {
    let keep_count = keep_recent_pairs * 2;
    if session.messages.len() <= keep_count {
        return Ok(0);
    }

    let split_at = session.messages.len() - keep_count;
    let old_messages = &session.messages[..split_at];

    let transcript = format_for_summary(old_messages);
    let req = CompletionRequest {
        model: String::new(),
        system: Some(COMPACTION_SYSTEM.to_string()),
        messages: vec![Message::user_text(transcript)],
        tools: Vec::new(),
        max_tokens: 2048,
        temperature: Some(0.1),
        enable_caching: false,
    };

    let resp = provider.complete(req).await?;
    let summary_text = resp.text();

    let recent: Vec<Message> = session.messages[split_at..].to_vec();
    session.messages.clear();
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: summary_text,
        }],
    });
    session.messages.extend(recent);

    Ok(split_at)
}

fn format_for_summary(messages: &[Message]) -> String {
    let mut buf = String::new();
    for msg in messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    buf.push_str(&format!("[{role}] {text}\n"));
                }
                ContentBlock::ToolUse { name, .. } => {
                    buf.push_str(&format!("[{role} tool_use] {name}\n"));
                }
                ContentBlock::ToolResult { content, .. } => {
                    let preview: String = content.chars().take(200).collect();
                    buf.push_str(&format!("[{role} tool_result] {preview}\n"));
                }
                ContentBlock::Thinking { .. } => {}
            }
        }
    }
    buf
}
