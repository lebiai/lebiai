//! 「决定」——选题单、审稿意见（`docs/spec/projects.md` §6）。
//!
//! **引擎写结构，人物写正文。** 决定是「谁定的、什么时候、定了什么、因为什么」四样，
//! 缺一不可；这四样由**引擎排版**，写在文件开头的一段 YAML 头里，正文那一大块留给
//! 人物（那才是给人看的）。所以：
//!
//! - 工具 `decision` 是**唯一**写头的地方；模型不许自己写这段结构。
//! - 界面读头渲染卡片（清单 + 为什么 + 谁定的），读不到头就把它当**普通产物**——
//!   用户手改过、或只写了半截，都不该让界面崩，更不该猜出一个「已点头」。
//!
//! 与 [`crate::frontmatter::split`] 的分工：那个是给**编译进二进制的定义**用的，
//! 格式不对就是我们的 bug，直接 assert；这里是给**用户能改的文件**用的，一律柔性降级。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 决定有哪几种。v1 就这两种——够这一期用，别提前造第四种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// 选题单：今晚出哪几条、什么顺序、什么角度。
    TopicList,
    /// 审稿意见：放行还是打回，打回必须给理由。
    Review,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::TopicList => "选题单",
            Kind::Review => "审稿意见",
        }
    }
}

/// 决定的状态。`Pending` 只对选题单有意义（**全流程唯一必停的一步**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// 等用户点头。
    Pending,
    /// 用户点了头。
    Approved,
    /// 用户要改（理由在 `answered_note`）。
    Revised,
    /// 不需要用户点头的（审稿意见：放行或打回由编辑给）。
    Settled,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Pending => "待点头",
            Status::Approved => "已点头",
            Status::Revised => "要改",
            Status::Settled => "已给出",
        }
    }
}

/// 选题单里的一条：进什么、为什么。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 一份决定。**头部四样**（谁定的 / 什么时候 / 定了什么 / 因为什么）在这里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub kind: Kind,
    /// 谁定的（人物 id）。由引擎填——模型说了不算。
    pub by: String,
    pub at: DateTime<Utc>,
    /// 因为什么。四样里最容易被省掉的一样，所以必填。
    pub why: String,
    pub status: Status,
    /// 哪一期（`YYYY-MM-DD`）。见 `docs/spec/projects.md` §4.3：期 = 日期目录。
    pub episode: String,
    /// 属于哪个项目组（工位会话里的决定没有组）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
    /// 审稿意见：`放行` / `打回`（打回的理由写在 `why` 或正文里）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// 用户什么时候答的。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<DateTime<Utc>>,
    /// 用户答了什么（点头时可以不写；要改时必须写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_note: Option<String>,
}

/// 头与正文的分界：文件**开头**那段 `---` 夹起来的 YAML。
fn split_header(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix("---")?;
    // 第一行必须**整行**是 `---`（免得把 `----` 之类的分隔线当开头）。
    let first_break = rest.find('\n')?;
    if !rest[..first_break].trim().is_empty() {
        return None;
    }
    let rest = &rest[first_break + 1..];
    // 找闭合的整行 `---`。
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return Some((yaml, body));
        }
        offset += line.len();
    }
    None
}

/// 读一份文件的决定头。**没有头 / 头不完整 / 不是我们的决定 → `None`**，不报错。
///
/// 这是界面渲染卡片的唯一入口：读不到就当普通产物（用户手改过也不该让界面崩）。
pub fn parse(raw: &str) -> Option<Decision> {
    let (yaml, _) = split_header(raw)?;
    serde_yaml::from_str::<Decision>(yaml).ok()
}

/// 把决定渲染成完整文件（头 + 正文）。头由引擎写，正文原样带过来。
pub fn render(d: &Decision, body: &str) -> String {
    let yaml = serde_yaml::to_string(d).unwrap_or_default();
    let body = body.trim_start_matches('\n');
    format!("---\n{}---\n\n{}", yaml.trim_end().to_string() + "\n", body)
}

/// 只换头，正文一字不动。给「点头 / 要改」用。
pub fn rewrite(raw: &str, d: &Decision) -> String {
    let body = split_header(raw).map(|(_, b)| b).unwrap_or(raw);
    render(d, body)
}

