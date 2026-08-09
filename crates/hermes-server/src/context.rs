//! Per-turn system prompt assembly. 1:1 with `hermes-gui/src/context.rs`.

use hermes_core::companion;
use hermes_llm::ContextLimits;
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

pub struct ContextSources<'a> {
    pub base: Option<&'a str>,
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
    pub workspace_root: &'a str,
    pub limits: ContextLimits,
}

impl<'a> ContextSources<'a> {
    pub fn build_turn_system(&self, user_query: &str) -> String {
        let mut buf = String::new();

        buf.push_str(&format!(
            "You are {}, a local work-and-companion partner. Your workspace is `{}`.\n\n",
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

        let relevant: Vec<&LoadedMemory> = hermes_memory::search_memories(
            self.active,
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
}
