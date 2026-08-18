//! Living-rule slots: one kind of work → one active memory.
//!
//! A durable memory is a rule that still holds next time without the user
//! repeating it. Session diaries, empty shells, tool-environment notes, and
//! "today I don't want to work" are not rules.

use crate::memory::LoadedMemory;

/// How a piece of work is done (not a topic, not a session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkSlot {
    /// 写 / 改成品：短句、先结论、口吻、结构。
    WriteDeliverable,
    /// 查此刻的公开事实：有效源，不是搜索日志。
    LookupFacts,
    /// 排优先级 / 决策怎么收。
    Prioritize,
    /// 做完怎么交：对话展示、Word、桌面。
    CloseOut,
    /// 对人说话的分寸：简洁、接梗。
    Tone,
    /// 稳定身份/角色（须用户认过）：号、岗位。
    Identity,
    /// 跨任务方法论：先找目的地再出发。
    WorkMethod,
}

impl WorkSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WriteDeliverable => "write-deliverable",
            Self::LookupFacts => "lookup-facts",
            Self::Prioritize => "prioritize",
            Self::CloseOut => "close-out",
            Self::Tone => "tone",
            Self::Identity => "identity",
            Self::WorkMethod => "work-method",
        }
    }
}

/// Infer the work slot from body (and optional zone/tags). `None` = uncategorized.
pub fn infer_slot(zone: &str, tags: &[String], body: &str) -> Option<WorkSlot> {
    let t = format!("{} {} {}", zone, tags.join(" "), body).to_lowercase();
    let raw = format!("{} {}", zone, body);

    if is_ephemeral_state(&raw) || is_environment_diary(&raw) {
        return None;
    }

    // Close-out before write: "不要默认落盘 / 放到桌面" is how you hand off.
    if contains_any(
        &t,
        &[
            "放到桌面",
            "放桌面",
            "写成word",
            "存成 word",
            "生成 word",
            "不要默认落盘",
            "对话里展示",
            "直接在对话里",
        ],
    ) && !contains_any(&t, &["短句", "先结论", "标题三原则", "犀利"])
    {
        return Some(WorkSlot::CloseOut);
    }

    if contains_any(
        &t,
        &[
            "短句",
            "先结论",
            "犀利",
            "标题三原则",
            "只讲透",
            "写作思路",
            "写作结构",
            "点名更狠",
        ],
    ) {
        return Some(WorkSlot::WriteDeliverable);
    }

    if contains_any(
        &t,
        &[
            "热榜",
            "热点",
            "tophub",
            "有效源",
            "先 fetch",
            "天气",
            "汇率",
        ],
    ) {
        return Some(WorkSlot::LookupFacts);
    }

    if contains_any(&t, &["优先级", "只要三件", "先卡点", "怎么选"]) {
        return Some(WorkSlot::Prioritize);
    }

    if contains_any(&t, &["简洁的回答", "接梗", "幽默", "不要冗长铺垫"]) {
        return Some(WorkSlot::Tone);
    }

    if contains_any(&raw, &["普法号", "微信公众号是", "法务部门", "内容号"])
        && !contains_any(&t, &["写作思路", "短句"])
    {
        return Some(WorkSlot::Identity);
    }

    if contains_any(&t, &["目的地", "先找好", "洗车"]) {
        return Some(WorkSlot::WorkMethod);
    }

    None
}

/// Must not be injected, listed as living, or enqueued.
pub fn is_worthless_for_living(body: &str) -> bool {
    let f = body.trim();
    if f.is_empty() || f.chars().count() < 8 {
        return true;
    }
    if f.ends_with('：')
        || f.ends_with(':')
        || f.contains("工作场景：。")
        || f.contains("工作场景：") && f.chars().count() < 16
    {
        return true;
    }
    if f.contains("见会话记录")
        || f.contains("见该会话")
        || f.contains("以当时对话中的实际操作为准")
        || f.contains("若有文件路径以对话中写明的为准")
        || f.contains("写入记忆时以本条为准（不依赖原会话文件）")
    {
        return true;
    }
    if is_ephemeral_state(f) || is_environment_diary(f) {
        return true;
    }
    if is_echo_template(f) {
        return true;
    }
    false
}

fn is_ephemeral_state(f: &str) -> bool {
    let t = f.to_lowercase();
    (t.contains("今天") || t.contains("当日") || t.contains("today"))
        && (t.contains("不想工作")
            || t.contains("不想上班")
            || t.contains("休息")
            || t.contains("摸鱼"))
        || t.contains("正在验收权限")
}

fn is_environment_diary(f: &str) -> bool {
    let t = f.to_lowercase();
    t.contains("python-docx")
        || t.contains("sandbox")
        || t.contains("seatbelt")
        || t.contains("pandoc")
        || (t.contains("本机") && (t.contains("已安装") || t.contains("环境")))
        || t.contains("web_fetch 有反爬")
        || t.contains("改用内置 web_search")
}

