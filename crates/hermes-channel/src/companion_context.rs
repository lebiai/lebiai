//! Shared companion-surface system prompt (desktop GUI + hermes-server/Flutter).
//!
//! Single source of truth — do **not** duplicate this builder in gui/server crates.
//! IM/CLI session assembly still uses [`crate::context::ContextSources`] (palace/profile).

use hermes_commitments::{Commitment, INDEX_CAP, OPEN_CROWD};
use hermes_core::companion;
use hermes_llm::ContextLimits;
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

pub struct ContextSources<'a> {
    pub base: Option<&'a str>,
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
    pub open_work: &'a [Commitment],
    pub first_human_today: bool,
    pub workspace_root: &'a str,
    pub limits: ContextLimits,
}

impl<'a> ContextSources<'a> {
    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = String::new();

        buf.push_str(&format!(
            "You are {}, a local work companion (工作搭子). Your workspace is `{}`.\n\n",
            companion::PRODUCT_NAME,
            self.workspace_root
        ));
        buf.push_str(companion::companion_protocol());
        buf.push('\n');
        buf.push_str(companion::gui_tools_clause());
        buf.push('\n');
        buf.push_str(companion::memory_save_clause());
        buf.push('\n');
        buf.push_str(companion::speech_honesty_clause());
        buf.push('\n');
        buf.push_str(companion::uploads_clause());
        buf.push('\n');

        self.append_open_work(&mut buf, user_query);

        if let Some(b) = self.base {
            buf.push_str(b);
            buf.push_str("\n\n");
        }

        if !self.pinned.is_empty() {
            buf.push_str("## Pinned memories (notes — verify before asserting identity)\n");
            for m in self.pinned {
                let body = m.body.trim();
                buf.push_str(&format!("- [{}] {}\n", m.frontmatter.id, body));
            }
            buf.push('\n');
        }

        let living: Vec<hermes_memory::LoadedMemory> =
            hermes_memory::living_rules(self.active.to_vec());
        let episodic: Vec<&LoadedMemory> =
            living.iter().filter(|m| !m.frontmatter.pinned).collect();
        if !episodic.is_empty() {
            buf.push_str("## Active memory index (living rules — one per kind of work)\n");
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

        let relevant: Vec<&LoadedMemory> = hermes_memory::search_memories(
            &living,
            user_query,
            self.limits.relevant_memory_cap + self.pinned.len(),
        )
        .into_iter()
        .filter(|m| !m.frontmatter.pinned)
        .take(self.limits.relevant_memory_cap)
        .collect();
        if !relevant.is_empty() {
            buf.push_str("## Relevant memories for this turn\n");
            buf.push_str(
                "If any match the user's task, use Continuity: one short beat like \"last time on similar work…\" with an anchor. \
**Work episodes** (zone=work or tag work-episode / body starts with 【工作情节】) are highest value for re-recognition — use them when they truly fit. \
If none truly match, do not pretend you remember.\n\n",
            );
            for m in relevant {
                let zone = m.frontmatter.zone.as_str();
                let episode = zone == "work"
                    || m.frontmatter.tags.iter().any(|t| {
                        let t = t.to_lowercase();
                        t == "work-episode" || t == "episode"
                    })
                    || m.body.contains("【工作情节】");
                let kind = if episode { "work-episode" } else { "note" };
                buf.push_str(&format!(
                    "- [{}|zone={}|{kind}] {}\n",
                    m.frontmatter.id,
                    zone,
                    m.body.trim()
                ));
            }
            buf.push('\n');
        }

        // Always-active skills are injected in full — their body is the
        // standing instruction set (e.g. memory-palace), not a discoverable
        // skill. Same semantics as `hermes-channel`'s session prompt.
        let always_active: Vec<&LoadedSkill> = self
            .all_skills
            .iter()
            .filter(|s| s.frontmatter.always_active)
            .collect();
        for s in always_active {
            buf.push_str(&format!("### {}\n", s.frontmatter.name));
            buf.push_str(s.body.trim());
            buf.push_str("\n\n");
        }

        if !self.all_skills.is_empty() {
            buf.push_str("## Available skills\n");
            buf.push_str(companion::skill_discovery_clause());
            buf.push('\n');
            for s in self.all_skills.iter().take(self.limits.skill_index_cap) {
                buf.push_str(&format!(
                    "- {}: {}\n",
                    s.frontmatter.name, s.frontmatter.description
                ));
            }
            buf.push('\n');
        }

        if hermes_core::companion::should_nudge_care_for_user_text(user_query) {
            buf.push('\n');
            buf.push_str(hermes_core::companion::care_when_delivering_nudge());
            buf.push('\n');
        }
        if hermes_core::companion::should_nudge_pushback_for_user_text(user_query) {
            buf.push('\n');
            buf.push_str(hermes_core::companion::pushback_nudge());
            buf.push('\n');
        }

        buf.trim_end().to_string()
    }

