//! Per-turn context assembly: stitch base system prompt + pinned memories
//! + active memory index + skills index (+ matched-skill bodies) into the
//!
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

use std::collections::HashMap;

use hermes_memory::{LoadedMemory, MemoryEffectiveness};
use hermes_skills::{LoadedSkill, SkillEffectiveness};

const ACTIVE_MEMORY_INDEX_CAP: usize = 50;
const SKILL_INDEX_CAP: usize = 50;
const TRIGGERED_SKILL_CAP: usize = 3;
const RELEVANT_MEMORY_CAP: usize = 3;

pub struct ContextSources<'a> {
    pub base: Option<&'a str>,
    pub palace_index: Option<&'a str>,
    pub compiled_profile: Option<&'a str>,
    pub always_active_skills: &'a [&'a LoadedSkill],
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
    pub effectiveness: Option<&'a HashMap<String, SkillEffectiveness>>,
    pub memory_effectiveness: Option<&'a HashMap<String, MemoryEffectiveness>>,
}

impl<'a> ContextSources<'a> {
    pub fn build_session_system(&self) -> String {
        let mut buf = String::new();
        if let Some(b) = self.base {
            buf.push_str(b);
            buf.push_str("\n\n");
        }

        if let Some(index) = self.palace_index {
            buf.push_str(index.trim());
            buf.push('\n');
        } else if let Some(profile) = self.compiled_profile {
            buf.push_str("## User Profile\n\n");
            buf.push_str(profile.trim());
            buf.push('\n');
        } else {
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
        }

        // Always-active skills injected directly into session prompt.
        for s in self.always_active_skills {
            buf.push_str(&format!("### {}\n", s.frontmatter.name));
            buf.push_str(s.body.trim());
            buf.push_str("\n\n");
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

    /// Build the per-turn system prompt: session-level prefix + relevant
    /// memory bodies + the bodies of the skills triggered by this user turn.
    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = self.build_session_system();

        // When palace index is active, agent uses tools for memory retrieval.
        // When compiled profile is active, it already contains all memories.
        // Only inject per-turn memories in the legacy (no palace, no profile) path.
        if self.palace_index.is_none() && self.compiled_profile.is_none() {
            let relevant: Vec<&LoadedMemory> = hermes_memory::search_memories_effective(
                self.active,
                user_query,
                RELEVANT_MEMORY_CAP + self.pinned.len(),
                self.memory_effectiveness,
            )
            .into_iter()
            .filter(|m| !m.frontmatter.pinned)
            .take(RELEVANT_MEMORY_CAP)
            .collect();
            if !relevant.is_empty() {
                if !buf.is_empty() {
                    buf.push_str("\n\n");
                }
                buf.push_str("## Relevant memories for this turn\n\n");
                for m in relevant {
                    buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, m.body.trim()));
                }
            }
        }

