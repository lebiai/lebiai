//! 长会话：按天分层 + 最近窗口 + 本会话翻旧账。
//!
//! 一条工位会话要日复一日地往下走，所以它必然越来越长。分层只有一个判据：
//! **一天从「人说的第一句话」开始**——用户消息带 `at` 的那一刻（**本地**日）。
//! 助手回复、工具结果、引擎提示都跟着它们所在的那一轮走。〔`docs/spec/projects.md` §4.4〕
//!
//! 两个不变量：
//! 1. 窗口与「编辑重发」的截断吃**同一个数组、同一个下标**——
//!    [`window_split`] 返回的 `base` 就是界面要带回来的偏移，不许另算一份。
//! 2. 短会话（不超过一个窗口）**逐条不变**：`base = 0`、没有任何折叠组。

use hermes_core::{Message, Role, SessionEvent};

/// 界面默认展开的最近条数。比一天略少一点：翻旧账最多点一次。
pub const DEFAULT_WINDOW: usize = 80;

/// 一条会话里的一天（或开头那段没有日期的「更早」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayGroup {
    /// 本地日期 `YYYY-MM-DD`；`None` = 这一组里的消息没有日期（老文件、压缩摘要）。
    pub day: Option<String>,
    /// 界面上写的那一行：`9 月 16 日` / `更早`。
    pub label: String,
    /// 这一组里「人说了几句」。
    pub turns: usize,
    /// 这一组在会话数组里的起点。
    pub from: usize,
    /// 这一组里有多少条消息。
    pub messages: usize,
}

/// 这一条消息是不是「人在说话」——只有这种消息才开新的一轮。
///
/// 工具结果、Care 提示、压缩摘要都**不算**：它们附着在某一轮上。
pub fn is_human_turn(m: &Message) -> bool {
    if m.role != Role::User || m.is_internal_instruction_only() {
        return false;
    }
    m.content.iter().any(|b| match b {
        hermes_core::ContentBlock::Text { text } => {
            let t = text.trim();
            !t.is_empty() && !hermes_core::compaction::is_summary_text(t)
        }
        _ => false,
    })
}

/// 这一轮属于哪一天（本地日）。没有 `at` 的老消息 → `None`。
pub fn day_of(m: &Message) -> Option<String> {
    m.at.map(|at| {
        at.with_timezone(&chrono::Local)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    })
}

/// `2026-09-16` → `9 月 16 日`。认不出来就原样返回（不静默丢信息）。
pub fn label_for(day: &str) -> String {
    let mut parts = day.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_y), Some(m), Some(d)) => {
            let mm = m.trim_start_matches('0');
            let dd = d.trim_start_matches('0');
            if mm.is_empty() || dd.is_empty() {
                day.to_string()
            } else {
                format!("{mm} 月 {dd} 日")
            }
        }
        _ => day.to_string(),
    }
}

/// 把一段会话切成一天一组。**连着的无日期消息合成一组**（都叫「更早」）。
pub fn day_groups(messages: &[Message]) -> Vec<DayGroup> {
    let mut out: Vec<DayGroup> = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        if !is_human_turn(m) {
            continue;
        }
        let day = day_of(m);
        let joins_prev = match (out.last(), &day) {
            (Some(g), Some(d)) => g.day.as_deref() == Some(d.as_str()),
            (Some(g), None) => g.day.is_none(),
            (None, _) => false,
        };
        if !joins_prev {
            out.push(DayGroup {
                day: day.clone(),
                label: day
                    .as_deref()
                    .map(label_for)
                    .unwrap_or_else(|| "更早".into()),
                turns: 0,
                from: i,
                messages: 0,
            });
        }
        if let Some(g) = out.last_mut() {
            g.turns += 1;
        }
    }
    for idx in 0..out.len() {
        let to = out.get(idx + 1).map(|g| g.from).unwrap_or(messages.len());
        out[idx].messages = to - out[idx].from;
    }
    out
}

