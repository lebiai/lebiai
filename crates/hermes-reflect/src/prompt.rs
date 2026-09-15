//! Build the reflection prompt: system instructions + user payload.

use hermes_core::{ContentBlock, Role, Session};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

// 「归属（owner）」那段（见下方）与 `micro.rs` 的 `MICRO_REFLECT_SYSTEM` 里那条
// `Owner:` 是**同一条纪律的两个语言版本**（这里是中文、那份是英文）。
// 两条提示词各自维护语言，**不合并**；但改一处必须同步另一处。
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

归属（owner）：
- 这条是在说**用户本人**（偏好、标准、交付习惯）→ 不写 owner，落全局。所有工位都要遵守。
- 这条是**这份活的专业口径**（本行业的判断规则、数据口径、措辞纪律）→ owner 写当前工位的 id。
- 拿不准就当成用户本人处理：宁可不隔离，不误隔离。

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
      "owner": "<当前工位的 id；省略 = 全局>",
      "tags": ["preference"] ,
      "zone": "preferences" | "standards" | "work" | "general",
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

    // 当前工位 id 必须写进正文：模型无从猜出 id，猜不中就等于「专业口径」永远落全局。
    // 「会话 → 归属」的转换只有 `hermes_core::persona::memory_owner_for` 一处。
    match hermes_core::persona::memory_owner_for(session.meta.persona.as_deref()) {
        Some(owner) => buf.push_str(&format!(
            "=== 当前工位 ===\n\
             {owner} —— 关于这份活的专业口径的候选，`owner` 写 \"{owner}\"；\
             关于用户本人的，不写 `owner`。\n\n"
        )),
        None => buf.push_str(
            "=== 当前工位 ===\n\
             （无 —— 一切落全局：候选一律不写 `owner`）\n\n",
        ),
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
    buf.push_str(
        "A slot is a KIND of work, not a single fact. Entries sharing a slot are usually\
complementary sides of it — keep them all. Only supersede an id when this turn genuinely\
replaces that entry (same rule, corrected or merged), never merely because the slot matches.\
",
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_persona(persona: Option<&str>) -> Session {
        let mut meta = hermes_core::SessionMeta::new("test-model", "test-provider");
        meta.persona = persona.map(str::to_string);
        Session {
            meta,
            messages: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    #[test]
    fn user_prompt_names_the_current_workspace_only_when_there_is_one() {
        let with = user_prompt(&session_with_persona(Some("xiao-xie")), &[], &[]);
        assert!(
            with.contains("xiao-xie"),
            "带工位时提示词必须出现该 id：\n{with}"
        );
        let without = user_prompt(&session_with_persona(None), &[], &[]);
        assert!(
            !without.contains("xiao-xie"),
            "无工位时不许提 id：\n{without}"
        );
    }

    /// 自带角色（李现 / 小文）的会话算「没有工位」——不搞专业分工，
    /// 转换点仍是 `memory_owner_for` 那一处。
    #[test]
    fn a_builtin_persona_session_reads_as_no_workspace() {
        let p = user_prompt(&session_with_persona(Some("li-xian")), &[], &[]);
        assert!(!p.contains("li-xian"), "自带角色的会话不该出现工位：\n{p}");
    }
}
