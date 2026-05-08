//! Build the reflection prompt: system instructions + user payload.

use hermes_core::{ContentBlock, Role, Session};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

const SYSTEM_PROMPT: &str = r###"You are a reflection module for a self-evolving agent. After each completed
session you are given the full transcript plus the agent's current skill /
memory inventory. Your job is to identify three things, and only when each
case truly meets the bar:

1. SKILL CANDIDATES — reusable procedures the agent worked out, with clear
   triggers and a self-contained body of instructions in markdown. Only
   propose if you would genuinely want the same procedure applied next
   time the same situation appears. Skip anything that was a one-shot
   exploration.

2. MEMORY CANDIDATES — durable facts, conventions, preferences, or
   constraints the agent discovered that should persist across sessions.
   One claim per memory. Default `scope` to `user`; pick `project` only
   when the fact is specific to the current repo / codebase.

3. CONFLICTS — when a new memory candidate contradicts, duplicates, or
   subsumes an existing memory, report a conflict referencing the existing
   memory id and proposing resolution options.

CRITICAL — the agent's user sees every candidate and must decide. Spammy
proposals erode trust. Default to empty arrays. Prefer false negatives over
false positives. Confidence = "high" should be rare.

Reply with EXACTLY ONE JSON object matching this schema. No prose. No
markdown fences. No commentary.

{
  "summary": "<one sentence summarising what the session accomplished>",
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
      "fact": "one short statement; one fact per memory",
      "tags": ["rust", "convention"],
      "scope": "user" | "project",
      "confidence": "low" | "medium" | "high",
      "rationale": "why this should persist",
      "supersedes": ["mem_xxxx"]
    }
  ],
  "conflicts": [
    {
      "with": "mem_xxxx",
      "kind": "contradiction" | "redundancy" | "scope_overlap",
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
                        let tag = if *is_error { "tool_error" } else { "tool_result" };
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