/// 最近窗口的切法：返回 `(base, 更早的按天分组)`。
///
/// `base` = 保持展开的起点（**会话数组里的下标**，不是窗口内的下标）。界面把它
/// 原样带回来，「编辑重发」的截断就是 `窗口内下标 + base`——判据只有这一处。
pub fn window_split(messages: &[Message], window: usize) -> (usize, Vec<DayGroup>) {
    let base = messages.len().saturating_sub(window);
    if base == 0 {
        return (0, Vec::new());
    }
    (base, day_groups(&messages[..base]))
}

/// 翻旧账取回某一天：判据与 [`window_split`] **同一刀**。
///
/// 只在窗口**之前**那些组里找 —— 折叠条上写的条数就是这里返回的条数，
/// 返回区间也必定落在窗口之前，不可能和界面上已经展开的最近窗口重叠。
/// `None` = 这一天不在折叠区（可能整段落在最近窗口里，也可能不存在）。
///
/// 修复的事故（`docs/records/20260918-reaudit.md` P1-3）：以前界面拿
/// `window_split` 的**前缀**分组画折叠条，点开时后端却对**全量**消息重新分组，
/// 于是同一天的组往后长进了最近窗口 —— 点一次「9 月 15 日」会取回 80 条
/// 用户已经看见的消息。判据只此一处，越界在类型上就不可能。
pub fn window_day(messages: &[Message], window: usize, day: Option<&str>) -> Option<DayGroup> {
    let (_, groups) = window_split(messages, window);
    groups.into_iter().find(|g| g.day.as_deref() == day)
}

/// 一次翻旧账的命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallHit {
    /// 命中所在的天（本地日）；没有日期的老消息 → `None`。
    pub day: Option<String>,
    /// 这条消息在会话文件里的序号（第几条 message 事件，从 0 起）。
    pub index: usize,
    /// `user` / `assistant`。
    pub role: &'static str,
    /// 命中的正文（已截断）。
    pub text: String,
}

const HIT_MAX: usize = 400;

