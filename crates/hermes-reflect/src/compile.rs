//! Compile all active memories into a structured profile document via LLM.

use hermes_core::{CompletionRequest, LlmProvider, Message};
use hermes_memory::LoadedMemory;

use crate::runner::ReflectError;

const COMPILE_SYSTEM: &str = r##"You are a memory curator. Given a list of individual memory entries about a user accumulated over multiple conversations, compile them into a structured profile document.

Rules:
- Use ## markdown headers to organize by topic (categories emerge naturally from the content)
- Merge overlapping or redundant memories into single concise entries
- Use bullet points, one line per point
- Preserve the user's language (Chinese / English as found in entries)
- Drop entries that are trivially obvious or redundant after merging
- Output ONLY the profile markdown, no preamble or explanation
"##;

/// 编译 `profile.md` 前该喂进去的那一份记忆：**只留全局可见的**。
///
/// `profile.md` 是**单个全局文件**，却被**每一个视图无条件注入系统提示词**
/// （`chat/mod.rs` 每轮都注；micro 接收后重编译写的也是这同一份）→ 它里面
/// **只能有所有视图都看得见的东西**。「全局可见」正好就是
/// [`hermes_memory::visible_owned`]`(active, None)`：`visible_to` 那唯一一处判定
/// 已经把它定义好了，本函数不新增任何判定点。
///
/// 这是第三条泄漏面（Task 1.10e）的收口：此前三个编译点都喂全量，人物私有口径
/// （`owner: xiao-xie`）于是被端进了不带人物的会话的系统提示词。人物自己的专业
/// 口径**不消失**——它仍在记忆索引与主题卡（已按视图合并）里，只是不进这份全局摘要。
/// 口径见 `docs/spec/personas.md` §5.2「注入：人物会话只给全局 + 本人物」。
///
/// 按 owner 分区的 profile（人物专业口径进它自己的 profile）是阶段 3+ 的候选；
/// 在那之前，`profile.md` 的正确语义就是**全局口径摘要**。
pub fn profile_input(active: &[LoadedMemory]) -> Vec<LoadedMemory> {
    hermes_memory::visible_owned(active, None)
}

/// Compile all active memories into a structured markdown profile.
pub async fn compile_profile(
    provider: &dyn LlmProvider,
    memories: &[LoadedMemory],
) -> Result<String, ReflectError> {
    let user_prompt = build_compile_prompt(memories);

    let req = CompletionRequest {
        model: String::new(),
        system: Some(COMPILE_SYSTEM.to_string()),
        messages: vec![Message::user_text(user_prompt)],
        tools: Vec::new(),
        max_tokens: 4096,
        temperature: Some(0.2),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text().trim().to_string();
    if text.is_empty() {
        return Err(ReflectError::Provider("empty response from LLM".into()));
    }
    Ok(text)
}

fn build_compile_prompt(memories: &[LoadedMemory]) -> String {
    let mut buf =
        String::from("Compile the following memory entries into a structured profile:\n\n");
    for m in memories {
        let pin = if m.frontmatter.pinned { "pinned, " } else { "" };
        let conf = format!("{:?}", m.frontmatter.confidence).to_lowercase();
        let body = m.body.trim();
        buf.push_str(&format!("- [{}] ({pin}{conf}) {body}\n", m.frontmatter.id));
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, pinned: bool, body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::User,
            Confidence::High,
            vec![],
            "general".to_string(),
        );
        fm.id = id.to_string();
        fm.pinned = pinned;
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    fn mem_owned(id: &str, owner: &str, body: &str) -> LoadedMemory {
        let mut m = mem(id, false, body);
        m.frontmatter.owner = Some(owner.to_string());
        m
    }

    #[test]
    fn prompt_includes_all_memories() {
        let mems = vec![
            mem("mem_a", true, "architect on Mac"),
            mem("mem_b", false, "prefers vim"),
        ];
        let prompt = build_compile_prompt(&mems);
        assert!(prompt.contains("[mem_a] (pinned, high) architect on Mac"));
        assert!(prompt.contains("[mem_b] (high) prefers vim"));
    }

    #[test]
    fn profile_input_keeps_only_globally_visible() {
        let mems = vec![
            mem("mem_global", false, "用户偏好简洁的周报"),
            mem_owned("mem_xiao_xie", "xiao-xie", "林碳项目的专业口径"),
        ];
        let kept = profile_input(&mems);
        let ids: Vec<&str> = kept.iter().map(|m| m.frontmatter.id.as_str()).collect();
        assert_eq!(ids, vec!["mem_global"]);
    }
}
