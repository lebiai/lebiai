//! Shared companion-surface system prompt (desktop GUI + hermes-server/Flutter).
//!
//! Single source of truth — do **not** duplicate this builder in gui/server crates.
//! IM/CLI session assembly still uses [`crate::context::ContextSources`] (cards/profile).
//!
//! Memory reaches the model in three layers, in this order: pinned memories in
//! full, the topic-card index, then whatever this turn's query actually pulls
//! back. A card is a summary — the memory text stays authoritative.
//!
//! A bound persona (工位) adds one block after the companion protocol and its
//! tool clauses, before open work and memory. It never replaces the protocol.

use hermes_commitments::{Commitment, INDEX_CAP, OPEN_CROWD};
use hermes_core::companion;
use hermes_llm::ContextLimits;
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;

pub struct ContextSources<'a> {
    /// 这里的 `base` 是**附加说明**，不是搭子协议：协议由
    /// `companion::companion_protocol()` 在 `build_turn_system` 开头写入，
    /// 人物块插在协议之后、`base` **之前**（本文件与 `context.rs` 的布局不同，
    /// 别照搬那边的 `base → 人物块`）。
    pub base: Option<&'a str>,
    /// 人物（工位）。`None` = 无人物——提示词与没有这层时逐字节相同。
    /// 位置：`companion_protocol()` 与工具条款之后、在办与 `base` 之前。
    pub persona: Option<&'a hermes_core::persona::Persona>,
    /// 本机**开着的**其他工位（[`hermes_core::persona::open`]）。指路只能用这份
    /// 名单（规格 §2.3）：指到一个用户根本没有的工位 = 没指。
    /// `persona` 有值而这里是空 → 提示词里会写「本机只有你这一个工位」，
    /// 模型只能拒绝、不会编名字——**空 ≠ 万事大吉**，接线时别漏。
    pub roster: &'a [&'a hermes_core::persona::Persona],
    /// Rendered topic-card index (see `hermes_memory::topics`). `None` falls
    /// back to a flat index of the living memories.
    pub topic_cards: Option<&'a str>,
    pub pinned: &'a [LoadedMemory],
    pub active: &'a [LoadedMemory],
    pub all_skills: &'a [LoadedSkill],
    pub open_work: &'a [Commitment],
    /// Engine-retrieved file excerpts. Empty on IM / when nothing matched.
    pub material_hits: &'a [hermes_core::MaterialHit],
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
        buf.push_str(&companion::companion_protocol());
        buf.push('\n');
        buf.push_str(companion::gui_tools_clause());
        buf.push('\n');
        buf.push_str(companion::memory_save_clause());
        buf.push('\n');
        buf.push_str(companion::speech_honesty_clause());
        buf.push('\n');
        buf.push_str(companion::uploads_clause());
        buf.push('\n');

        // 人物（工位）叠加在搭子协议与工具条款之后、在办与记忆之前。这条路径
        // **没有**独立的会话层（`build_turn_system` 就是整份提示词），所以块
        // 在这里落一次、也只落一次——顺序即断言，见 `mod tests`。
        if let Some(p) = self.persona {
            buf.push_str(&hermes_core::persona::block(
                p,
                &crate::persona_scope::others(p, self.roster),
            ));
            buf.push('\n');
        }

        self.append_open_work(&mut buf, user_query);

        if !self.material_hits.is_empty() {
            buf.push_str(&hermes_core::companion::materials_hits_block(
                self.material_hits,
            ));
        }

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
        if let Some(cards) = self.topic_cards {
            buf.push_str(cards.trim());
            buf.push_str("\n\n");
        } else {
            let episodic: Vec<&LoadedMemory> =
                living.iter().filter(|m| !m.frontmatter.pinned).collect();
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
                let zone = companion::zones::normalize(&m.frontmatter.zone);
                let episode = companion::zones::is_work(zone)
                    || m.frontmatter
                        .tags
                        .iter()
                        .any(|t| companion::tags::is_episode_tag(t))
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
        // 技能面收窄（规格 §6.1②），判据与 CLI/IM 同源。下面两个循环都从
        // `skills` 取，免得「整段注入」和「只广告」两条路各判一遍。
        let skills = crate::persona_scope::visible_skills(self.persona, self.all_skills);
        let always_active: Vec<&LoadedSkill> = skills
            .iter()
            .copied()
            .filter(|s| s.frontmatter.always_active)
            .collect();
        for s in always_active {
            buf.push_str(&format!("### {}\n", s.frontmatter.name));
            buf.push_str(s.body.trim());
            buf.push_str("\n\n");
        }

        if !skills.is_empty() {
            buf.push_str("## Available skills\n");
            buf.push_str(companion::skill_discovery_clause());
            buf.push('\n');
            buf.push_str(crate::persona_scope::SKILLS_ARE_NOT_STATIONS);
            buf.push('\n');
            for s in skills.iter().take(self.limits.skill_index_cap) {
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

    fn mem_in(id: &str, zone: &str, tags: &[&str], body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::User,
            Confidence::High,
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
    fn companion_not_chatbot() {
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("hello");
        assert!(s.contains("work companion"));
        assert!(!s.contains("work partner"));
        assert!(!s.contains("helpful local AI assistant"));
    }

    /// 「工作情节」的判定读**唯一**词表（`companion::tags::is_episode_tag`，自带 trim）
    /// ——Task 1.12 必修 B。本文件曾留着一份本地副本（`t == "episode"`，不 trim），
    /// 于是桌面 / 手机这条表面上，模型写成 `" episode "` 的记忆被打成 `note`。
    /// 三条 or 分支各钉一条，免得日后顺手删掉其中一条。
    #[test]
    fn an_episode_memory_is_marked_as_a_work_episode_in_the_turn_index() {
        let by_tag = mem_in(
            "mem_tag",
            "general",
            &[" episode "],
            "归档流水线 nightly 重跑改到白天",
        );
        let by_zone = mem_in("mem_zone", "work", &[], "归档流水线 nightly 重跑改到白天");
        let by_marker = mem_in(
            "mem_marker",
            "general",
            &[],
            "【工作情节】归档流水线 nightly 重跑改到白天",
        );
        let note = mem_in(
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
            pinned: &[],
            active: &active,
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
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
    fn topic_cards_replace_the_flat_index_and_pinned_stays_in_full() {
        let pin = mem("mem_p", "pinned full body", true);
        let notes = vec![mem("mem_a", "a memory body", false)];
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: Some("## Topic cards (index — NOT the wording)\n- 财经内容 (3): sum"),
            pinned: std::slice::from_ref(&pin),
            active: &notes,
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("普通问题");
        assert!(s.contains("Topic cards"), "cards should be injected");
        assert!(
            s.contains("pinned full body"),
            "pinned memory must stay in full when an index is present"
        );
        assert!(
            !s.contains("Active memory index"),
            "the flat index must not be injected alongside cards"
        );
    }

    #[test]
    fn care_nudge_on_deliverable_request() {
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
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
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
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

    /// 2026-09-15 实测的那个错：李现在的会话里没有真工位名单，模型就把技能名
    /// 「小王」当成工位指给了用户。这条把三件事钉在一起——名册进提示词、
    /// 收窄的工位看不见别人的技能、元技能仍然在。
    #[test]
    fn a_station_sees_the_real_roster_and_only_its_own_skills() {
        let me = hermes_core::persona::get("li-xian").unwrap();
        let roster = [me, hermes_core::persona::get("wang-hai-yan").unwrap()];
        let skills = [skill("memory-palace", "META"), skill("xiao-wang", "SECRET")];
        let sources = ContextSources {
            base: None,
            persona: Some(me),
            roster: &roster,
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &skills,
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("帮我采今天的情报");

        assert!(
            s.contains("- 情报王海燕 · 今天的事，今天给你"),
            "名册要按侧栏那行写，用户按这个才找得到人：\n{s}"
        );
        assert!(
            !s.contains("- 工具人李现在 · "),
            "名册是「其他工位」，不该把自己列进去：\n{s}"
        );
        assert!(
            !s.contains("xiao-wang"),
            "写了 skills 的工位只广告自己的技能——技能名被当成工位就是指路时从这儿挑的：\n{s}"
        );
        assert!(s.contains("memory-palace"), "内置元技能不许被收窄掉：\n{s}");
        assert!(
            s.contains("never offer one of these names as a place to take a task"),
            "技能索引要说清「这是技能不是工位」：\n{s}"
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
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: std::slice::from_ref(&ep),
            all_skills: std::slice::from_ref(&sk),
            open_work: &[],
            material_hits: &[],
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
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: std::slice::from_ref(&item),
            persona: None,
            roster: &[],
            material_hits: &[],
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
    fn materials_block_is_internal_and_cited() {
        let hit = hermes_core::MaterialHit {
            id: "src_x".into(),
            title: "服务合同".into(),
            excerpt: "第七条违约金百分之二十".into(),
        };
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: std::slice::from_ref(&hit),
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("违约金怎么写");
        assert!(s.contains("[lebi-AI Materials]"));
        assert!(s.contains("服务合同"));
        assert!(s.contains("百分之二十"));
        let quiet = ContextSources {
            material_hits: &[],
            ..sources
        };
        let q = quiet.build_turn_system("你好");
        assert!(!q.contains("百分之二十"), "{q}");
    }

    #[test]
    fn always_active_skill_body_is_inlined() {
        let mut active = skill("memory-palace", "ALWAYS_ACTIVE_BODY_MUST_APPEAR");
        active.frontmatter.always_active = true;
        let sources = ContextSources {
            base: None,
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: std::slice::from_ref(&active),
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("hello");
        assert!(s.contains("ALWAYS_ACTIVE_BODY_MUST_APPEAR"));
        assert!(s.contains("### memory-palace"));
    }

    #[test]
    fn the_persona_block_sits_after_the_companion_protocol_and_before_memory() {
        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let mut item =
            hermes_commitments::Commitment::new("周五交改稿", hermes_commitments::Source::User)
                .unwrap();
        item.id = "cmt_test".into();
        let pinned = mem("mem_p", "pinned body", true);
        let sources = ContextSources {
            base: Some("BASE"),
            persona: Some(p),
            roster: &[],
            topic_cards: Some("## Topic cards (index — NOT the wording)\n- 林碳 (1): sum"),
            pinned: std::slice::from_ref(&pinned),
            active: &[],
            all_skills: &[],
            open_work: std::slice::from_ref(&item),
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let s = sources.build_turn_system("今天干什么");

        let protocol = companion::companion_protocol();
        let protocol_at = s.find(&protocol).expect("搭子协议必须在");
        let mine = s.find("## 你现在是谁").expect("人物块必须在");
        let zaiban = s
            .find(hermes_core::companion::zaiban_index_clause())
            .expect("在办索引必须在");
        let base = s.find("BASE").expect("base 必须在");
        let pinned_at = s.find("## Pinned memories").expect("pinned 必须在");
        let cards = s.find("## Topic cards").expect("卡必须在");

        assert!(
            protocol_at + protocol.len() <= mine,
            "人物块只能叠加在搭子协议之后"
        );
        assert!(
            mine < zaiban && zaiban < base && base < pinned_at && pinned_at < cards,
            "顺序：人物块 → 在办 → base → 记忆与卡"
        );
        assert!(s.contains("没依据的话，我不说"));
    }

    #[test]
    fn the_persona_block_appears_exactly_once_without_a_session_layer() {
        // 这条路径**没有**独立的会话层：`build_turn_system` 就是整份提示词。
        // 所以人物块必须在这里落一次、且只落一次。
        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let sources = ContextSources {
            base: None,
            persona: Some(p),
            roster: &[],
            topic_cards: None,
            pinned: &[],
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let block = hermes_core::persona::block(p, &[]);
        // `build_turn_system` 结尾 `trim_end()`；这条路径上人物块后面没有别的
        // 内容时会连它的尾换行一起去掉——针同样去掉尾换行，只问「出现几次」。
        let needle = block.trim_end();
        assert_eq!(
            sources.build_turn_system("你好").matches(needle).count(),
            1,
            "整份提示词里只出现一次"
        );
    }

    #[test]
    fn no_persona_leaves_the_companion_prompt_byte_identical() {
        // 零迁移：`persona: None` 就是今天的提示词——插块之外的字节一个不动。
        let pinned = mem("mem_p", "pinned body", true);
        let plain = ContextSources {
            base: Some("BASE"),
            persona: None,
            roster: &[],
            topic_cards: None,
            pinned: std::slice::from_ref(&pinned),
            active: &[],
            all_skills: &[],
            open_work: &[],
            material_hits: &[],
            first_human_today: false,
            workspace_root: "/tmp/ws",
            limits: ContextLimits::default(),
        };
        let without = plain.build_turn_system("你好");
        assert!(!without.contains("你现在是谁"), "{without}");

        let p = hermes_core::persona::get("xiao-xie").unwrap();
        let with = ContextSources {
            persona: Some(p),
            roster: &[],
            ..plain
        }
        .build_turn_system("你好");
        assert_eq!(
            with.replace(&format!("{}\n", hermes_core::persona::block(p, &[])), ""),
            without,
            "有/无人物之间只差人物块那一段"
        );
    }
}