/// 【工作情节】 where 情境/做法/可复用点 repeat the same user utterance.
fn is_echo_template(f: &str) -> bool {
    if !f.contains("【工作情节】") {
        return false;
    }
    let lines: Vec<&str> = f.lines().collect();
    let title = lines
        .first()
        .unwrap_or(&"")
        .trim()
        .trim_start_matches("【工作情节】")
        .trim();
    if title.is_empty() {
        return true;
    }
    let mut repeats = 0;
    for line in &lines {
        let v = line
            .split('：')
            .nth(1)
            .or_else(|| line.split(':').nth(1))
            .unwrap_or("")
            .trim();
        if !v.is_empty() && (v == title || v.contains(title) || title.contains(v)) {
            repeats += 1;
        }
    }
    repeats >= 2
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Drop worthless bodies; for each inferred slot keep one (newest, then higher confidence).
pub fn living_rules(memories: Vec<LoadedMemory>) -> Vec<LoadedMemory> {
    let mut unslotted = Vec::new();
    let mut by_slot: Vec<(WorkSlot, LoadedMemory)> = Vec::new();

    for m in memories {
        if is_worthless_for_living(&m.body) {
            continue;
        }
        match infer_slot(&m.frontmatter.zone, &m.frontmatter.tags, &m.body) {
            None => unslotted.push(m),
            Some(slot) => {
                if let Some(pos) = by_slot.iter().position(|(s, _)| *s == slot) {
                    if should_replace(&by_slot[pos].1, &m) {
                        by_slot[pos] = (slot, m);
                    }
                } else {
                    by_slot.push((slot, m));
                }
            }
        }
    }

    let mut out: Vec<LoadedMemory> = by_slot.into_iter().map(|(_, m)| m).collect();
    out.extend(unslotted);
    out
}

fn should_replace(old: &LoadedMemory, new: &LoadedMemory) -> bool {
    use std::cmp::Ordering;
    match new.frontmatter.confidence.cmp(&old.frontmatter.confidence) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => new.frontmatter.created >= old.frontmatter.created,
    }
}

/// Existing active memory ids that occupy the same slot as `body`.
pub fn same_slot_ids(
    active: &[LoadedMemory],
    zone: &str,
    tags: &[String],
    body: &str,
) -> Vec<String> {
    let Some(slot) = infer_slot(zone, tags, body) else {
        return Vec::new();
    };
    active
        .iter()
        .filter(|m| {
            !is_worthless_for_living(&m.body)
                && infer_slot(&m.frontmatter.zone, &m.frontmatter.tags, &m.body) == Some(slot)
        })
        .map(|m| m.id().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, zone: &str, body: &str, conf: Confidence) -> LoadedMemory {
        LoadedMemory {
            frontmatter: MemoryFrontmatter {
                id: id.into(),
                created: chrono::Utc::now(),
                source: Source::Reflection,
                confidence: conf,
                pinned: false,
                tags: vec![],
                zone: zone.into(),
                supersedes: vec![],
                extra: Default::default(),
            },
            body: body.into(),
            source_path: PathBuf::from("x.md"),
            scope: Scope::User,
        }
    }

    #[test]
    fn worthless_empty_scene_and_echo() {
        assert!(is_worthless_for_living("用户的工作场景：。"));
        assert!(is_worthless_for_living(
            "【工作情节】把文章，写成word；放在我的电脑桌面上\n- 情境：用户在本轮表达的工作意图：把文章，写成word；放在我的电脑桌面上\n- 做法：以当时对话中的实际操作为准（要点已写入本条，不依赖会话文件）\n- 产出：若有文件路径以对话中写明的为准；否则以本条意图为可检索摘要\n- 用户反馈/修正：无\n- 可复用点：把文章，写成word；放在我的电脑桌面上"
        ));
        assert!(is_worthless_for_living(
            "用户今天（当日）不想工作，处于休息/摸鱼状态"
        ));
        assert!(is_worthless_for_living(
            "本机 Python 环境已安装 python-docx 1.2.0"
        ));
        assert!(!is_worthless_for_living(
            "用户偏好写文档时使用短句、先结论后细节的写作结构。"
        ));
    }

    #[test]
    fn infer_write_and_closeout() {
        assert_eq!(
            infer_slot("work", &[], "用户偏好写文档时使用短句、先结论后细节"),
            Some(WorkSlot::WriteDeliverable)
        );
        assert_eq!(
            infer_slot(
                "preferences",
                &[],
                "生成的文案直接在对话里展示，不要默认落盘"
            ),
            Some(WorkSlot::CloseOut)
        );
        assert_eq!(
            infer_slot("general", &[], "用户的微信公众号是法律/普法方向的内容号。"),
            Some(WorkSlot::Identity)
        );
    }

    #[test]
    fn living_keeps_one_write_slot() {
        let a = mem(
            "mem_old",
            "general",
            "用户偏好写文档时使用短句、先结论后细节的写作结构。",
            Confidence::Medium,
        );
        let mut b = mem(
            "mem_new",
            "work",
            "用户认可犀利观点风、点名更狠，写科技稿按此执行。",
            Confidence::High,
        );
        b.frontmatter.created += chrono::Duration::seconds(10);
        let out = living_rules(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id(), "mem_new");
    }
}
