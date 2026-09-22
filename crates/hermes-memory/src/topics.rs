//! Topic cards: a rebuildable, *derived* distillation of the active memories.
//!
//! A card is a view, never a source of truth. It says **what belongs with
//! what** and carries a short summary; the memory bodies stay authoritative for
//! wording, numbers and red lines. Merging, rebuilding or discarding cards
//! never deletes a memory — delete the card file (`topics.json` for 全局,
//! `topics-<owner>.json` for 一个专属角色) and the store is untouched.
//!
//! 归属由**文件路径**承载：全局一份，每个专属角色各一份
//! (`docs/spec/personas.md` §5.5)。卡面结构里**不**复制 owner——复制就会有两处
//! 真相，早晚打架。
//!
//! Why topics instead of the work-slot taxonomy in [`crate::slot`]: slots say
//! *how a piece of work is done* (write / lookup / close-out), which is the
//! right axis for "don't store a second peer rule", but the wrong axis for
//! "show me everything about this subject". Cards use the subject axis.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::memory::{normalize_owner, LoadedMemory};

/// Hard cap on how many cards one build may keep. Beyond this the card list
/// stops being a map and becomes another pile.
pub const MAX_CARDS: usize = 8;

/// A card this size can no longer be summarised honestly; a rebuild should
/// split it rather than grow the summary.
pub const SPLIT_THRESHOLD: usize = 12;

/// Title of the card that catches memories no theme claimed. Guarantees every
/// active memory is referenced by some card, which is what makes staleness a
/// clean question ("is anything newer than the cards?").
pub const UNGROUPED_TITLE: &str = "未归类";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicCard {
    pub id: String,
    pub title: String,
    pub summary: String,
    /// Memory ids this card points at. References, not ownership: the same
    /// memory may appear on more than one card (cross-cutting rules such as
    /// delivery format belong to several subjects at once).
    #[serde(default)]
    pub members: Vec<String>,
    /// RFC3339. Inherited across merges so the UI can say when it was built.
    pub built_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicCards {
    #[serde(default)]
    pub cards: Vec<TopicCard>,
}

impl TopicCards {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Every memory id referenced by any card.
    pub fn member_ids(&self) -> HashSet<&str> {
        self.cards
            .iter()
            .flat_map(|c| c.members.iter().map(String::as_str))
            .collect()
    }
}

/// 卡文件的路径。`None` = 全局那份 `topics.json`；`Some(id)` = 该专属角色的
/// `topics-<id>.json`（两个自带角色用全局那份，不额外生成一套卡——调用方经
/// `Persona::memory_owner()` 传 `None`，这里不认识 `builtin`）。
///
/// `owner` 先过 [`normalize_owner`]：空白 = 全局，首尾空白归到同一个人，
/// 否则 `Some(" xiao-xie ")` 会自己开一份谁都读不到的文件。
pub fn path(owner: Option<&str>) -> Result<PathBuf> {
    Ok(match normalize_owner(owner) {
        None => hermes_core::data_path("topics.json"),
        Some(owner) => hermes_core::data_path(format!("topics-{owner}.json")),
    })
}

/// Read **one** scope's card file. 「文件不存在」与「空文件」都当「还没建过卡」
/// 返回空卡；**读不动或解析失败是真错误**（`Err`），由调用方决定怎么处置
/// （面板与 CLI 目前用 `.unwrap_or_default()` 吞掉 → 显示成「还没整理过」，
/// 只留一条 `tracing::warn`）。
///
/// 「这个视图该看到哪些卡」不是这一层的事——见 [`cards_for_view_any`]：全局那份是
/// 所有视图共用的索引，人物视图在它之后接上自己那份。
pub fn load(owner: Option<&str>) -> Result<TopicCards> {
    let path = path(owner)?;
    if !path.exists() {
        return Ok(TopicCards::default());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(TopicCards::default());
    }
    let cards: TopicCards =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(cards)
}

pub fn save(owner: Option<&str>, cards: &TopicCards) -> Result<PathBuf> {
    let path = path(owner)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cards).context("serialising topic cards")?;
    let tmp = tmp_path(&path);
    std::fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

/// 临时名跟着目标文件名走：多份卡各自落盘时共用一个 `.topics.json.tmp`
/// 会互相踩（写到一半被另一个 `rename` 搬走）。
fn tmp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("topics.json");
    path.with_file_name(format!(".{name}.tmp"))
}

/// Drop references to memories that are no longer active, then drop cards left
/// empty. Cards shrink on their own; nothing here touches memory files.
pub fn prune(cards: &TopicCards, active: &[LoadedMemory]) -> TopicCards {
    let live: HashSet<&str> = active.iter().map(|m| m.id()).collect();
    prune_to(cards, &live)
}

