//! Per-turn system prompt assembly. 1:1 with `hermes-gui/src/context.rs`.

use hermes_core::ToolSpec;
use hermes_llm::ContextLimits;
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

pub struct ContextSources<'a> {
    pub base: Option<&'a str>,
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
    pub tools: &'a [ToolSpec],
    pub workspace_root: &'a str,
    pub limits: ContextLimits,
}

impl<'a> ContextSources<'a> {
    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = String::new();

        buf.push_str(&format!(
            "You are Hermes, a helpful AI assistant. Your workspace is `{}`.\n\n\
             You have tools available to you. When the user asks you to do something \
             that requires accessing the web, reading files, running commands, or any \
             other action — use your tools immediately. Do NOT say you cannot do something \
             if you have a tool that can do it. Do NOT output tool calls as text — use \
             the tool_use mechanism provided by the API.\n\n\
             When you decide to use a tool, just use it directly without asking for permission \
             unless the action is destructive.\n\n",
            self.workspace_root
        ));

        if let Some(b) = self.base {
            buf.push_str(b);
            buf.push_str("\n\n");
        }
        if !self.pinned.is_empty() {
            buf.push_str("## Pinned memories (always loaded)\n");
            for m in self.pinned {
                let body = m.body.trim();
                buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, body));
            }
            buf.push('\n');
        }
        let episodic: Vec<&LoadedMemory> = self
            .active
            .iter()
            .filter(|m| !m.frontmatter.pinned)
            .collect();
        if !episodic.is_empty() {
            buf.push_str("## Active memory index\n");
            for m in episodic.iter().take(self.limits.active_memory_index_cap) {
                let line = m.body.lines().next().unwrap_or("").trim();
                buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, line));
            }
            if episodic.len() > self.limits.active_memory_index_cap {
                buf.push_str(&format!(
                    "- ... ({} more not shown)\n",
                    episodic.len() - self.limits.active_memory_index_cap
                ));
            }
            buf.push('\n');
        }
        if !self.all_skills.is_empty() {
            buf.push_str("## Available skills\n");
            for s in self.all_skills.iter().take(self.limits.skill_index_cap) {
                buf.push_str(&format!(
                    "- {}: {}\n",
                    s.frontmatter.name, s.frontmatter.description
                ));
            }
            buf.push('\n');
        }
        let matched = hermes_skills::match_for_query(
            self.all_skills,
            user_query,
            self.limits.triggered_skill_cap,
        );
        if !matched.is_empty() {
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str("## Skills triggered for this turn\n\n");
            for s in matched {
                buf.push_str(&format!("### {}\n", s.frontmatter.name));
                buf.push_str(s.body.trim());
                buf.push_str("\n\n");
            }
        }
        buf.trim_end().to_string()
    }
}
