//! `material_read` —— 按 ID 取那一期的料（一条或几条，一次调用）。
//!
//! 库就是那两个文件：`outputs/<日期>/index-<日期>.md`（一行一条的索引）+
//! `outputs/<日期>/资讯-<日期>.md`（每条三行的成品）。这个工具只做一件事——
//! 把「给我 ID，还我那条」变成**一次调用**。
//!
//! ID 形如 `20260921-market-007`（`<YYYYMMDD>-<lane>-<NNN>`）。lane 是**纯 ASCII 的
//! 三个词**（`macro` / `industry` / `market`）：ID 是机器和接力棒用的东西，掺中文会让
//! BSD awk 把汉字切半、让路径校验多一类坑（实测踩过）。「哪一栏」看板块抬头就够了。
//!
//! 为什么要有它：下游原来得先 `grep` 索引拿行号、再 `read(offset = 行号 - 1)`
//! 取回三行。两步里任何一步算错（行号看串、越界、成品换行），就会变成「取不到」，
//! 而每次取料要跑两个模型往返。ID 里本来就带着日期，行号也已经在索引里躺着——
//! 这两件事该由引擎算，不该交给模型算。
//!
//! 它**不新增存储、不整份重写、不设配额**：索引不在就明说不在。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

/// 成品里一条最多取这么多行（正常三条：层级来源时间 / 概要 / 链接）。
/// 概要换行时兜底，也防止索引里的行号指错地方时一路吞到文件尾。
const MAX_ITEM_LINES: usize = 12;

pub fn handles(name: &str) -> bool {
    name == "material_read"
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "material_read".into(),
        description: "Read clippings **by ID** from the material collected for a day — one ID or \
            several in a single call. Returns that clipping's own lines (tier/source/time, \
            summary, link) with no line numbers and no index arithmetic.\n\
            \n\
            Use it where you would otherwise grep the index for a line number and then read at an \
            offset: after the index told you which entries you want, or after a topic list named \
            the IDs you must write up.\n\
            \n\
            IDs look like `20260921-market-007` — the date is inside the ID, so nothing else is \
            needed. A short ID without the date (`market-037`) is accepted too: it is resolved \
            against the day the material belongs to, which is today unless `date` says otherwise. \
            If the index for that day is not on disk, it says so instead of guessing."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "ids": {
                    "description": "素材 ID，一个或多个（如 20260921-market-007；短 ID market-037 也行）。",
                    "oneOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}}
                    ]
                },
                "date": {
                    "type": "string",
                    "description": "哪一期（YYYY-MM-DD）。只在拿到短 ID、且这一期不是今天时才要写。"
                }
            },
            "required": ["ids"]
        }),
        requires_confirmation: false,
    }
}