    fn append_open_work(&self, buf: &mut String, user_query: &str) {
        let owed: Vec<&Commitment> = self
            .open_work
            .iter()
            .filter(|c| c.status.is_owed())
            .collect();
        let titles: Vec<&str> = owed.iter().flat_map(|c| c.phrases()).collect();
        let hits = companion::query_hits_zaiban_title(user_query, &titles);
        if !companion::should_inject_zaiban_index(
            user_query,
            !owed.is_empty(),
            hits,
            self.first_human_today,
        ) {
            return;
        }
        buf.push_str(companion::zaiban_index_clause());
        for c in owed.iter().take(INDEX_CAP) {
            let extra = match (c.status, c.soft_due.as_deref()) {
                (hermes_commitments::Status::Waiting, due) => {
                    format!(
                        " waiting{}",
                        due.map(|d| format!(" {d}")).unwrap_or_default()
                    )
                }
                (_, Some(due)) => format!(" due={due}"),
                _ => String::new(),
            };
            buf.push_str(&format!("- [{}] {}{extra}\n", c.id, c.title));
        }
        if owed.len() > INDEX_CAP {
            buf.push_str(&format!("- ... ({} more)\n", owed.len() - INDEX_CAP));
        }
        buf.push('\n');
        if owed.len() >= OPEN_CROWD {
            buf.push_str(companion::zaiban_crowded_nudge());
            buf.push('\n');
        }
        let today = chrono::Local::now().date_naive();
        if owed.iter().any(|c| c.is_overdue(today)) {
            buf.push_str(companion::zaiban_overdue_nudge());
            buf.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, MemoryFrontmatter, Source};
    use hermes_skills::{Scope, SkillFrontmatter};
    use serde_yaml::Mapping;
    use std::path::PathBuf;

    fn mem(id: &str, body: &str, pinned: bool) -> LoadedMemory {
        let mut fm =
            MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "work".to_string());
        fm.id = id.to_string();
        fm.pinned = pinned;
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: hermes_memory::Scope::User,
        }
    }

    fn skill(name: &str, body: &str) -> LoadedSkill {
        LoadedSkill {
            frontmatter: SkillFrontmatter {
                name: name.to_string(),
                description: "desc".into(),
                triggers: vec!["rust".into()],
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
    fn companion_not_chatbot() {
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("hello");
        assert!(s.contains("work companion"));
        assert!(!s.contains("work partner"));
        assert!(!s.contains("helpful local AI assistant"));
    }

    #[test]
    fn care_nudge_on_deliverable_request() {
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("帮我写一份项目复盘并整理成文档");
        assert!(
            s.contains("Care reminder") || s.contains("work delivery"),
            "deliverable request should attach Care nudge"
        );
        let s2 = sources.build_turn_system("定稿，不要建议");
        assert!(!s2.contains("Care reminder"));
    }

    #[test]
    fn pushback_nudge_on_decision_request() {
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("A 和 B 两个方案怎么选，帮我权衡");
        assert!(
            s.contains("Give-and-take") || s.contains("rubber-stamp"),
            "decision turns should attach pushback nudge"
        );
    }

    #[test]
    fn injects_relevant_memory_not_skill_body() {
        let ep = mem(
            "mem1",
            "user drafted a project retro with three-part structure",
            false,
        );
        let sk = skill("retro-write", "SECRET_BODY_SHOULD_NOT_APPEAR");
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: std::slice::from_ref(&ep),
            all_skills: std::slice::from_ref(&sk),
            open_work: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("write another project retro");
        assert!(s.contains("Relevant memories") || s.contains("mem1") || s.contains("retro"));
        assert!(
            !s.contains("SECRET_BODY_SHOULD_NOT_APPEAR"),
            "skill bodies must not be inlined"
        );
        assert!(s.contains("skill_read") || s.contains("Available skills"));
    }

    #[test]
    fn injects_open_work_on_query() {
        let mut item =
            hermes_commitments::Commitment::new("周五交改稿", hermes_commitments::Source::User)
                .unwrap();
        item.id = "cmt_test".into();
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: std::slice::from_ref(&item),
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("今天干什么");
        assert!(s.contains("周五交改稿"), "{s}");
        assert!(s.contains("cmt_test"));
        let quiet = sources.build_turn_system("这段标题怎么改");
        assert!(!quiet.contains("周五交改稿"), "{quiet}");
    }

    #[test]
    fn always_active_skill_body_is_inlined() {
        let mut active = skill("memory-palace", "ALWAYS_ACTIVE_BODY_MUST_APPEAR");
        active.frontmatter.always_active = true;
        let sources = ContextSources {
            base: None,
            pinned: &[],
            active: &[],
            all_skills: std::slice::from_ref(&active),
            open_work: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("hello");
        assert!(s.contains("ALWAYS_ACTIVE_BODY_MUST_APPEAR"));
        assert!(s.contains("### memory-palace"));
    }
}
