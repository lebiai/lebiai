//! Build the reflection prompt: system instructions + user payload.

use hermes_core::{ContentBlock, Role, Session};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

const SYSTEM_PROMPT: &str = r###"You are the reflection module for lebi-AI, a local work-and-companion AI.
After each completed dialogue you receive the full transcript plus current
skills and memories. Extract only what will make the next similar work
better — not a transcript dump.

Identify three things, and only when each truly meets the bar:

1. SKILL CANDIDATES — reusable procedures with clear triggers and a
   self-contained markdown body. Only if the same procedure should apply
   next time. Skip one-shot exploration.

2. MEMORY CANDIDATES — durable knowledge that should persist. Prefer kinds:
   - preference (stable taste) → zone "preferences", tags include "preference"
   - standard (what "good" means for a kind of work) → zone "standards", tag "standard"
   - work-episode (a completed piece of work worth re-recognizing) → zone "work",
     tag "work-episode", fact body shaped as:
     【工作情节】<task one-liner>
     - 情境：…
     - 做法：…
     - 产出：…
     - 用户反馈/修正：…（无则写「无」）
     - 可复用点：…
   - other durable fact → zone "general"
   One claim (or one episode block) per memory. Default scope "user";
   "project" only for repo-specific facts.

3. CONFLICTS — when a candidate contradicts, duplicates, or subsumes an
   existing memory. kind "stale" when the old memory is wrong/outdated;
   always pair stale with a superseding memory_candidate.

Priority when the session did real work: work-episode and standards first,
then preferences, then skills. Skip trivia.

C-SESS (continuity): If any real work was completed, prefer one work-episode
with zone "work" and tag "work-episode", body shape 【工作情节】…
**CRITICAL — self-contained:** every field must be readable if the session
transcript is deleted. NEVER write "见会话记录" / "见该会话" / pointers only.
Put the actual intent, approach, and reusable point in the fact text itself.
If you cannot write a self-contained episode, omit it (empty is better than hollow).
summary must be a concrete one-liner of what was done (not "chatted").

CRITICAL — the user sees every candidate and must decide. Spam erodes trust.
Default to empty arrays. Prefer false negatives. confidence "high" is rare.

Reply with EXACTLY ONE JSON object. No prose. No markdown fences.

{
  "summary": "<one sentence: what work was done together>",
  "skill_candidates": [
    {
      "name": "kebab-case-name",
      "description": "one-line description for matcher",
      "triggers": ["keyword", "phrase"],
      "body": "## Title\n\nFull markdown instructions, multi-line.",
      "rationale": "why this is reusable enough to keep",
      "confidence": "low" | "medium" | "high"
    }
  ],
  "memory_candidates": [
    {
      "fact": "one statement OR work-episode block",
      "tags": ["preference"] ,
      "zone": "preferences" | "standards" | "work" | "general" | "core",
      "scope": "user" | "project",
      "confidence": "low" | "medium" | "high",
      "rationale": "why this should persist",
      "supersedes": ["mem_xxxx"]
    }
  ],
  "conflicts": [
    {
      "with": "mem_xxxx",
      "kind": "contradiction" | "redundancy" | "scope_overlap" | "stale",
      "explain": "what the disagreement is",
      "options": ["keep_old", "keep_new", "merge", "scope_split"]
    }
  ]
}
"###;

pub fn system_prompt() -> String {
    SYSTEM_PROMPT.to_string()
}

pub fn user_prompt(session: &Session, skills: &[LoadedSkill], memories: &[LoadedMemory]) -> String {
    let mut buf = String::new();

    // Feed back recent reflection outcomes so the LLM learns from accept/reject patterns.
    if let Ok(outcomes) = crate::log::recent_outcomes(10) {
        if !outcomes.is_empty() {
            buf.push_str("=== Recent reflection outcomes (learn from these) ===\n");
            for e in &outcomes {
                let action = match e.action {
                    crate::log::ActionTaken::Accept => "ACCEPTED",
                    crate::log::ActionTaken::Reject => "REJECTED",
                    crate::log::ActionTaken::Defer => "DEFERRED",
                    _ => "OTHER",
                };
                let kind = match e.kind {
                    crate::log::CandidateKind::Skill => "skill",
                    crate::log::CandidateKind::Memory => "memory",
                    crate::log::CandidateKind::ConflictMemory => "conflict-memory",
                    crate::log::CandidateKind::OrphanConflict => "orphan-conflict",
                };
                buf.push_str(&format!("- [{action}] {kind}: \"{}\"\n", e.label));
            }
            buf.push('\n');
        }
    }

    buf.push_str("=== Session transcript ===\n");
    if session.messages.is_empty() {
        buf.push_str("(empty)\n");
    } else {
        for msg in &session.messages {
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
                        // Only include assistant thinking if it's terse;
                        // the LLM rarely needs it for reflection. Truncate.
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

    buf.push_str("\n=== Current skills (active, name + description) ===\n");
    if skills.is_empty() {
        buf.push_str("(none)\n");
    } else {
        for s in skills {
            buf.push_str(&format!(
                "- {}: {}\n",
                s.frontmatter.name, s.frontmatter.description
            ));
        }
    }

    buf.push_str("\n=== Current active memories (id, scope, fact) ===\n");
    if memories.is_empty() {
        buf.push_str("(none)\n");
    } else {
        for m in memories {
            let body_preview = m.body.lines().next().unwrap_or("").trim();
            buf.push_str(&format!(
                "- {} [{:?}]: {}\n",
                m.frontmatter.id, m.scope, body_preview
            ));
        }
    }

    buf.push_str("\nNow produce the reflection JSON. Default to empty arrays.\n");
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
