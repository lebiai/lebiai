//! Context compaction: summarise old messages when approaching the model's
//! context limit, keeping recent turns verbatim.

use crate::{CompletionRequest, ContentBlock, LlmProvider, Message, Role, Session};

const COMPACTION_SYSTEM: &str = "Summarize the following conversation preserving: key decisions made, tool results and their outcomes, user preferences stated, and any facts the user asked to remember. Be concise — aim for 1/5 the original length. Start with '[Context Summary]'.";

/// Estimate token count for a string.
///
/// We split characters into two buckets:
/// - **CJK** (Han, Hiragana, Katakana, Hangul, full-width punctuation): one
///   character is roughly one token in modern tokenisers.
/// - **Everything else** (ASCII, Latin scripts, code): roughly 4 chars/token.
///
/// The estimate is intentionally conservative on the CJK side (1 char/token
/// rather than the empirical 0.7–0.9) and uses 4 chars/token for ASCII (the
/// classic OpenAI rule of thumb). This stays within ~15% of cl100k_base /
/// claude tokenizers on mixed prose, and over-counts mildly on pure code —
/// which is the right side to err on for compaction triggering.
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk: usize = 0;
    let mut other: usize = 0;
    for c in text.chars() {
        if is_cjk(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + (other as f64 / 4.0).ceil() as usize
}

/// True for code points that tokenise at roughly one-token-per-char.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F |   // CJK symbols & punctuation
        0x3040..=0x309F |   // Hiragana
        0x30A0..=0x30FF |   // Katakana
        0x3400..=0x4DBF |   // CJK Unified Ideographs Extension A
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0xAC00..=0xD7AF |   // Hangul Syllables
        0xF900..=0xFAFF |   // CJK Compatibility Ideographs
        0xFF00..=0xFFEF |   // Halfwidth and Fullwidth Forms
        0x20000..=0x2A6DF | // CJK Extension B
        0x2A700..=0x2B73F | // CJK Extension C
        0x2B740..=0x2B81F   // CJK Extension D
    )
}

/// Estimate total tokens for a session + system prompt + tools.
pub fn estimate_session_tokens(system: &str, session: &Session, tools_json_approx: usize) -> usize {
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
                // Base64 image data is sent to the provider; count it so
                // compaction triggers correctly for image-heavy turns.
                ContentBlock::Image { source } => total += estimate_tokens(&source.data),
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
        content: vec![ContentBlock::Text { text: summary_text }],
        at: None,
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
                    let preview = preview_with_head_tail(content, 400, 200);
                    buf.push_str(&format!("[{role} tool_result] {preview}\n"));
                }
                ContentBlock::Thinking { .. } => {}
                ContentBlock::Image { source } => {
                    buf.push_str(&format!("[{role} image: {}]\n", source.media_type));
                }
            }
        }
    }
    buf
}

/// Preview a long string by keeping its first `head_chars` and last
/// `tail_chars`, with a marker in between. Short strings are returned as-is.
fn preview_with_head_tail(s: &str, head_chars: usize, tail_chars: usize) -> String {
    let total: Vec<char> = s.chars().collect();
    if total.len() <= head_chars + tail_chars {
        return s.to_string();
    }
    let head: String = total.iter().take(head_chars).collect();
    let tail: String = total.iter().skip(total.len() - tail_chars).collect();
    let elided = total.len() - head_chars - tail_chars;
    format!("{head}\n…[{elided} chars elided]…\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_ascii() {
        // "hello world" = 11 chars / 4 = 3 tokens.
        assert_eq!(estimate_tokens("hello world"), 3);
    }

    #[test]
    fn token_estimate_cjk() {
        // 5 Han chars → 5 tokens.
        assert_eq!(estimate_tokens("你好世界呀"), 5);
    }

    #[test]
    fn token_estimate_mixed() {
        // "你好 world" = 2 CJK + 6 ASCII ("好 world" includes space)
        // CJK: 2 tokens, ASCII: ceil(6/4)=2, total 4.
        // Actually "你好 world" = chars: '你','好',' ','w','o','r','l','d'
        // 2 CJK + 6 non-CJK = 2 + ceil(6/4)=2 = 4.
        assert_eq!(estimate_tokens("你好 world"), 4);
    }

    #[test]
    fn token_estimate_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn preview_keeps_short_strings_intact() {
        let s = "short string";
        assert_eq!(preview_with_head_tail(s, 400, 200), s);
    }

    #[test]
    fn preview_elides_long_strings() {
        let s = "a".repeat(1000);
        let p = preview_with_head_tail(&s, 400, 200);
        assert!(p.contains("[400 chars elided]"));
        assert!(p.starts_with(&"a".repeat(400)));
        assert!(p.ends_with(&"a".repeat(200)));
    }
}