/// [`prune`] 的 id 版：调用方手上已经是「这个视图看得见谁」的集合时走这条
/// （`render_from_disk` 每一轮都要用，省掉一次克隆）。
fn prune_to(cards: &TopicCards, live: &HashSet<&str>) -> TopicCards {
    let cards = cards
        .cards
        .iter()
        .map(|c| TopicCard {
            members: {
                let mut seen = HashSet::new();
                c.members
                    .iter()
                    .filter(|id| live.contains(id.as_str()))
                    .filter(|id| seen.insert((**id).clone()))
                    .cloned()
                    .collect()
            },
            ..c.clone()
        })
        .filter(|c| !c.members.is_empty())
        .collect();
    TopicCards { cards }
}

/// Active memories no card points at — the input to a merge, and the signal
/// that the card set has fallen behind.
pub fn unassigned<'a>(cards: &TopicCards, active: &'a [LoadedMemory]) -> Vec<&'a LoadedMemory> {
    let known = cards.member_ids();
    active.iter().filter(|m| !known.contains(m.id())).collect()
}

/// Cards exist but no longer cover everything active. Only meaningful once
/// something has been built — "nothing built yet" is the empty state, not stale.
pub fn is_stale(cards: &TopicCards, active: &[LoadedMemory]) -> bool {
    !cards.is_empty() && !unassigned(cards, active).is_empty()
}

/// The single call every prompt surface uses: read **this view's** cards, drop
/// references to memories that are gone (or that this view cannot see), and
/// render. `None` means "nothing built yet" — callers fall back to a plain
/// memory index.
///
/// `owner` 是**当前视图**的人物 id（语义同 [`crate::visible_to`]）。`active`
/// 一般是调用方**没有**过滤过的全量列表：这里自己过一遍 [`crate::filter_visible`]，
/// 所以调用方漏了过滤也不会把别人的卡渲进这个视图。可见性判定仍然只有 `visible_to`
/// 一处，这里只是**调用**它。
///
/// 过滤粒度是**成员级**（`prune_to`）：一条成员看不见就从成员里去掉；只有
/// **所有**成员都被去掉时卡才消失。也就是说「卡里混了一条你看不见的记忆」时
/// 卡会留在这一视图里，而它的 `summary` 是当初建卡时生成的——按构造路径
/// （记忆与卡都按同一可见集合切）这产不出跨归属成员的卡，只有归属漂移或手改
/// 文件才可能；真要收紧得让 `prune_to` 同时知道「live 全集」与「可见集」，
/// 别在这里临时加「缺人就丢卡」（那会让任何一次记忆删除都炸掉卡）。
pub fn render_from_disk(active: &[LoadedMemory], owner: Option<&str>) -> Option<String> {
    match owner {
        Some(o) => render_from_disk_any(active, &[o]),
        None => render_from_disk_any(active, &[]),
    }
}

/// 一组归属的卡视图（组会话：全局那份 + 本项目组 + 这一轮说话的人）。
///
/// 判据仍是 `visible_to`（经 `filter_visible_any`）——「谁看得见什么」只有那一处。
pub fn render_from_disk_any(active: &[LoadedMemory], owners: &[&str]) -> Option<String> {
    let view = crate::memory::filter_visible_any(active, owners);
    let live: HashSet<&str> = view.iter().map(|m| m.id()).collect();
    let pruned = prune_to(&cards_for_view_any(owners), &live);
    if pruned.is_empty() {
        None
    } else {
        Some(render_for_prompt(&pruned))
    }
}

/// 这个视图该看到的卡：**全局那份 + 自己那份**。
///
/// 全局那份是所有视图共用的索引——全局记忆本来就谁都看得见，没有理由把它藏起来；
/// 而且它让「整理」只需要为自己的私有记忆建卡：不必把全局记忆在每个角色里各蒸一遍
/// （token × 人物数），也不会把共享口径切碎挤进 `MAX_CARDS`。
///
/// 同 id 时**以自己那份为准**：那份是在「全局 + 本人」的可见面上重新整理出来的。
/// 读不动（文件损坏）按「没有卡」处理 + 一条 warn：卡是派生视图，读不动不该让会话
/// 起不来，但静默失败也不该无声无息。
/// 多个归属的卡：**全局那份 + 每个归属各一份**，同 id 后面那份覆盖前面那份
/// （越靠后越「自己」：组在说话人之前，说话人那份更贴他这一刻的活）。
fn cards_for_view_any(owners: &[&str]) -> TopicCards {
    let mut cards = match load(None) {
        Ok(cards) => cards,
        Err(e) => {
            tracing::warn!(error = %e, "reading the global topic cards");
            TopicCards::default()
        }
    };
    for owner in owners {
        let owner = owner.trim();
        if owner.is_empty() {
            continue;
        }
        match load(Some(owner)) {
            Ok(own) => {
                for c in own.cards {
                    cards.cards.retain(|g| g.id != c.id);
                    cards.cards.push(c);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, owner = %owner, "reading this scope's topic cards")
            }
        }
    }
    cards
}

