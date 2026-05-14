//! Micro-reflection: lightweight per-turn reflection that runs in the
//! background after each agent loop completes.
//!
//! Unlike the full session-end reflection, micro-reflection only looks at
//! the most recent turn (user message + assistant response including tool
//! calls). It's cheaper (~500 tokens in, ~200 out) and runs async so it
//! never blocks the user's next input.

use hermes_core::{
    CompletionRequest, ContentBlock, LlmProvider, Message, Role,
};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

use crate::output::ReflectionOutput;
use crate::runner::ReflectError;

/// Heuristic: should we bother running micro-reflection on this turn?
pub fn should_micro_reflect(
    turn_messages: &[Message],
    turns_since_last_reflect: usize,
) -> bool {
    // Suppress if we just reflected recently.
    if turns_since_last_reflect < 3 {
        return false;
    }

    let mut tool_call_count = 0;
    let mut has_write_or_edit = false;
    let mut user_text = String::new();
    let mut output_chars = 0;

    for msg in turn_messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    if msg.role == Role::User {
                        user_text.push_str(text);
                    } else {
                        output_chars += text.len();
                    }
                }
                ContentBlock::ToolUse { name, .. } => {
                    tool_call_count += 1;
                    if matches!(name.as_str(), "write" | "edit") {
                        has_write_or_edit = true;
                    }
                }
                ContentBlock::ToolResult { .. } => {}
                ContentBlock::Thinking { .. } => {}
            }
        }
    }

    let user_lower = user_text.to_lowercase();
    let has_explicit_intent = user_lower.contains("记住")
        || user_lower.contains("以后")
        || user_lower.contains("偏好")
        || user_lower.contains("总是")
        || user_lower.contains("remember")
        || user_lower.contains("always")
        || user_lower.contains("prefer");

    // Decision matrix:
    if has_explicit_intent {
        return true;
    }
    if tool_call_count >= 2 {
        return true;
    }
    if has_write_or_edit && output_chars > 300 {
        return true;
    }
    if output_chars > 1500 {
        return true;
    }

    false
}

const MICRO_REFLECT_SYSTEM: &str = r##"You are a micro-reflection module. You just observed ONE turn of conversation (user request + assistant response). Decide if anything from this turn is worth persisting as a memory or skill.

Rules:
- Default to empty arrays. Most turns produce nothing.
- Only propose a memory if the user stated a durable preference, convention, or fact.
- Only propose a skill if the assistant followed a multi-step procedure that would be reusable verbatim next time.
- Never propose more than 1 memory and 1 skill per micro-reflection.
- Confidence should be "low" or "medium" — never "high" for micro-reflection (that's reserved for explicit user requests caught by full reflection).

Reply with EXACTLY ONE JSON object:
{
  "summary": "<one sentence>",
  "skill_candidates": [],
  "memory_candidates": [],
  "conflicts": []
}
"##;

/// Run a micro-reflection on the most recent turn. Much cheaper than full
/// session reflection — only sends the last turn's messages.
pub async fn micro_reflect(
    provider: &dyn LlmProvider,
    turn_messages: &[Message],
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
) -> Result<ReflectionOutput, ReflectError> {
    let user_prompt = build_micro_prompt(turn_messages, skills, memories);

    let req = CompletionRequest {
        model: String::new(),
        system: Some(MICRO_REFLECT_SYSTEM.to_string()),
        messages: vec![Message::user_text(user_prompt)],
        tools: Vec::new(),
        max_tokens: 1024,
        temperature: Some(0.1),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text();
    let json_str = crate::runner::strip_code_fence_pub(&text);

    serde_json::from_str(json_str).map_err(|e| ReflectError::ParseFailed {
        error: e.to_string(),
        raw: text,
    })
}

fn build_micro_prompt(
    turn_messages: &[Message],
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
) -> String {
    let mut buf = String::new();
    buf.push_str("=== This turn ===\n");
    for msg in turn_messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    let preview: String = text.chars().take(500).collect();
                    buf.push_str(&format!("[{role}] {preview}\n"));
                }
                ContentBlock::ToolUse { name, .. } => {
                    buf.push_str(&format!("[{role} tool_use] {name}\n"));
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let preview: String = content.chars().take(200).collect();
                    let tag = if *is_error { "tool_error" } else { "tool_result" };
                    buf.push_str(&format!("[{role} {tag}] {preview}\n"));
                }
                ContentBlock::Thinking { .. } => {}
            }
        }
    }

    if !memories.is_empty() {
        buf.push_str("\n=== Existing memories (for conflict check) ===\n");
        for m in memories.iter().take(20) {
            let line = m.body.lines().next().unwrap_or("").trim();
            buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, line));
        }
    }

    if !skills.is_empty() {
        buf.push_str("\n=== Existing skills (avoid duplicates) ===\n");
        for s in skills.iter().take(10) {
            buf.push_str(&format!("- {}: {}\n", s.frontmatter.name, s.frontmatter.description));
        }
    }

    buf
}
