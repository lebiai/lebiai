//! Per-turn context assembly: stitch base system prompt + pinned memories
//! + active memory index + skills discovery index into the `system` string
//! sent to the LLM. Skill bodies are NOT injected here — the LLM activates
//! a skill by calling the `skill_read` tool when it decides one is relevant
//! (Agent Skills "Progressive Disclosure": Discovery → Activation → Execution).
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
//! ## Available skills
//! <usage instruction telling the LLM to call skill_read>
//! - <name>: <description>
//! ```

use std::collections::HashMap;

use hermes_llm::ContextLimits;
use hermes_memory::{LoadedMemory, MemoryEffectiveness};
use hermes_skills::{LoadedSkill, SkillEffectiveness};

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
    pub limits: ContextLimits,
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
        }

        // Always-active skills injected directly into session prompt.
        for s in self.always_active_skills {
            buf.push_str(&format!("### {}\n", s.frontmatter.name));
            buf.push_str(s.body.trim());
            buf.push_str("\n\n");
        }

        if !self.all_skills.is_empty() {
            buf.push_str("## Available skills\n");
            buf.push_str(
                "Each entry below is just a skill's name and one-line description — the body is NOT loaded yet. \
When a user's request matches one of these, call the `skill_read` tool with the skill's name to load its full instructions before acting. \
Do not invent capabilities that aren't listed; do not paraphrase a skill from memory — read it first.\n\n",
            );
            for s in self.all_skills.iter().take(self.limits.skill_index_cap) {
                buf.push_str(&format!(
                    "- {}: {}\n",
                    s.frontmatter.name, s.frontmatter.description
                ));
            }
            if self.all_skills.len() > self.limits.skill_index_cap {
                buf.push_str(&format!(
                    "- ... ({} more not shown)\n",
                    self.all_skills.len() - self.limits.skill_index_cap
                ));
            }
            buf.push('\n');
        }

        buf.trim_end().to_string()
    }

    /// Build the per-turn system prompt: session-level prefix + relevant
    /// memory bodies. Skill bodies are NOT injected here — the discovery
    /// index lives in `build_session_system`, and the LLM activates a skill
    /// by calling the `skill_read` tool when it decides one is relevant.
    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = self.build_session_system();

        // When palace index is active, agent uses tools for memory retrieval.
        // When compiled profile is active, it already contains all memories.
        // Only inject per-turn memories in the legacy (no palace, no profile) path.
        if self.palace_index.is_none() && self.compiled_profile.is_none() {
            let relevant: Vec<&LoadedMemory> = hermes_memory::search_memories_effective(
                self.active,
                user_query,
                self.limits.relevant_memory_cap + self.pinned.len(),
                self.memory_effectiveness,
            )
            .into_iter()
            .filter(|m| !m.frontmatter.pinned)
            .take(self.limits.relevant_memory_cap)
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

        // Skill effectiveness is currently unread post-token-matcher removal;
        // keeping the field on the struct so callers don't have to be updated
        // when we wire it back in for ordering the discovery index.
        let _ = self.effectiveness;

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
            limits: ContextLimits::default(),
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
            limits: ContextLimits::default(),
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
    fn build_turn_system_does_not_inject_skill_bodies() {
        // Skills are now discovered via the index + activated via the
        // `skill_read` tool. The per-turn prompt must NOT inline any
        // skill body, even when the user query matches the triggers.
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
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("please rewrite the rust unwrap calls");
        assert!(s.contains("Available skills"), "discovery index must appear");
        assert!(s.contains("rust-error: switch unwrap to anyhow"));
        assert!(
            !s.contains("Skills triggered"),
            "per-turn skill injection has been removed"
        );
        assert!(
            !s.contains("step 1: find unwrap"),
            "skill body must NOT be inlined — LLM should call skill_read instead"
        );
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
            limits: ContextLimits::default(),
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
            limits: ContextLimits::default(),
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
            limits: ContextLimits::default(),
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
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("rewrite rust unwrap calls");
        assert!(s.contains("User Profile"), "profile should be present");
        assert!(
            s.contains("Available skills"),
            "discovery index should still appear"
        );
        assert!(
            s.contains("rust-error: switch unwrap to anyhow"),
            "skill name + description should be in discovery"
        );
        assert!(
            !s.contains("step 1: find unwrap"),
            "skill body must NOT be inlined post-token-matcher removal"
        );
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
            limits: ContextLimits::default(),
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
            limits: ContextLimits::default(),
        };
        let s = sources.build_session_system();
        assert!(s.contains("### memory-palace"), "always-active skill header should appear");
        assert!(s.contains("Memory Palace Protocol"), "always-active skill body should appear");
    }
}
