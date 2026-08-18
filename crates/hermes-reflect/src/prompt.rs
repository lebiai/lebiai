//! Build the reflection prompt: system instructions + user payload.

use hermes_core::{ContentBlock, Role, Session};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

const SYSTEM_PROMPT: &str = r###"You are the living-rule distiller for lebi-AI (a work companion).
You do NOT write a diary of what happened. You maintain **one active rule
per kind of work** so the next similar job can be done their way without
them repeating themselves.

A memory is valuable only if: next time they do this kind of work, following
the rule still makes the work better — even if they say nothing extra.

NEVER persist:
- session recap / 流水账 / "用户说了XXX" restated as an episode
- today's mood ("不想上班") or in-progress status
- tool/environment facts (python-docx, sandbox, which binary exists)
- empty shells, "见会话记录", copying the user utterance into 情境/做法/可复用点
- one-off topic choices (this article's title, this company's name)

ALWAYS prefer empty arrays over a weak candidate.

How to extract (all kinds of work — writing, planning, lookup, review):
1. Look at the **delta**: first deliverable vs what they rejected / insisted /
   finally accepted. The stable part of that delta is the rule.
2. Ask: "If they never say this again, should we still do it?" If no → omit.
3. Map to ONE slot and ONE fact:
   - write-deliverable (how a finished piece should read/look)
   - lookup-facts (which sources actually worked — full URLs only if they paid off)
   - close-out (how they take delivery: in-chat / Word / Desktop)
   - tone, identity, work-method, prioritize — only if durable
4. If an **existing memory** is the same slot (same kind of work): do NOT add a
   second peer. Write ONE replacement fact that merges still-true old points
   with this turn's correction, and set `supersedes` to the old id(s).
   If this turn is a one-off exception ("这篇写长一点") → omit (do not store).
5. CONFLICTS: kind "stale" when replacing; pair with the superseding candidate.

Skills: only a reusable procedure they would want run again the same way.
Not a recap of this session.

summary: one sentence of the work done (for logs), not a memory.

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
            if msg.is_internal_instruction_only() {
                continue;
            }
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

    buf.push_str("\n=== Current living memories (id, slot, fact) ===\n");
    buf.push_str("Same slot → you MUST supersede these ids, not add a peer.\n");
    let living = hermes_memory::living_rules(memories.to_vec());
    if living.is_empty() {
        buf.push_str("(none)\n");
    } else {
        for m in &living {
            let slot = hermes_memory::infer_slot(&m.frontmatter.zone, &m.frontmatter.tags, &m.body);
            let slot_s = slot.map(|s| s.as_str()).unwrap_or("unslotted");
            let body_preview = m.body.lines().next().unwrap_or("").trim();
            buf.push_str(&format!(
                "- {} [slot={slot_s}]: {}\n",
                m.frontmatter.id, body_preview
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