#[derive(Deserialize)]
struct Args {
    #[serde(default)]
    ids: Option<OneOrMany>,
    #[serde(default)]
    id: Option<String>,
    /// 哪一期。短 ID 认不出日期，就落在这天（默认今天）。
    #[serde(default)]
    date: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

/// 一天的那两份文件，读一次就够。
struct DayFiles {
    /// ID → 成品里的起始行号（1 起）。
    index: HashMap<String, usize>,
    body: Vec<String>,
}

impl DayFiles {
    fn item(&self, line_no: usize) -> Option<String> {
        if line_no == 0 || line_no > self.body.len() {
            return None;
        }
        let mut out: Vec<&str> = Vec::new();
        for line in self.body.iter().skip(line_no - 1) {
            if line.trim().is_empty() {
                break;
            }
            out.push(line);
            if out.len() >= MAX_ITEM_LINES {
                break;
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out.join("\n"))
        }
    }
}

/// 三个栏名，**唯一**的合法取值。ID 是契约的一部分：写错栏名要当场说不认识，
/// 不要让它变成「索引里没有这个 ID」那种含糊的下游错误。
pub const LANES: [&str; 3] = ["macro", "industry", "market"];

/// ID →（哪一期，写全的 ID）。
///
/// 全 ID（`20260921-market-007`）日期在 ID 里；短 ID（`market-037`）认不出日期，落在
/// `default_date`（调用方给的那期，默认今天）——但它**只在那一期里找**，不去别的日子乱翻。
/// 形状不对就是形状不对，不猜。
fn resolve(id: &str, default_date: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = id.split('-').collect();
    match parts.as_slice() {
        [day, lane, num] if is_day(day) && is_num(num) && LANES.contains(lane) => Some((
            format!("{}-{}-{}", &day[..4], &day[4..6], &day[6..8]),
            id.to_string(),
        )),
        [lane, num] if is_num(num) && LANES.contains(lane) => {
            let compact: String = default_date.chars().filter(char::is_ascii_digit).collect();
            Some((default_date.to_string(), format!("{compact}-{id}")))
        }
        _ => None,
    }
}

fn is_day(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_num(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn load_day(workspace: &Path, date: &str) -> std::result::Result<DayFiles, String> {
    let rel = format!("outputs/{date}");
    let idx_path = workspace
        .join("outputs")
        .join(date)
        .join(format!("index-{date}.md"));
    if !idx_path.is_file() {
        return Err(format!(
            "材料没落盘：{rel}/index-{date}.md 不在——这一期还没交上来，先让王海燕交。"
        ));
    }
    let raw = std::fs::read_to_string(&idx_path)
        .map_err(|e| format!("索引读不出来（{rel}/index-{date}.md）：{e}"))?;
    let mut index = HashMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('｜');
        let Some(id) = fields.next().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(lineno) = line
            .rsplit('｜')
            .next()
            .map(str::trim)
            .and_then(|s| s.strip_prefix('L'))
            .and_then(|s| s.parse::<usize>().ok())
        else {
            continue;
        };
        index.entry(id.to_string()).or_insert(lineno);
    }
    let body_path = workspace
        .join("outputs")
        .join(date)
        .join(format!("资讯-{date}.md"));
    let body = match std::fs::read_to_string(&body_path) {
        Ok(text) => text.lines().map(str::to_string).collect(),
        Err(_) => Vec::new(),
    };
    Ok(DayFiles { index, body })
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("material_read: 参数不对：{e}")))?;

    // 顺序 = 模型给的顺序，重复的去掉（同一 ID 要两遍没有意义）。
    let mut wanted: Vec<String> = Vec::new();
    let mut push = |s: String| {
        let s = s.trim().to_string();
        if !s.is_empty() && !wanted.contains(&s) {
            wanted.push(s);
        }
    };
    match a.ids {
        Some(OneOrMany::One(s)) => push(s),
        Some(OneOrMany::Many(v)) => {
            for s in v {
                push(s);
            }
        }
        None => {}
    }
    if let Some(s) = a.id {
        push(s);
    }
    if wanted.is_empty() {
        return Ok(ToolCallOutcome {
            content: "material_read: 给一个 ID（如 20260921-market-007），或几个 ID 一起给。"
                .into(),
            is_error: true,
        });
    }

