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

const COMPILE_PALACE_SYSTEM: &str = r##"You are a memory curator organizing a Memory Palace index. Given memories grouped by zone, produce a concise zone map.

Rules:
- Use ## Memory Palace as the top header
- Show total memory count and zone count in the first line
- For each zone, use ### zone_name (count) as header
- Under each zone, list 2-3 bullet points summarizing key content
- Keep the entire output under 200 tokens
- Preserve the user's language (Chinese / English as found in entries)
- Output ONLY the index markdown, no preamble
"##;

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

/// LLM-compiled palace index: richer than the simple code-generated version.
pub async fn compile_palace_index(
    provider: &dyn LlmProvider,
    memories: &[LoadedMemory],
) -> Result<String, ReflectError> {
    let grouped = hermes_memory::group_by_zone(memories);
    let mut user_prompt = String::from("Organize these memories into a palace index:\n\n");
    for (zone, mems) in &grouped {
        user_prompt.push_str(&format!("### {} ({} memories)\n", zone, mems.len()));
        for m in mems {
            user_prompt.push_str(&format!("- {}\n", m.body.trim()));
        }
        user_prompt.push('\n');
    }

    let req = CompletionRequest {
        model: String::new(),
        system: Some(COMPILE_PALACE_SYSTEM.to_string()),
        messages: vec![Message::user_text(user_prompt)],
        tools: Vec::new(),
        max_tokens: 1024,
        temperature: Some(0.2),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text().trim().to_string();
    if text.is_empty() {
        return Err(ReflectError::Provider("empty palace index from LLM".into()));
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
}
