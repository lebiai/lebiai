//! Per-turn context assembly.
//!
//! Duplicated from `hermes-cli::commands::context` to avoid coupling the
//! TUI crate to the CLI crate. If the two drift further apart, extract a
//! shared `hermes-context` crate.

use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

const ACTIVE_MEMORY_INDEX_CAP: usize = 50;
const SKILL_INDEX_CAP: usize = 50;
const TRIGGERED_SKILL_CAP: usize = 3;

pub struct ContextSources<'a> {
    pub base: Option<&'a str>,
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
}

impl<'a> ContextSources<'a> {
    pub fn build_session_system(&self) -> String {
        let mut buf = String::new();
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
            for m in episodic.iter().take(ACTIVE_MEMORY_INDEX_CAP) {
                let line = m.body.lines().next().unwrap_or("").trim();
                buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, line));
            }
            if episodic.len() > ACTIVE_MEMORY_INDEX_CAP {
                buf.push_str(&format!(
                    "- ... ({} more not shown)\n",
                    episodic.len() - ACTIVE_MEMORY_INDEX_CAP
                ));
            }
            buf.push('\n');
        }
        if !self.all_skills.is_empty() {
            buf.push_str("## Available skills (use the body only when you decide to)\n");
            for s in self.all_skills.iter().take(SKILL_INDEX_CAP) {
                buf.push_str(&format!(
                    "- {}: {}\n",
                    s.frontmatter.name, s.frontmatter.description
                ));
            }
            if self.all_skills.len() > SKILL_INDEX_CAP {
                buf.push_str(&format!(
                    "- ... ({} more not shown)\n",
                    self.all_skills.len() - SKILL_INDEX_CAP
                ));
            }
            buf.push('\n');
        }
        buf.trim_end().to_string()
    }

    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = self.build_session_system();
        let matched =
            hermes_skills::match_for_query(self.all_skills, user_query, TRIGGERED_SKILL_CAP);
        if matched.is_empty() {
            return buf;
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str("## Skills triggered for this turn\n\n");
        for s in matched {
            buf.push_str(&format!("### {}\n", s.frontmatter.name));
            buf.push_str(s.body.trim());
            buf.push_str("\n\n");
        }
        buf.trim_end().to_string()
    }
}
