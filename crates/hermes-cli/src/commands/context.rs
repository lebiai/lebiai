//! Per-turn context assembly: stitch base system prompt + pinned memories
//! + active memory index + skills index (+ matched-skill bodies) into the
//! `system` string sent to the LLM.
//!
//! Layout:
//! ```text
//! <base system, if any>
//!
//! ## Pinned memories (always loaded)
//! - <full bodies>
//!
//! ## Active memory index
//! - <id>: <one-line summary>
//!
//! ## Available skills (use the body only when you decide to)
//! - <name>: <description>
//!
//! ## Skills triggered for this turn
//! ### <name>
//! <full body>
//! ```

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

        // Episodic = active and not pinned (we already have pinned above).
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

    /// Build the per-turn system prompt: session-level prefix + the bodies
    /// of the skills triggered by this user turn.
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

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, MemoryFrontmatter, Source};
    use hermes_skills::{Scope, SkillFrontmatter};
    use serde_yaml::Mapping;
    use std::path::PathBuf;

    fn pinned_memory(id: &str, body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![]);
        fm.id = id.to_string();
        fm.pinned = true;
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: hermes_memory::Scope::User,
        }
    }

    fn episodic_memory(id: &str, body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(Source::Reflection, Confidence::Medium, vec![]);
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: hermes_memory::Scope::User,
        }
    }

    fn skill(name: &str, desc: &str, triggers: &[&str], body: &str) -> LoadedSkill {
        LoadedSkill {
            frontmatter: SkillFrontmatter {
                name: name.to_string(),
                description: desc.to_string(),
                triggers: triggers.iter().map(|s| s.to_string()).collect(),
                version: None,
                license: None,
                extra: Mapping::new(),
            },
            body: body.to_string(),
            source: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn empty_inputs_yield_empty_string() {
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
        };
        assert_eq!(sources.build_session_system(), "");
    }

    #[test]
    fn assembles_three_sections() {
        let pinned = pinned_memory("mem_p", "always remember this");
        let ep = episodic_memory("mem_e", "occasional fact\nsecond line");
        let sk = skill(
            "rust-error",
            "switch unwrap to anyhow",
            &["rust", "anyhow"],
            "step 1\nstep 2",
        );
        let sources = ContextSources {
            base: Some("you are a helpful agent."),
            pinned: &[pinned],
            active: &[ep],
            all_skills: &[sk],
        };
        let s = sources.build_session_system();
        assert!(s.contains("you are a helpful agent."));
        assert!(s.contains("Pinned memories"));
        assert!(s.contains("[mem_p] always remember this"));
        assert!(s.contains("Active memory index"));
        assert!(s.contains("[mem_e] occasional fact"));
        assert!(!s.contains("second line"), "episodic should be one-line");
        assert!(s.contains("Available skills"));
        assert!(s.contains("rust-error: switch unwrap to anyhow"));
        // Body must NOT appear in the index.
        assert!(!s.contains("step 1"));
    }

    #[test]
    fn build_turn_system_injects_matched_skill_body() {
        let sk = skill(
            "rust-error",
            "switch unwrap to anyhow",
            &["rust", "anyhow", "unwrap"],
            "step 1: find unwrap\nstep 2: rewrite",
        );
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[sk],
        };
        let s = sources.build_turn_system("please rewrite the rust unwrap calls");
        assert!(s.contains("Skills triggered for this turn"));
        assert!(s.contains("### rust-error"));
        assert!(s.contains("step 1: find unwrap"));
    }

    #[test]
    fn build_turn_system_skips_section_when_no_match() {
        let sk = skill("python-x", "py", &["python"], "py body");
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[sk],
        };
        let s = sources.build_turn_system("rust unwrap question");
        assert!(!s.contains("Skills triggered"));
    }

    #[test]
    fn pinned_memory_excluded_from_episodic_section() {
        // A memory marked pinned must appear only in the Pinned section,
        // not duplicated in the Active memory index.
        let p = pinned_memory("mem_p", "pinned body");
        let sources = ContextSources {
            base: None,
            pinned: &[p.clone()],
            active: &[p],
            all_skills: &[],
        };
        let s = sources.build_session_system();
        let occurrences = s.matches("mem_p").count();
        assert_eq!(occurrences, 1, "pinned memory must not duplicate");
    }
}