/// 本期（`YYYY-MM-DD`）。决定和产物都按天归，**期 = 日期目录**（规格 §4.3）。
pub fn episode_of(path: &str, today: NaiveDate) -> String {
    path.split('/')
        .find_map(|seg| NaiveDate::parse_from_str(seg, "%Y-%m-%d").ok())
        .unwrap_or(today)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Decision {
        Decision {
            kind: Kind::TopicList,
            by: "lv-lao-shi".into(),
            at: "2026-09-17T06:03:00Z".parse().unwrap(),
            why: "今日产业类只剩两条硬货".into(),
            status: Status::Pending,
            episode: "2026-09-17".into(),
            team: Some("caifu-zaozhidao".into()),
            items: vec![
                Item {
                    title: "央行开展 5000 亿 MLF".into(),
                    note: None,
                },
                Item {
                    title: "半导体设备出口管制更新".into(),
                    note: Some("扫地僧称重：真分量".into()),
                },
            ],
            verdict: None,
            answered_at: None,
            answered_note: None,
        }
    }

    #[test]
    fn a_rendered_decision_round_trips_and_keeps_the_body() {
        let text = render(&sample(), "# 选题单 · 9 月 17 日\n\n1. 央行…\n");
        let back = parse(&text).expect("自己写的头自己得读得回来");
        assert_eq!(back.kind, Kind::TopicList);
        assert_eq!(back.by, "lv-lao-shi");
        assert_eq!(back.status, Status::Pending);
        assert_eq!(back.items.len(), 2);
        assert_eq!(back.items[1].note.as_deref(), Some("扫地僧称重：真分量"));
        assert!(
            text.contains("# 选题单 · 9 月 17 日"),
            "正文要原样在：{text}"
        );
        assert!(text.contains("why: 今日产业类只剩两条硬货"));
    }

    /// 旧文件头里还带着 `seconds`（选题单时代留下的）——这一样已经撤了，
    /// 但**旧文件必须还读得回来**：读不出头，界面就把它当普通产物。
    #[test]
    fn a_legacy_header_with_seconds_still_reads() {
        let raw = "---\nkind: topic-list\nby: lv-lao-shi\n\
                   at: \"2026-09-17T06:03:00Z\"\nwhy: 旧头也得读得回来\nstatus: pending\n\
                   episode: \"2026-09-17\"\nitems:\n- title: 央行开展 5000 亿 MLF\n  seconds: 40\n---\n\n# 选题单\n";
        let d = parse(raw).expect("旧头必须还读得回来");
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.items[0].title, "央行开展 5000 亿 MLF");
    }

    /// 用户手改过 / 只有半截 / 根本不是我们的文件 → 一律 `None`，不许猜。
    #[test]
    fn a_broken_header_is_not_a_decision() {
        assert!(parse("").is_none());
        assert!(parse("# 随便一个文件\n").is_none());
        assert!(parse("---\nkind: topic-list\n").is_none(), "头没闭合");
        assert!(
            parse("---\nkind: 随便\nwhy: x\n---\n\n正文\n").is_none(),
            "kind 不认识就不是我们的决定"
        );
        assert!(
            parse("---\nkind: topic-list\nby: lv-lao-shi\n---\n\n正文\n").is_none(),
            "缺 at / why / status / episode 的不算——四样缺一不可"
        );
    }

    #[test]
    fn answering_rewrites_the_head_and_leaves_the_body_alone() {
        let raw = render(&sample(), "# 选题单\n\n正文一字不动\n");
        let mut d = parse(&raw).unwrap();
        d.status = Status::Approved;
        d.answered_at = Some("2026-09-17T07:05:00Z".parse().unwrap());
        let out = rewrite(&raw, &d);

        assert!(out.contains("正文一字不动"), "正文不许被动：{out}");
        let back = parse(&out).unwrap();
        assert_eq!(back.status, Status::Approved);
        assert_eq!(
            back.answered_at.unwrap().to_rfc3339(),
            "2026-09-17T07:05:00+00:00"
        );
        assert_eq!(out.matches("---").count(), 2, "只该有一对分隔线：{out}");
    }

    #[test]
    fn the_episode_comes_from_the_day_folder() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 17).unwrap();
        assert_eq!(
            episode_of("outputs/2026-09-15/选题单-2026-09-15.md", today),
            "2026-09-15"
        );
        assert_eq!(episode_of("outputs/选题单.md", today), "2026-09-17");
    }
}