        let matched = hermes_skills::match_for_query_with_effectiveness(
            self.all_skills,
            user_query,
            TRIGGERED_SKILL_CAP,
            self.effectiveness,
        );
        if matched.is_empty() {
            return buf.trim_end().to_string();
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
        let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "core".to_string());
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
        let mut fm = MemoryFrontmatter::new(Source::Reflection, Confidence::Medium, vec![], "general".to_string());
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
                always_active: false,
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
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
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
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[pinned],
            active: &[ep],
            all_skills: &[sk],
            effectiveness: None,
            memory_effectiveness: None,
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
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[sk],
            effectiveness: None,
            memory_effectiveness: None,
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
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[sk],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_turn_system("rust unwrap question");
        assert!(!s.contains("Skills triggered"));
    }

    #[test]
    fn build_turn_system_injects_relevant_memory_bodies() {
        let ep = episodic_memory("mem_r", "user prefers anyhow over thiserror for app-layer errors");
        let sources = ContextSources {
            base: None,
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[ep],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_turn_system("how should I handle errors in the app layer?");
        assert!(s.contains("Relevant memories for this turn"));
        assert!(s.contains("[mem_r]"));
        assert!(
            s.contains("user prefers anyhow"),
            "full body should be injected, not just one-line index"
        );
    }

    #[test]
    fn pinned_memory_excluded_from_episodic_section() {
        // A memory marked pinned must appear only in the Pinned section,
        // not duplicated in the Active memory index.
        let p = pinned_memory("mem_p", "pinned body");
        let p_pinned = p.clone();
        let sources = ContextSources {
            base: None,
            palace_index: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: std::slice::from_ref(&p_pinned),
            active: &[p],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_session_system();
        let occurrences = s.matches("mem_p").count();
        assert_eq!(occurrences, 1, "pinned memory must not duplicate");
    }

    #[test]
    fn compiled_profile_replaces_memory_sections() {
        let pinned = pinned_memory("mem_p", "pinned body");
        let ep = episodic_memory("mem_e", "episodic body");
        let profile = "## User\n- architect on Mac\n\n## Habits\n- prefers vim";
        let sources = ContextSources {
            base: Some("base prompt"),
            palace_index: None,
            compiled_profile: Some(profile),
            always_active_skills: &[],
            pinned: &[pinned],
            active: &[ep],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_session_system();
        assert!(s.contains("User Profile"), "profile section should exist");
        assert!(s.contains("architect on Mac"), "profile content should appear");
        assert!(!s.contains("Pinned memories"), "pinned section should be skipped");
        assert!(!s.contains("Active memory index"), "index should be skipped");
        assert!(!s.contains("mem_p"), "individual memory IDs should not appear");

        let t = sources.build_turn_system("error handling question");
        assert!(!t.contains("Relevant memories"), "per-turn memories should be skipped");
    }

    #[test]
    fn compiled_profile_coexists_with_skills() {
        let sk = skill(
            "rust-error",
            "switch unwrap to anyhow",
            &["rust", "anyhow", "unwrap"],
            "step 1: find unwrap\nstep 2: rewrite",
        );
        let sources = ContextSources {
            base: None,
            palace_index: None,
            compiled_profile: Some("## Profile\n- architect"),
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[sk],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_turn_system("rewrite rust unwrap calls");
        assert!(s.contains("User Profile"), "profile should be present");
        assert!(s.contains("Skills triggered"), "skills should still trigger");
        assert!(s.contains("step 1: find unwrap"), "skill body should be injected");
    }

    #[test]
    fn palace_index_replaces_all_memory_sections() {
        let pinned = pinned_memory("mem_p", "pinned body");
        let ep = episodic_memory("mem_e", "episodic body");
        let index = "## Memory Palace\n3 memories across 2 zones.\n\n### core (2)\n- architect\n### general (1)\n- misc";
        let sources = ContextSources {
            base: Some("base prompt"),
            palace_index: Some(index),
            compiled_profile: Some("should be ignored"),
            always_active_skills: &[],
            pinned: std::slice::from_ref(&pinned),
            active: std::slice::from_ref(&ep),
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_session_system();
        assert!(s.contains("Memory Palace"), "palace index should appear");
        assert!(s.contains("core (2)"), "zone listing should appear");
        assert!(!s.contains("User Profile"), "compiled profile should be skipped");
        assert!(!s.contains("Pinned memories"), "pinned section should be skipped");
        assert!(!s.contains("Active memory index"), "index section should be skipped");

        let t = sources.build_turn_system("how do I handle errors?");
        assert!(!t.contains("Relevant memories"), "per-turn memories should be skipped when palace active");
    }

    #[test]
    fn always_active_skills_injected() {
        let sk = skill(
            "memory-palace",
            "Protocol for navigating the Memory Palace",
            &[],
            "# Memory Palace Protocol\nYour memories are organized into zones.",
        );
        let sk_ref = &sk;
        let sources = ContextSources {
            base: None,
            palace_index: Some("## Memory Palace\n1 zone"),
            compiled_profile: None,
            always_active_skills: std::slice::from_ref(&sk_ref),
            pinned: &[],
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
        };
        let s = sources.build_session_system();
        assert!(s.contains("### memory-palace"), "always-active skill header should appear");
        assert!(s.contains("Memory Palace Protocol"), "always-active skill body should appear");
    }
}
