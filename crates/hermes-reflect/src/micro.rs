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

// `- Owner:` 那条与 `prompt.rs` 的「归属（owner）」段是**同一条纪律的两个语言版本**
// （这里是英文、那份是中文）。两条提示词各自维护语言，**不合并**；
// 但改一处必须同步另一处。
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
- Owner: a rule about **the user themselves** (preferences, standards, how they take delivery) omits `owner` — it must hold in every workspace. A rule about **this kind of work** (domain judgement, data sourcing, wording discipline) sets `owner` to the current workspace id given in the user message. When unsure, treat it as about the user: better left global than locked into one workspace.

Reply with EXACTLY ONE JSON object:
{
  "summary": "<one sentence>",
  "skill_candidates": [],
  "memory_candidates": [{"fact": "<short statement>", "owner": "<current workspace id, omit for a global rule>", "tags": [], "scope": "user", "confidence": "low|medium", "rationale": "<why>", "supersedes": ["mem_xxx"]}],
  "conflicts": [{"with": "mem_xxx", "kind": "stale", "explain": "<why old memory is wrong>", "options": ["keep_new", "keep_old"]}]
}
"##;

/// Run a micro-reflection on the most recent turn. Much cheaper than full
/// session reflection — only sends the last turn's messages.
///
/// `owner` is the current workspace's memory owner (`None` = no workspace /
/// builtin persona): the prompt names it so the model can tag this kind of
/// work's rules with it, and never has to guess an id.
pub async fn micro_reflect(
    provider: &dyn LlmProvider,
    turn_messages: &[Message],
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
    owner: Option<&str>,
) -> Result<ReflectionOutput, ReflectError> {
    let user_prompt = build_micro_prompt(turn_messages, skills, memories, owner);

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
        Ok(out) => Ok(crate::episode::finalize_reflection_output_with(
            out, memories,
        )),
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
    owner: Option<&str>,
) -> String {
    let mut buf = String::new();
    match owner {
        Some(owner) => buf.push_str(&format!(
            "=== Current workspace: {owner} ===\n\
             A rule about this kind of work sets `owner` to \"{owner}\". \
             A rule about the user themselves omits `owner`.\n\n"
        )),
        None => buf.push_str(
            "=== Current workspace: none ===\n\
             No workspace is active, so omit `owner` on every candidate — \
             every memory stays global.\n\n",
        ),
    }
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

    /// 工位 id 必须进正文：模型无从猜出 id，猜不中 = 专业口径永远落全局。
    #[test]
    fn prompt_names_the_current_workspace_only_when_there_is_one() {
        let msgs = [user_msg("写一篇林碳的稿子")];
        let with = build_micro_prompt(&msgs, &[], &[], Some("xiao-xie"));
        assert!(
            with.contains("xiao-xie"),
            "带工位时提示词必须出现该 id：\n{with}"
        );
        let without = build_micro_prompt(&msgs, &[], &[], None);
        assert!(
            !without.contains("xiao-xie") && without.contains("none"),
            "无工位时不许提 id，且要说明一切落全局：\n{without}"
        );
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

    /// 抓下 `micro_reflect` 真正发出的 system + user，用来钉 owner 有没有进正文。
    #[derive(Default)]
    struct CapturingProvider {
        seen: std::sync::Mutex<Vec<(String, String)>>,
    }

    impl LlmProvider for CapturingProvider {
        // 手写 `#[async_trait]` 展开后的签名：本 crate 的 dev-dependencies 里
        // 没有 async-trait（也不想为一条测试加）。
        fn complete<'life0, 'async_trait>(
            &'life0 self,
            req: CompletionRequest,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = hermes_core::Result<hermes_core::CompletionResponse>,
                    > + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                let system = req.system.clone().unwrap_or_default();
                let user = req
                    .messages
                    .iter()
                    .flat_map(|m| m.content.iter())
                    .filter_map(|b| b.as_text())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.seen.lock().unwrap().push((system, user));
                Ok(hermes_core::CompletionResponse {
                    content: vec![ContentBlock::Text { text: "{}".into() }],
                    stop_reason: hermes_core::StopReason::EndTurn,
                    usage: hermes_core::Usage::default(),
                    truncated_tool_ids: vec![],
                })
            })
        }

        fn capabilities(&self) -> hermes_core::Capabilities {
            hermes_core::Capabilities {
                tool_use: false,
                prompt_caching: false,
                streaming: false,
            }
        }

        fn name(&self) -> &str {
            "capturing"
        }
    }

    /// 没有 tokio / futures：假 provider 立刻 Ready，用 std 的安全 waker
    /// （`unsafe_code = "forbid"`，手搓 RawWaker 不行）转一圈就够了。
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        struct Noop;
        impl std::task::Wake for Noop {
            fn wake(self: std::sync::Arc<Self>) {}
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(Noop));
        let mut cx = std::task::Context::from_waker(&waker);
        let mut fut = Box::pin(fut);
        loop {
            match fut.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(v) => return v,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn last_system_and_user(p: &CapturingProvider) -> (String, String) {
        p.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("provider must have been called once")
    }

    /// `micro_reflect` 必须把 owner 转给提示词——这是「工位 id 进提示词」的唯一通路，
    /// 只测 `build_micro_prompt` 抓不住「这里被硬编成 None」。
    #[test]
    fn micro_reflect_forwards_the_workspace_into_the_prompt() {
        let msgs = [user_msg("写一篇林碳的稿子")];

        let p = CapturingProvider::default();
        block_on(micro_reflect(&p, &msgs, &[], &[], Some("xiao-xie"))).unwrap();
        let (system, user) = last_system_and_user(&p);
        assert!(
            system.contains("Owner:"),
            "系统提示词必须讲归属：\n{system}"
        );
        assert!(
            user.contains("Current workspace: xiao-xie"),
            "带工位时正文必须写出该 id：\n{user}"
        );

        let q = CapturingProvider::default();
        block_on(micro_reflect(&q, &msgs, &[], &[], None)).unwrap();
        let (_, user) = last_system_and_user(&q);
        assert!(
            user.contains("Current workspace: none") && !user.contains("xiao-xie"),
            "无工位时不许提 id，且要说明一切落全局：\n{user}"
        );
    }
}
