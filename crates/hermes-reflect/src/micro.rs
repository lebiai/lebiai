//! Micro-reflection: lightweight per-turn reflection that runs in the
//! background after each agent loop completes.
//!
//! Unlike the full session-end reflection, micro-reflection only looks at
//! the most recent turn (user message + assistant response including tool
//! calls). It's cheaper (~500 tokens in, ~200 out) and runs async so it
//! never blocks the user's next input.

use hermes_core::{CompletionRequest, ContentBlock, LlmProvider, Message, Role};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

use crate::output::ReflectionOutput;
use crate::runner::ReflectError;

const REFLECT_INTERVAL: usize = 3;

/// Should we run micro-reflection on this turn?
///
/// Two triggers:
/// 1. **Explicit intent** — user says something that sounds like a
///    correction or teaching moment. Bypasses cooldown entirely.
/// 2. **Periodic** — every `REFLECT_INTERVAL` turns, let the LLM decide
///    whether the turn was worth remembering. Cheap (~500 tokens) and the
///    LLM returns empty arrays for trivial turns.
pub fn should_micro_reflect(turn_messages: &[Message], turns_since_last_reflect: usize) -> bool {
    if has_explicit_intent(turn_messages) {
        return true;
    }
    turns_since_last_reflect >= REFLECT_INTERVAL
}

/// True when the user's turn explicitly teaches the agent something —
/// a stated preference, convention, or a correction. Such turns should
/// persist their memory candidate regardless of the confidence floor: the
/// user literally asked to be remembered, so requiring extra confidence (or
/// manual confirmation) would feel broken.
pub fn has_explicit_intent(turn_messages: &[Message]) -> bool {
    let mut user_text = String::new();
    for msg in turn_messages {
        if msg.role != Role::User {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                user_text.push_str(text);
            }
        }
    }
    let lower = user_text.to_lowercase();
    lower.contains("记住")
        || lower.contains("以后")
        || lower.contains("偏好")
        || lower.contains("总是")
        || lower.contains("remember")
        || lower.contains("always")
        || lower.contains("prefer")
        || lower.contains("不是")
        || lower.contains("不对")
        || lower.contains("错了")
        || lower.contains("don't")
        || lower.contains("wrong")
        || lower.contains("actually")
        || lower.contains("no,")
}

const MICRO_REFLECT_SYSTEM: &str = r##"You are a micro-reflection module. You just observed ONE turn of conversation (user request + assistant response). Decide if anything from this turn is worth persisting as a memory or skill, and whether any existing memory is now stale.

Rules:
- Default to empty arrays. Most turns produce nothing.
- Only propose a memory if it is a living rule that will still hold next time (how they work — not what happened this turn).
- If it updates an existing memory of the same kind of work, set supersedes to that id. Never add a second peer rule for the same kind of work.
- Never persist today's mood, tool/environment facts, or a recap of the task.
- Only propose a skill if the assistant followed a multi-step procedure that would be reusable verbatim next time.
- Never propose more than 1 memory and 1 skill per micro-reflection.
- Confidence should be "low" or "medium" — never "high" for micro-reflection (that's reserved for explicit user requests caught by full reflection).
- If the conversation reveals that an existing memory is WRONG or OUTDATED, produce a memory_candidates entry with the corrected fact and set `supersedes` to the old memory's id, plus a conflicts entry with kind "stale" explaining why the old memory is no longer accurate.

Reply with EXACTLY ONE JSON object:
{
  "summary": "<one sentence>",
  "skill_candidates": [],
  "memory_candidates": [{"fact": "<short statement>", "tags": [], "scope": "user", "confidence": "low|medium", "rationale": "<why>", "supersedes": ["mem_xxx"]}],
  "conflicts": [{"with": "mem_xxx", "kind": "stale", "explain": "<why old memory is wrong>", "options": ["keep_new", "keep_old"]}]
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
        max_tokens: 2048,
        temperature: Some(0.1),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text();
    let json_str = crate::runner::strip_code_fence_pub(&text);

    match serde_json::from_str(json_str) {
        Ok(out) => Ok(crate::episode::finalize_reflection_output_with(out, memories)),
        Err(first_err) => {
            if let Some(repaired) = crate::runner::repair_truncated_json(json_str) {
                if let Ok(out) = serde_json::from_str(&repaired) {
                    tracing::info!("recovered truncated micro-reflection JSON");
                    return Ok(crate::episode::finalize_reflection_output_with(
                        out, memories,
                    ));
                }
            }
            Err(ReflectError::ParseFailed {
                error: first_err.to_string(),
                raw: text,
            })
        }
    }
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
                ContentBlock::Image { source } => {
                    buf.push_str(&format!("[{role} image: {}]\n", source.media_type));
                }
                ContentBlock::Text { text } => {
                    let preview: String = text.chars().take(500).collect();
                    buf.push_str(&format!("[{role}] {preview}\n"));
                }
                ContentBlock::ToolUse { name, .. } => {
                    buf.push_str(&format!("[{role} tool_use] {name}\n"));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let preview: String = content.chars().take(200).collect();
                    let tag = if *is_error {
                        "tool_error"
                    } else {
                        "tool_result"
                    };
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
            buf.push_str(&format!(
                "- {}: {}\n",
                s.frontmatter.name, s.frontmatter.description
            ));
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::{ContentBlock, Role};

    fn user_msg(text: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            at: None,
        }
    }

    fn assistant_msg(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            at: None,
        }
    }

    #[test]
    fn explicit_intent_bypasses_cooldown() {
        let msgs = [user_msg("remember this")];
        assert!(should_micro_reflect(&msgs, 0));
    }

    #[test]
    fn explicit_intent_chinese_correction() {
        let msgs = [user_msg("不对，我不用 VSCode")];
        assert!(should_micro_reflect(&msgs, 0));
    }

    #[test]
    fn explicit_intent_english_correction() {
        let msgs = [user_msg("that's wrong, actually I prefer vim")];
        assert!(should_micro_reflect(&msgs, 1));
    }

    #[test]
    fn periodic_triggers_at_interval() {
        let msgs = [user_msg("hello"), assistant_msg("hi there")];
        assert!(!should_micro_reflect(&msgs, 2));
        assert!(should_micro_reflect(&msgs, 3));
        assert!(should_micro_reflect(&msgs, 10));
    }

    #[test]
    fn trivial_turn_skipped_within_cooldown() {
        let msgs = [user_msg("hello"), assistant_msg("hi")];
        assert!(!should_micro_reflect(&msgs, 0));
    }

    #[test]
    fn parse_output_with_conflict_and_supersedes() {
        let json = r#"{
            "summary": "user corrected a preference",
            "skill_candidates": [],
            "memory_candidates": [{
                "fact": "user prefers vim over VSCode",
                "tags": ["editor", "preference"],
                "scope": "user",
                "confidence": "medium",
                "rationale": "user explicitly corrected",
                "supersedes": ["mem_editor_pref"]
            }],
            "conflicts": [{
                "with": "mem_editor_pref",
                "kind": "stale",
                "explain": "user said they don't use VSCode",
                "options": ["keep_new", "keep_old"]
            }]
        }"#;
        let output: ReflectionOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.memory_candidates.len(), 1);
        assert_eq!(
            output.memory_candidates[0].supersedes,
            vec!["mem_editor_pref"]
        );
        assert_eq!(output.conflicts.len(), 1);
        assert_eq!(output.conflicts[0].kind, "stale");
        assert_eq!(output.conflicts[0].with, "mem_editor_pref");
    }
}