/// 在本会话里翻旧账。
///
/// **读文件**，所以被上下文压缩换掉的原文也在这里（这正是它存在的理由——
/// 压缩是有损的，旧账不能跟着丢）。匹配是大小写无关的子串；整句不中时，
/// 退一步要求以空白切开的每个词都出现。
pub fn recall_in_session(
    path: impl AsRef<std::path::Path>,
    query: &str,
    limit: usize,
) -> crate::session::Result<Vec<RecallHit>> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let needle = query.to_lowercase();
    let terms: Vec<String> = needle
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();

    let path = path.as_ref();
    let text =
        std::fs::read_to_string(path).map_err(|source| crate::session::SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    let mut hits = Vec::new();
    let mut day: Option<String> = None;
    let mut index = 0usize;
    // 翻旧账和读会话走同一套解析：历史遗留的半条事件也要能翻出来，
    // 不然「更早的日子」永远少几句。
    for event in crate::session::read_events(text.as_bytes(), path)? {
        let SessionEvent::Message(m) = event else {
            continue;
        };
        if is_human_turn(&m) && m.at.is_some() {
            day = day_of(&m);
        }
        let body: String = m
            .content
            .iter()
            .filter_map(|b| match b {
                hermes_core::ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let body = body.trim();
        if !body.is_empty() && matches_query(&body.to_lowercase(), &needle, &terms) {
            hits.push(RecallHit {
                day: day.clone(),
                index,
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                text: truncate(body, HIT_MAX),
            });
            if hits.len() >= limit {
                break;
            }
        }
        index += 1;
    }
    Ok(hits)
}

fn matches_query(lowered: &str, needle: &str, terms: &[String]) -> bool {
    if lowered.contains(needle) {
        return true;
    }
    terms.len() > 1 && terms.iter().all(|t| lowered.contains(t.as_str()))
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::{ContentBlock, Message, Role};

    fn user_at(text: &str, at: Option<&str>) -> Message {
        let mut m = Message::user_text(text);
        m.at = at.map(|s| s.parse().unwrap());
        m
    }

    fn assistant(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            at: None,
            speaker: None,
        }
    }

    #[test]
    fn a_day_is_cut_by_what_the_person_said_not_by_the_reply() {
        let msgs = vec![
            user_at("早上好", Some("2026-09-16T01:00:00Z")),
            assistant("早"),
            user_at("再来一条", Some("2026-09-16T09:00:00Z")),
            assistant("好"),
        ];
        let groups = day_groups(&msgs);
        assert_eq!(groups.len(), 1, "同一天只有一组");
        assert_eq!(groups[0].turns, 2, "两轮");
        assert_eq!(groups[0].messages, 4, "助手回复跟着那一轮走");
        assert_eq!(groups[0].from, 0);
    }

    #[test]
    fn two_days_make_two_groups_and_undated_old_messages_go_to_earlier() {
        let msgs = vec![
            user_at("很久以前说过的一句", None),
            assistant("嗯"),
            user_at("昨天说的", Some("2026-09-16T02:00:00Z")),
            assistant("好"),
            user_at("今天说的", Some("2026-09-17T02:00:00Z")),
            assistant("好"),
        ];
        let groups = day_groups(&msgs);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].day, None);
        assert_eq!(groups[0].label, "更早");
        assert_eq!(groups[0].from, 0);
        assert_eq!(groups[0].messages, 2, "助手回复跟着「更早」那一轮");
        assert_eq!(groups[1].label, "9 月 16 日");
        assert_eq!(groups[2].label, "9 月 17 日");
    }

    /// 连着的无日期消息合成一组——不然后面会冒出一串「更早」。
    #[test]
    fn undated_turns_never_split_into_several_earlier_groups() {
        let msgs = vec![
            user_at("第一句", None),
            assistant("a"),
            user_at("第二句", None),
            assistant("b"),
        ];
        let groups = day_groups(&msgs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].turns, 2);
        assert_eq!(groups[0].messages, 4);
    }

    /// 压缩摘要不是人说的话：它不能自己开一轮。
    #[test]
    fn a_compaction_summary_does_not_start_a_turn() {
        let mut summary = Message::user_text(format!(
            "{} 之前的对话被压缩了",
            hermes_core::compaction::SUMMARY_PREFIX
        ));
        summary.at = Some("2026-09-17T03:00:00Z".parse().unwrap());
        let msgs = vec![
            summary,
            assistant("继续"),
            user_at("继续干", Some("2026-09-17T04:00:00Z")),
        ];
        let groups = day_groups(&msgs);
        assert_eq!(groups.len(), 1, "摘要不开口，只有那一句人话算一轮");
        assert_eq!(groups[0].turns, 1);
    }

    #[test]
    fn a_short_session_has_no_folds_at_all() {
        let msgs: Vec<Message> = (0..5)
            .map(|i| user_at(&format!("第 {i} 句"), Some("2026-09-17T04:00:00Z")))
            .collect();
        let (base, days) = window_split(&msgs, DEFAULT_WINDOW);
        assert_eq!(base, 0);
        assert!(days.is_empty(), "短会话一条都不折叠");
    }

    #[test]
    fn the_window_keeps_the_tail_and_folds_everything_before_it() {
        let mut msgs = Vec::new();
        for day in ["2026-09-16", "2026-09-17"] {
            for i in 0..50 {
                msgs.push(user_at(
                    &format!("{day} 第 {i} 句"),
                    Some(&format!("{day}T0{}:00:00Z", i % 10)),
                ));
            }
        }
        let (base, days) = window_split(&msgs, 80);
        assert_eq!(base, 20, "100 条里保留最后 80 条");
        assert_eq!(days.len(), 1, "被折起来的那一段只跨了一天");
        assert_eq!(days[0].label, "9 月 16 日");
        assert_eq!(days[0].from, 0);
        assert_eq!(days[0].messages, 20, "只数被折起来的那 20 条");
        assert_eq!(days[0].turns, 20);
    }

    /// 窗口切在某天中间时，那一天被折起来的轮数**不能**按整天算。
    #[test]
    fn a_day_cut_by_the_window_reports_only_its_folded_turns() {
        let mut msgs = Vec::new();
        for i in 0..100 {
            msgs.push(user_at(&format!("第 {i} 句"), Some("2026-09-17T04:00:00Z")));
        }
        let (base, days) = window_split(&msgs, 80);
        assert_eq!(base, 20);
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].turns, 20);
        assert_eq!(days[0].messages, 20);
    }

    #[test]
    fn recall_finds_what_was_said_and_says_which_day_it_was() {
        let dir = std::env::temp_dir().join(format!("lebi-recall-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let meta = hermes_core::SessionMeta::new("m", "p");
        let lines = [
            serde_json::to_string(&SessionEvent::Meta(meta)).unwrap(),
            serde_json::to_string(&SessionEvent::Message(user_at(
                "半导体先压末尾",
                Some("2026-09-16T02:00:00Z"),
            )))
            .unwrap(),
            serde_json::to_string(&SessionEvent::Message(assistant("记下了"))).unwrap(),
            serde_json::to_string(&SessionEvent::Message(user_at(
                "今天说别的",
                Some("2026-09-17T02:00:00Z"),
            )))
            .unwrap(),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        let hits = recall_in_session(&path, "半导体", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].day.as_deref(), Some("2026-09-16"));
        assert_eq!(hits[0].role, "user");
        assert!(hits[0].text.contains("压末尾"));

        assert!(
            recall_in_session(&path, "压根没说过的话", 5)
                .unwrap()
                .is_empty(),
            "没说过就是没说过，不许编"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 折叠条上的条数，就是点开真的取回的条数 —— 而且一条都不落进最近窗口。
    #[test]
    fn a_folded_day_never_overlaps_the_recent_window() {
        let mut msgs = Vec::new();
        for i in 0..275 {
            // 两条一天：0..100 → 9-10，100..200 → 9-11，200..275 → 9-12
            let day = if i < 100 {
                "2026-09-10"
            } else if i < 200 {
                "2026-09-11"
            } else {
                "2026-09-12"
            };
            msgs.push(user_at(
                &format!("第 {i} 句"),
                Some(&format!("{day}T02:00:00Z")),
            ));
        }
        let (base, groups) = window_split(&msgs, DEFAULT_WINDOW);
        assert_eq!(base, 195);

        for g in &groups {
            assert!(
                g.from + g.messages <= base,
                "折叠组 {g:?} 越过了窗口起点 {base}"
            );
            let got = window_day(&msgs, DEFAULT_WINDOW, g.day.as_deref()).unwrap();
            assert_eq!(got, *g, "点开取回的那一天必须和折叠条说的是同一组");
            assert_eq!(
                got.messages, g.messages,
                "声称 {} 条却取了 {} 条",
                g.messages, got.messages
            );
        }

        // 整段落在最近窗口里的那天不属于折叠区。
        assert!(
            window_day(&msgs, DEFAULT_WINDOW, Some("2026-09-12")).is_none(),
            "9-12 全在最近窗口里，不该能被「翻」出来"
        );
    }

    /// 短会话没有折叠区，翻旧账也不该凭空造出一天。
    #[test]
    fn a_short_session_has_nothing_to_fold() {
        let msgs = vec![
            user_at("就一句", Some("2026-09-16T02:00:00Z")),
            assistant("嗯"),
        ];
        let (base, groups) = window_split(&msgs, DEFAULT_WINDOW);
        assert_eq!((base, groups.len()), (0, 0));
        assert!(window_day(&msgs, DEFAULT_WINDOW, Some("2026-09-16")).is_none());
    }

    /// 翻旧账和读会话必须一样结实：被劈开的半条事件也得翻得出来。
    #[test]
    fn recall_also_reads_a_line_split_by_a_stray_newline() {
        let dir = std::env::temp_dir().join(format!("lebi-recall-split-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let meta =
            serde_json::to_string(&SessionEvent::Meta(hermes_core::SessionMeta::new("m", "p")))
                .unwrap();
        let broken = "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"半导体先压末尾\n这一句被劈开了\"}],\"at\":\"2026-09-16T02:00:00Z\"}}";
        std::fs::write(&path, format!("{meta}\n{broken}\n")).unwrap();

        let hits = recall_in_session(&path, "半导体", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].day.as_deref(), Some("2026-09-16"));
        assert!(hits[0].text.contains("这一句被劈开了"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