/// Render the cards for a system prompt.
///
/// Deliberately does **not** list member ids: there is no fetch-by-id tool, so
/// ids would be tokens spent on nothing. The card title is the retrieval key —
/// the model is told to search with it.
pub fn render_for_prompt(cards: &TopicCards) -> String {
    let mut buf = String::from("## Topic cards (index — NOT the wording)\n");
    buf.push_str(
        "These cards group memories by subject. They are summaries: for anything about exact \
wording, numbers, red lines, sources or delivery rules, load the memories first \
(`memory_search` with the card's subject, or `palace_read_zone`) and treat the memory text as \
authoritative if it disagrees with a card.\n",
    );
    for c in &cards.cards {
        let summary = c.summary.replace('\n', " · ");
        buf.push_str(&format!(
            "- {} ({}): {}\n",
            c.title,
            c.members.len(),
            summary
        ));
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use crate::test_env::with_data_dir;

    fn mem(id: &str, body: &str) -> LoadedMemory {
        mem_owned(id, body, None)
    }

    /// 归属写在 frontmatter 上，卡面只引用 id——所以一个测试要同时用到两种归属。
    fn mem_owned(id: &str, body: &str, owner: Option<&str>) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::User,
            Confidence::High,
            vec![],
            "general".to_string(),
        )
        .owned(owner.map(str::to_string));
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn each_owner_gets_its_own_card_file() {
        assert!(path(None).unwrap().ends_with("topics.json"));
        assert!(path(Some("xiao-xie"))
            .unwrap()
            .ends_with("topics-xiao-xie.json"));
        // 归一化只有一份（`memory::normalize_owner`）：空白 = 全局，首尾空白 = 同一个人，
        // 否则 `Some(" xiao-xie ")` 会自己开一个谁都读不到的文件。
        assert!(path(Some("  ")).unwrap().ends_with("topics.json"));
        assert!(path(Some(" xiao-xie "))
            .unwrap()
            .ends_with("topics-xiao-xie.json"));
    }

    #[test]
    fn a_persona_view_gets_the_global_cards_plus_its_own() {
        with_data_dir(|root| {
            let global = mem_owned("mem_global", "交付要 Word 放桌面", None);
            let xie = mem_owned("mem_xie", "林碳报告只引 IEA", Some("xiao-xie"));
            save(
                None,
                &TopicCards {
                    cards: vec![card("tc_global", "交付习惯", &[global.id()])],
                },
            )
            .unwrap();
            save(
                Some("xiao-xie"),
                &TopicCards {
                    cards: vec![card("tc_xie", "林碳口径", &[xie.id()])],
                },
            )
            .unwrap();

            // 两份卡各在一份文件里，互不覆盖。
            assert!(root.join("topics.json").exists());
            assert!(root.join("topics-xiao-xie.json").exists());

            let active = vec![global, xie];
            let view = render_from_disk(&active, Some("xiao-xie")).unwrap();
            assert!(view.contains("林碳口径"), "{view}");
            assert!(
                view.contains("交付习惯"),
                "全局那份是所有视图共用的索引，人物视图必须接上它：{view}"
            );
            let globals = render_from_disk(&active, None).unwrap();
            assert!(!globals.contains("林碳口径"), "全局那份卡不该看到小谢的卡");
        });
    }

    #[test]
    fn a_scope_without_its_own_cards_sees_the_globals_and_never_another_scope() {
        with_data_dir(|_| {
            let global = mem_owned("mem_global", "交付要 Word 放桌面", None);
            let wang = mem_owned("mem_wang", "海燕选题只看政策原文", Some("wang-hai-yan"));
            save(
                None,
                &TopicCards {
                    cards: vec![card("tc_global", "交付习惯", &[global.id()])],
                },
            )
            .unwrap();
            save(
                Some("wang-hai-yan"),
                &TopicCards {
                    cards: vec![card("tc_wang", "海燕口径", &[wang.id()])],
                },
            )
            .unwrap();

            let active = vec![global, wang];
            // 没整理过的小谢：全局那份索引照给（全局记忆他本来就看得到），
            // 但别人物的卡一张都不许冒出来。
            let xie = render_from_disk(&active, Some("xiao-xie")).unwrap();
            assert!(xie.contains("交付习惯"), "{xie}");
            assert!(!xie.contains("海燕口径"), "别人物的卡不许冒出来：{xie}");
            assert!(render_from_disk(&active, None).is_some());
        });
    }

    /// 读侧自己也要挡一道：卡面摘要会直接进提示词，所以「这张卡引用的记忆这个视图
    /// 看不见」时必须被剪掉——不能指望构建期永远不出错。这里是**成员级**过滤：
    /// 这张卡只有一个成员且它不可见 → 成员清空 → 整张卡消失（见 `render_from_disk`
    /// 文档：成员部分可见时卡会留下，那是已知的窄残留面）。
    #[test]
    fn a_card_pointing_at_an_invisible_memory_is_dropped() {
        with_data_dir(|_| {
            let wang = mem_owned("mem_wang", "海燕选题只看政策原文", Some("wang-hai-yan"));
            // 海燕自己那份：引用的正是他自己的记忆 → 对他活着。
            save(
                Some("wang-hai-yan"),
                &TopicCards {
                    cards: vec![card("tc_wang", "海燕口径", &[wang.id()])],
                },
            )
            .unwrap();
            // 小谢那份：故意引用海燕的记忆（构建期不该发生，读侧必须自己挡住）。
            save(
                Some("xiao-xie"),
                &TopicCards {
                    cards: vec![card("tc_leak", "不该出现的卡", &[wang.id()])],
                },
            )
            .unwrap();

            let active = vec![wang];
            assert!(
                render_from_disk(&active, Some("wang-hai-yan")).is_some(),
                "海燕自己的卡对他自己是活的"
            );
            assert!(
                render_from_disk(&active, Some("xiao-xie")).is_none(),
                "卡里唯一的成员看不见 → 剪光成员后整张卡消失"
            );
        });
    }

    /// 合并的优先级：同一个卡 id 两边都有时，以自己那份为准。
    #[test]
    fn the_own_card_wins_when_card_ids_collide() {
        with_data_dir(|_| {
            let global = mem_owned("mem_global", "交付要 Word 放桌面", None);
            let xie = mem_owned("mem_xie", "林碳报告只引 IEA", Some("xiao-xie"));
            save(
                None,
                &TopicCards {
                    cards: vec![card("tc_same", "全局的旧版本", &[global.id()])],
                },
            )
            .unwrap();
            save(
                Some("xiao-xie"),
                &TopicCards {
                    cards: vec![card("tc_same", "小谢整理过的新版本", &[xie.id()])],
                },
            )
            .unwrap();

            let active = vec![global, xie];
            let view = render_from_disk(&active, Some("xiao-xie")).unwrap();
            assert!(view.contains("小谢整理过的新版本"), "{view}");
            assert!(
                !view.contains("全局的旧版本"),
                "同 id 以自己那份为准：{view}"
            );
        });
    }

    fn card(id: &str, title: &str, members: &[&str]) -> TopicCard {
        TopicCard {
            id: id.into(),
            title: title.into(),
            summary: "sum".into(),
            members: members.iter().map(|s| s.to_string()).collect(),
            built_at: "2026-09-14T00:00:00Z".into(),
        }
    }

    #[test]
    fn prune_drops_dead_members_and_empty_cards() {
        let cards = TopicCards {
            cards: vec![card("t1", "A", &["m1", "m2"]), card("t2", "B", &["m3"])],
        };
        let active = vec![mem("m1", "one"), mem("m3", "three")];
        let pruned = prune(&cards, &active);
        assert_eq!(pruned.cards.len(), 2);
        assert_eq!(pruned.cards[0].members, vec!["m1"]);
        assert_eq!(pruned.cards[1].members, vec!["m3"]);
    }

    #[test]
    fn prune_drops_card_whose_members_all_died() {
        let cards = TopicCards {
            cards: vec![card("t1", "A", &["gone"])],
        };
        let active = vec![mem("m1", "one")];
        assert!(prune(&cards, &active).is_empty());
    }

    #[test]
    fn unassigned_finds_new_memories() {
        let cards = TopicCards {
            cards: vec![card("t1", "A", &["m1"])],
        };
        let active = vec![mem("m1", "one"), mem("m2", "two")];
        let left = unassigned(&cards, &active);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id(), "m2");
        assert!(is_stale(&cards, &active));
    }

    #[test]
    fn nothing_built_is_not_stale() {
        let active = vec![mem("m1", "one")];
        assert!(!is_stale(&TopicCards::default(), &active));
    }

    #[test]
    fn a_memory_may_be_referenced_by_two_cards() {
        let cards = TopicCards {
            cards: vec![card("t1", "A", &["m1"]), card("t2", "B", &["m1"])],
        };
        let active = vec![mem("m1", "one")];
        assert!(!is_stale(&cards, &active));
        assert_eq!(cards.member_ids().len(), 1);
    }

    #[test]
    fn prompt_lists_cards_without_member_ids() {
        let cards = TopicCards {
            cards: vec![card("t1", "财经内容", &["mem_secret_id"])],
        };
        let out = render_for_prompt(&cards);
        assert!(out.contains("财经内容"));
        assert!(!out.contains("mem_secret_id"));
    }
}