    // 短 ID 落在哪一期：调用方给了 `date` 就用它，没给就是今天。
    let default_date = match a.date.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) if is_date(d) => d.to_string(),
        Some(d) => {
            return Ok(ToolCallOutcome {
                content: format!("material_read: `date` 要是 YYYY-MM-DD（给的是「{d}」）。"),
                is_error: true,
            })
        }
        None => chrono::Local::now().date_naive().to_string(),
    };

    let mut days: BTreeMap<String, std::result::Result<DayFiles, String>> = BTreeMap::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    for id in &wanted {
        let Some((date, full_id)) = resolve(id, &default_date) else {
            notes.push(format!(
                "「{id}」不是素材 ID。ID 长这样：20260921-market-007（日期-栏-序号），\
                 栏只认 {LANES:?} 这三个 ASCII 词；短 ID market-037 也行，但要能认出是哪一期（默认今天）。"
            ));
            continue;
        };
        let day = days
            .entry(date.clone())
            .or_insert_with(|| load_day(workspace, &date));
        let day = match day {
            Ok(d) => d,
            Err(msg) => {
                notes.push(format!("[{full_id}] {msg}"));
                continue;
            }
        };
        let Some(&line_no) = day.index.get(&full_id) else {
            notes.push(format!("[{full_id}] 这一期的索引里没有这个 ID。"));
            continue;
        };
        match day.item(line_no) {
            // 抬头一律写**全 ID**：短 ID 是谁解析出来的、落到哪一期，一眼看得见。
            Some(text) => blocks.push(format!("[{full_id}]\n{text}")),
            None => notes.push(format!(
                "[{full_id}] 索引说在 L{line_no}，但成品里那个位置取不到（成品 {} 行）——\
                 本条按取不到处理，别猜。",
                day.body.len()
            )),
        }
    }

    if blocks.is_empty() && notes.is_empty() {
        return Ok(ToolCallOutcome {
            content: "material_read: 没取到任何一条。".into(),
            is_error: true,
        });
    }

    let mut content = blocks.join("\n\n");
    if !notes.is_empty() {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&notes.join("\n"));
    }
    let is_error = blocks.is_empty();
    Ok(ToolCallOutcome { content, is_error })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试自己一个目录：并行跑时共用 `temp_dir()/固定名` 会互相删。
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let day = dir.path().join("outputs").join("2026-09-21");
        std::fs::create_dir_all(&day).unwrap();
        // 成品：卷首一行窗口 + 空行，再 `## 宏观` 抬头 + 空行，第 5 行起是第一条
        // （与王海燕的行号口径一致）。
        std::fs::write(
            day.join("资讯-2026-09-21.md"),
            "# 财富早知道 · 资讯卷｜窗口 2026-09-21 00:00 — 2026-09-21 19:30（北京时间）\n\n## 宏观\n\n【官方】新华财经｜2026-09-21 19:30｜\n潘功胜会见香港财政司司长。｜\nhttps://example.com/1\n\n【其他】36氪｜2026-09-21 19:20｜\n央行召开外资金融机构座谈会。｜\nhttps://example.com/2\n",
        )
        .unwrap();
        std::fs::write(
            day.join("index-2026-09-21.md"),
            "20260921-macro-001｜新华财经｜2026-09-21 19:30｜潘功胜会见香港财政司司长…｜L5\n20260921-macro-002｜36氪｜2026-09-21 19:20｜央行召开外资金融机构座谈会…｜L9\n",
        )
        .unwrap();
        dir
    }

    async fn call(ws: &Path, args: serde_json::Value) -> ToolCallOutcome {
        run(ws, args).await.unwrap()
    }

    #[tokio::test]
    async fn one_id_returns_that_clipping_without_line_numbers() {
        let ws = fixture();
        let out = call(ws.path(), serde_json::json!({"ids": "20260921-macro-002"})).await;
        assert!(!out.is_error, "{}", out.content);
        assert!(
            out.content.contains("[20260921-macro-002]"),
            "{}",
            out.content
        );
        assert!(
            out.content.contains("【其他】36氪｜2026-09-21 19:20｜"),
            "{}",
            out.content
        );
        assert!(
            out.content.contains("https://example.com/2"),
            "{}",
            out.content
        );
        // 只这一条：别的条目一个字都不许带出来。
        assert!(!out.content.contains("潘功胜"), "串条了：{}", out.content);
        // read 会带行号；这个工具不带——行号进稿子就是脏字。
        assert!(
            !out.content.contains("\t"),
            "不该有行号前缀：{}",
            out.content
        );
    }

    #[tokio::test]
    async fn several_ids_come_back_in_one_call_and_keep_order() {
        let ws = fixture();
        let out = call(
            ws.path(),
            serde_json::json!({"ids": ["20260921-macro-002", "20260921-macro-001"]}),
        )
        .await;
        assert!(!out.is_error, "{}", out.content);
        let a = out.content.find("macro-002").unwrap();
        let b = out.content.find("macro-001").unwrap();
        assert!(a < b, "顺序要跟模型给的一致：{}", out.content);
        assert!(out.content.contains("潘功胜") && out.content.contains("36氪"));
    }

    #[tokio::test]
    async fn unknown_id_says_so_and_still_returns_the_ones_it_found() {
        let ws = fixture();
        let out = call(
            ws.path(),
            serde_json::json!({"ids": ["20260921-macro-999", "20260921-macro-001"]}),
        )
        .await;
        assert!(!out.is_error, "取到了一条就不算失败：{}", out.content);
        assert!(out.content.contains("【官方】新华财经"), "{}", out.content);
        assert!(
            out.content.contains("索引里没有这个 ID"),
            "取不到的要明说：{}",
            out.content
        );
    }

    #[tokio::test]
    async fn missing_index_is_reported_not_invented() {
        let ws = fixture();
        let out = call(ws.path(), serde_json::json!({"ids": "20260920-macro-001"})).await;
        assert!(out.is_error);
        assert!(out.content.contains("材料没落盘"), "{}", out.content);
    }

    #[tokio::test]
    async fn a_malformed_id_is_named_as_malformed() {
        let ws = fixture();
        let out = call(ws.path(), serde_json::json!({"ids": "小王干活"})).await;
        assert!(out.is_error);
        assert!(out.content.contains("不是素材 ID"), "{}", out.content);
    }

    #[tokio::test]
    async fn no_ids_at_all_is_a_usage_error() {
        let ws = fixture();
        let out = call(ws.path(), serde_json::json!({})).await;
        assert!(out.is_error);
        assert!(out.content.contains("给一个 ID"), "{}", out.content);
    }

    #[test]
    fn a_full_id_carries_its_own_date_and_a_short_one_falls_on_the_default() {
        let today = "2026-09-21";
        assert_eq!(
            resolve("20260921-macro-001", today),
            Some(("2026-09-21".into(), "20260921-macro-001".into()))
        );
        assert_eq!(
            resolve("market-037", today),
            Some(("2026-09-21".into(), "20260921-market-037".into()))
        );
        // 形状不对就不猜——尤其不能让它拼进路径。
        assert!(resolve("2026092-macro-001", today).is_none());
        assert!(resolve("20260921-macro-01", today).is_none());
        assert!(resolve("../../etc/passwd", today).is_none());
        assert!(resolve("20260921-../macro-001", today).is_none());
        assert!(resolve("小王干活", today).is_none());
        // lane 是纯 ASCII 的三个词：旧的中文栏名一律不认（ID 是机器用的东西）。
        assert!(resolve("20260921-宏观-001", today).is_none());
        assert!(resolve("资本-037", today).is_none());
        assert!(
            resolve("20260921-Market-007", today).is_none(),
            "大小写敏感，只认小写"
        );
    }

    #[tokio::test]
    async fn a_short_id_finds_that_days_clipping_under_its_full_name() {
        let ws = fixture();
        let out = call(
            ws.path(),
            serde_json::json!({"ids": "macro-002", "date": "2026-09-21"}),
        )
        .await;
        assert!(!out.is_error, "{}", out.content);
        // 抬头写全 ID —— 短 ID 落到哪一期一眼看得见。
        assert!(
            out.content.contains("[20260921-macro-002]"),
            "{}",
            out.content
        );
        assert!(
            out.content.contains("https://example.com/2"),
            "{}",
            out.content
        );
    }

    #[test]
    fn a_bad_date_argument_is_refused_not_guessed() {
        assert!(is_date("2026-09-21"));
        assert!(!is_date("2026/09/21"));
        assert!(!is_date("20260921"));
        assert!(!is_date("2026-09-21/../../"));
    }
}
