//! Focused reflection: distill recent turns into at most ONE skill candidate.
//!
//! Unlike `runner::reflect` (which produces skills + memories + conflicts over
//! a full session), this variant is invoked on demand — typically by the
//! `propose_skill` tool when the user explicitly asks the agent to save a
//! workflow as a reusable skill.
//!
//! Design notes:
//! - Skills only; memory_candidates / conflicts are forced empty in the prompt.
//! - The hint (a one-liner from the user/agent) is injected at the top so the
//!   LLM knows what to focus on.
//! - Takes a slice of recent messages, not a full Session, so callers can
//!   trim to "last N turns" before invoking.

use hermes_core::{CompletionRequest, ContentBlock, LlmProvider, Message, Role};

use crate::output::SkillCandidate;
use crate::runner::{repair_truncated_json, strip_code_fence_pub, ReflectError, Result};

const FOCUSED_MAX_TOKENS: u32 = 4096;

#[derive(serde::Deserialize)]
struct FocusedOutput {
    #[serde(default)]
    skill_candidates: Vec<SkillCandidate>,
}

/// Run a focused reflection on `recent_messages`. Returns 0-1 skill candidates.
pub async fn reflect_focused(
    provider: &dyn LlmProvider,
    recent_messages: &[Message],
    hint: Option<&str>,
) -> Result<Option<SkillCandidate>> {
    let system = focused_system_prompt();
    let user = focused_user_prompt(recent_messages, hint);

    let req = CompletionRequest {
        model: String::new(),
        system: Some(system),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: FOCUSED_MAX_TOKENS,
        temperature: Some(0.2),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text();
    let json_str = strip_code_fence_pub(&text);

    let out: FocusedOutput = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(first_err) => {
            if let Some(repaired) = repair_truncated_json(json_str) {
                if let Ok(v) = serde_json::from_str(&repaired) {
                    tracing::info!("recovered truncated focused-reflection JSON");
                    v
                } else {
                    return Err(ReflectError::ParseFailed {
                        error: first_err.to_string(),
                        raw: text,
                    });
                }
            } else {
                return Err(ReflectError::ParseFailed {
                    error: first_err.to_string(),
                    raw: text,
                });
            }
        }
    };

    // Take the first candidate; LLM is instructed to produce at most one.
    Ok(out.skill_candidates.into_iter().next())
}

fn focused_system_prompt() -> String {
    r###"You are a focused reflection module for a self-evolving agent. The user
or agent has explicitly identified the recent conversation turns as containing
a reusable workflow worth distilling into a skill.

Your job: produce EXACTLY ZERO OR ONE skill candidate.

Be more generous than usual — the request itself is a strong signal that a
skill is warranted. Only return zero if the recent turns truly contain no
multi-step procedure at all. Otherwise draft your best attempt; the user
will approve, edit, or reject.

Guidance for the skill body:
- Distill the SUCCESSFUL path. If the agent tried something that failed and
  then found something that worked, write the body around the working path
  and (optionally) call out the dead-end as a "skip X, do Y" note.
- Triggers should be specific keywords the user is likely to use again
  (e.g. "weather", "code review", "tech news") — avoid generic words like
  "help" or "do".
- name is kebab-case and unique.

Reply with EXACTLY ONE JSON object. No prose. No markdown fences.

{
  "skill_candidates": [
    {
      "name": "kebab-case-name",
      "description": "one-line description for matcher",
      "triggers": ["keyword", "phrase"],
      "body": "## Title\n\nFull markdown instructions, multi-line.",
      "rationale": "why this is reusable enough to keep",
      "confidence": "low" | "medium" | "high"
    }
  ]
}

If truly no skill applies, return: {"skill_candidates": []}
"###
    .to_string()
}

fn focused_user_prompt(recent: &[Message], hint: Option<&str>) -> String {
    let mut buf = String::new();

    if let Some(h) = hint {
        if !h.trim().is_empty() {
            buf.push_str("=== Hint from user/agent ===\n");
            buf.push_str(h.trim());
            buf.push_str("\n\n");
        }
    }

    buf.push_str("=== Recent turns to distill ===\n");
    if recent.is_empty() {
        buf.push_str("(no recent turns)\n");
    } else {
        for msg in recent {
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
                        buf.push_str(&format!("[{role}] {text}\n"));
                    }
                    ContentBlock::Thinking { thinking, .. } => {
                        let preview: String = thinking.chars().take(200).collect();
                        buf.push_str(&format!("[{role} thinking] {preview}\n"));
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        let args = serde_json::to_string(input).unwrap_or_default();
                        let args = truncate(&args, 200);
                        buf.push_str(&format!("[{role} tool_use] {name}({args})\n"));
                    }
                    ContentBlock::ToolResult {
                        content, is_error, ..
                    } => {
                        let preview = truncate(content, 400);
                        let tag = if *is_error {
                            "tool_error"
                        } else {
                            "tool_result"
                        };
                        buf.push_str(&format!("[{role} {tag}] {preview}\n"));
                    }
                }
            }
        }
    }

    buf.push_str("\nNow produce the skill candidate JSON (zero or one).\n");
    buf
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}
