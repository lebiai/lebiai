//! Per-turn context assembly (shared engine surface): stitch the base system prompt, pinned memories,
//! active memory index, and skills discovery index into the `system` string
//! sent to the LLM.
//!
//! Skill bodies are NOT injected here — the LLM activates a skill by calling
//! the `skill_read` tool when it decides one is relevant (Agent Skills
//! "Progressive Disclosure": Discovery → Activation → Execution).
//!
//! Layout:
//! ```text
//! <base system, if any>
//!
//! ## 你现在是谁 (persona block — only when a persona is bound; it adds,
//!    never replaces the companion protocol above)
//!
//! ## Pinned memories (always loaded, full bodies — never displaced by an index)
//! - <full bodies>
//!
//! ## Topic cards (index — subject groups; the memory text stays authoritative)
//! - <title> (n): <summary>
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
    /// 基底正文：CLI `--system` / IM 渠道固定人设。Chat 与 IM 这条里含搭子协议
    /// （`hermes run` 的 `PromptKind::Batch` 是另一套正文，**不含**协议——别把
    /// 「base 一定等于搭子协议」当成本文件的通例）。
    /// **人物块插在它之后**：本文件的链是 `base → 人物块 → 记忆与卡`，
    /// 见 `build_session_system`。
    pub base: Option<&'a str>,
    /// 人物（工位）。`None` = 无人物——提示词与没有这层时逐字节相同。
    /// 只叠加在 `base` 之后、记忆与卡之前，永不覆盖协议本身（协议由 `base` 带入）。
    pub persona: Option<&'a hermes_core::persona::Persona>,
    /// 本机**开着的**其他工位（[`hermes_core::persona::open`]）。指路只能用这份
    /// 名单（规格 §2.3）：指到一个用户根本没有的工位 = 没指。
    /// `persona` 有值而这里是空 → 提示词里会写「本机只有你这一个工位」，
    /// 模型只能拒绝、不会编名字——**空 ≠ 万事大吉**，接线时别漏。
    pub roster: &'a [&'a hermes_core::persona::Persona],
    pub topic_cards: Option<&'a str>,
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
        // Identity humility — shared by all CLI dialogue paths.
        buf.push_str(hermes_core::companion::identity_discipline());
        buf.push('\n');
        if let Some(b) = self.base {
            buf.push_str(b);
            buf.push_str("\n\n");
        }

        // 人物（工位）叠加在搭子协议之后、记忆与卡之前。顺序即断言：
        // `the_persona_block_never_displaces_the_companion_protocol`（规格
        // `docs/spec/personas.md` §6.1 —— 人物只叠加，永不覆盖 `companion.rs`）。
        if let Some(p) = self.persona {
            buf.push_str(&hermes_core::persona::block(
                p,
                &crate::persona_scope::others(p, self.roster),
            ));
            buf.push('\n');
        }

        // Pinned memories come first and in full, whatever else exists. An
        // index is a summary and must never displace something the user pinned.
        if !self.pinned.is_empty() {
            buf.push_str("## Pinned memories (notes — verify before asserting identity)\n");
            for m in self.pinned {
                let body = m.body.trim();
                buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, body));
            }
            buf.push('\n');
        }

        if let Some(cards) = self.topic_cards {
            buf.push_str(cards.trim());
            buf.push('\n');
        } else if let Some(profile) = self.compiled_profile {
            buf.push_str("## User Profile (notes — verify before asserting identity)\n\n");
            buf.push_str(profile.trim());
            buf.push('\n');
        } else {
            // Episodic = active and not pinned (we already have pinned above).
            let episodic: Vec<&LoadedMemory> = self
                .active
                .iter()
                .filter(|m| !m.frontmatter.pinned)
                .collect();
            if !episodic.is_empty() {
                buf.push_str("## Active memory index (notes — may be wrong)\n");
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

        // 技能面收窄（规格 §6.1②）：这个工位只广告它绑定的技能 + 内置元技能。
        // 判据只有 `persona_scope::visible_skills` 一处——这里自己写一遍，
        // 迟早会和 GUI 那条路漂开。
        let skills = crate::persona_scope::visible_skills(self.persona, self.all_skills);
        if !skills.is_empty() {
            buf.push_str("## Available skills\n");
            buf.push_str(crate::persona_scope::SKILLS_ARE_NOT_STATIONS);
            buf.push('\n');
            buf.push_str(
                "Each entry below is just a skill's name and one-line description — the body is NOT loaded yet. \
When a user's request matches one of these, call the `skill_read` tool with the skill's name to load its full instructions before acting. \
Do not invent capabilities that aren't listed; do not paraphrase a skill from memory — read it first.\n\n",
            );
            for s in skills.iter().take(self.limits.skill_index_cap) {
                buf.push_str(&format!(
                    "- {}: {}\n",
                    s.frontmatter.name, s.frontmatter.description
                ));
            }
            if skills.len() > self.limits.skill_index_cap {
                buf.push_str(&format!(
                    "- ... ({} more not shown)\n",
                    skills.len() - self.limits.skill_index_cap
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

        // With topic cards the agent navigates memory through the index + tools.
        // When a compiled profile is active it already contains all memories.
        // Only inject per-turn memories in the legacy (no cards, no profile) path.
        if self.topic_cards.is_none() && self.compiled_profile.is_none() {
            let living = hermes_memory::living_rules(self.active.to_vec());
            let relevant: Vec<&LoadedMemory> = hermes_memory::search_memories_effective(
                &living,
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
                buf.push_str("## Relevant memories for this turn\n");
                buf.push_str(
                    "If any match the task, use Continuity (one short \"last time similar…\" with an anchor). \
Work episodes (zone=work / tag work-episode / 【工作情节】) are highest value — only when they truly fit.\n\n",
                );
                for m in relevant {
                    let zone = hermes_core::companion::zones::normalize(&m.frontmatter.zone);
                    let episode = hermes_core::companion::zones::is_work(zone)
                        || m.frontmatter
                            .tags
                            .iter()
                            .any(|t| hermes_core::companion::tags::is_episode_tag(t))
                        || m.body.contains("【工作情节】");
                    let kind = if episode { "work-episode" } else { "note" };
                    buf.push_str(&format!(
                        "- [{}|zone={}|{kind}] {}\n",
                        m.frontmatter.id,
                        zone,
                        m.body.trim()
                    ));
                }
            }
        }

        // Skill effectiveness is currently unread post-token-matcher removal;
        // keeping the field on the struct so callers don't have to be updated
        // when we wire it back in for ordering the discovery index.
        let _ = self.effectiveness;

        if hermes_core::companion::should_nudge_care_for_user_text(user_query) {
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str(hermes_core::companion::care_when_delivering_nudge());
        }
        if hermes_core::companion::should_nudge_pushback_for_user_text(user_query) {
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str(hermes_core::companion::pushback_nudge());
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
        let mut fm =
            MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "core".to_string());
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
        let mut fm = MemoryFrontmatter::new(
            Source::Reflection,
            Confidence::Medium,
            vec![],
            "general".to_string(),
        );
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

    fn tagged_memory(id: &str, zone: &str, tags: &[&str], body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::Reflection,
            Confidence::Medium,
            tags.iter().map(|t| t.to_string()).collect(),
            zone.to_string(),
        );
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: hermes_memory::Scope::User,
        }
    }

    #[test]
    fn empty_inputs_still_have_identity_discipline() {
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits::default(),
        };
        let s = sources.build_session_system();
        assert!(
            s.contains("Do not assert the user's profession"),
            "S3 identity discipline must always be present: {s}"
        );
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
            persona: None,
            roster: &[],
            topic_cards: None,
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
            persona: None,
            roster: &[],
            topic_cards: None,
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
        assert!(
            s.contains("Available skills"),
            "discovery index must appear"
        );
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
        let ep = episodic_memory(
            "mem_r",
            "user prefers anyhow over thiserror for app-layer errors",
        );
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
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

    /// 「工作情节」的判定读**唯一**词表（`companion::tags::is_episode_tag`，自带 trim）
    /// ——Task 1.12 必修 B。本文件曾留着一份本地副本（`t == "episode"`，不 trim），
    /// 模型写成 `" episode "` 的记忆因此在索引里被打成 `note`，连续性漏掉。
    /// 三条 or 分支各钉一条，免得日后顺手删掉其中一条。
    #[test]
    fn an_episode_memory_is_marked_as_a_work_episode_in_the_turn_index() {
        let by_tag = tagged_memory(
            "mem_tag",
            "general",
            &[" episode "],
            "归档流水线 nightly 重跑改到白天",
        );
        let by_zone = tagged_memory("mem_zone", "work", &[], "归档流水线 nightly 重跑改到白天");
        let by_marker = tagged_memory(
            "mem_marker",
            "general",
            &[],
            "【工作情节】归档流水线 nightly 重跑改到白天",
        );
        let note = tagged_memory(
            "mem_note",
            "general",
            &[],
            "归档流水线 nightly 重跑改到白天",
        );
        let active = vec![by_tag, by_zone, by_marker, note];
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &active,
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits {
                relevant_memory_cap: 8,
                ..ContextLimits::default()
            },
        };
        let s = sources.build_turn_system("归档流水线 nightly 重跑");

        assert!(
            s.contains("[mem_tag|zone=general|work-episode]"),
            "带空格的 \" episode \" tag 必须打成工作情节：{s}"
        );
        assert!(
            s.contains("[mem_zone|zone=work|work-episode]"),
            "zone=work 的语义不变：{s}"
        );
        assert!(
            s.contains("[mem_marker|zone=general|work-episode]"),
            "正文里的【工作情节】语义不变：{s}"
        );
        assert!(
            s.contains("[mem_note|zone=general|note]"),
            "三个信号都没有的就是 note：{s}"
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
            persona: None,
            roster: &[],
            topic_cards: None,
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
    fn compiled_profile_replaces_the_plain_index_but_not_pinned() {
        let pinned = pinned_memory("mem_p", "pinned body");
        let ep = episodic_memory("mem_e", "episodic body");
        let profile = "## User\n- architect on Mac\n\n## Habits\n- prefers vim";
        let sources = ContextSources {
            base: Some("base prompt"),
            persona: None,
            roster: &[],
            topic_cards: None,
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
        assert!(
            s.contains("architect on Mac"),
            "profile content should appear"
        );
        assert!(
            s.contains("Pinned memories") && s.contains("pinned body"),
            "pinned full text stays even when a profile exists"
        );
        assert!(
            !s.contains("Active memory index"),
            "plain index should be skipped"
        );

        let t = sources.build_turn_system("error handling question");
        assert!(
            !t.contains("Relevant memories"),
            "per-turn memories should be skipped"
        );
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
            persona: None,
            roster: &[],
            topic_cards: None,
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
    fn topic_cards_replace_the_plain_index_but_never_the_pinned_block() {
        let pinned = pinned_memory("mem_p", "pinned body");
        let ep = episodic_memory("mem_e", "episodic body");
        let cards = "## Topic cards (index — NOT the wording)\n- 财经内容 (2): sum";
        let sources = ContextSources {
            base: Some("base prompt"),
            persona: None,
            roster: &[],
            topic_cards: Some(cards),
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
        assert!(s.contains("Topic cards"), "topic cards should appear");
        assert!(
            !s.contains("User Profile"),
            "compiled profile should be skipped when cards exist"
        );
        assert!(
            s.contains("Pinned memories") && s.contains("pinned body"),
            "pinned full text must survive an index being present"
        );
        assert!(
            !s.contains("Active memory index"),
            "plain index section should be skipped"
        );
        assert!(s.contains("财经内容"), "card body should appear");

        let t = sources.build_turn_system("how do I handle errors?");
        assert!(
            !t.contains("Relevant memories"),
            "per-turn memories should be skipped when cards are active"
        );
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
            persona: None,
            roster: &[],
            topic_cards: Some("## Topic cards\n- 财经 (1): sum"),
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
        assert!(
            s.contains("### memory-palace"),
            "always-active skill header should appear"
        );
        assert!(
            s.contains("Memory Palace Protocol"),
            "always-active skill body should appear"
        );
    }

    #[test]
    fn the_persona_block_never_displaces_the_companion_protocol() {
        let pinned = pinned_memory("mem_p", "pinned body");
        let cards = "## Topic cards (index — NOT the wording)\n- 财经内容 (2): sum";
        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let sources = ContextSources {
            base: Some("BASE"),
            persona: Some(p),
            roster: &[],
            topic_cards: Some(cards),
            compiled_profile: None,
            always_active_skills: &[],
            pinned: std::slice::from_ref(&pinned),
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits::default(),
        };
        let sys = sources.build_session_system();
        let ident = sys
            .find(hermes_core::companion::identity_discipline())
            .expect("身份纪律必须在");
        let base = sys.find("BASE").expect("base 必须在");
        let mine = sys.find("## 你现在是谁").expect("人物块必须在");
        let pinned_at = sys.find("## Pinned memories").expect("记忆必须在");
        let cards_at = sys.find("## Topic cards").expect("卡必须在");
        assert!(
            ident < base && base < mine,
            "顺序：身份纪律 → base（搭子协议）→ 人物块"
        );
        assert!(
            mine < pinned_at && pinned_at < cards_at,
            "人物块必须落在记忆与卡之前（规格 §6.1：顺序即断言）"
        );
        assert!(sys.contains("没依据的话，我不说"));
    }

    #[test]
    fn no_persona_means_no_persona_block() {
        let sources = ContextSources {
            base: Some("BASE"),
            persona: None,
            roster: &[],
            topic_cards: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits::default(),
        };
        assert!(
            !sources.build_session_system().contains("你现在是谁"),
            "没指定人物的会话不长出人物块"
        );
    }

    #[test]
    fn a_session_without_a_persona_is_byte_identical_to_today() {
        // 零迁移：`persona: None` 就是今天的提示词。插一块人物只**插入**那一段，
        // 不改动其它任何一个字节——按位置切开比前后两段，不靠 replace 或尾换行。
        let pinned = pinned_memory("mem_p", "always remember this");
        let ep = episodic_memory("mem_e", "occasional fact");
        let plain = ContextSources {
            base: Some("BASE"),
            persona: None,
            roster: &[],
            topic_cards: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: std::slice::from_ref(&pinned),
            active: std::slice::from_ref(&ep),
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits::default(),
        };
        let without = plain.build_turn_system("帮我改这段标题");
        assert!(!without.contains("你现在是谁"), "{without}");

        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let with = ContextSources {
            persona: Some(p),
            roster: &[],
            ..plain
        }
        .build_turn_system("帮我改这段标题");
        let segment = format!("{}\n", hermes_core::persona::block(p, &[]));
        let at = with.find(&segment).expect("人物块必须以完整的这一段出现");
        assert_eq!(with.get(..at), without.get(..at), "插入点之前逐字节相同");
        assert_eq!(
            with.get(at + segment.len()..),
            without.get(at..),
            "插入点之后逐字节相同——有/无人物之间只多出那一段"
        );
    }

    #[test]
    fn the_persona_block_is_inserted_once_per_turn_prompt() {
        // 本文件的 `build_turn_system` 以 `build_session_system` 为前缀，两条
        // 路径共用同一次插入——每轮追加的记忆 / 技能索引不得再插一遍。
        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let sources = ContextSources {
            base: Some("BASE"),
            persona: Some(p),
            roster: &[],
            topic_cards: None,
            compiled_profile: None,
            always_active_skills: &[],
            pinned: &[],
            active: &[],
            all_skills: &[],
            effectiveness: None,
            memory_effectiveness: None,
            limits: ContextLimits::default(),
        };
        let block = hermes_core::persona::block(p, &[]);
        // 两个 builder 都会 `trim_end()`；这里没有别的内容跟着，所以针用
        // `trim_end()` 后的文本——只去掉尾换行，不影响「出现几次」这个问题。
        let needle = block.trim_end();
        assert_eq!(
            sources.build_session_system().matches(needle).count(),
            1,
            "会话层只插一次"
        );
        assert_eq!(
            sources
                .build_turn_system("帮我看看林碳这一摊")
                .matches(needle)
                .count(),
            1,
            "每轮提示词里仍然只出现一次"
        );
    }
}
