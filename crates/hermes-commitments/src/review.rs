//! Period review (回顾): a readable account of work in a date range.
//! Not session-end reflection. Not a memory.

use std::fs;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate, Utc, Weekday};
use serde::{Deserialize, Serialize};

use crate::due::is_content_deliverable;
use crate::store::{Commitment, Status};

pub const REVIEW_SYSTEM: &str = r#"You write a weekly-report draft for a local work companion (工作搭子).

Answer only: what work landed this stretch. Not a chat log, not files they touched, not mood.

Return EXACTLY one JSON object:
{
  "focus": "one sentence on what this stretch was about, or empty if nothing landed",
  "done": ["what they DID — verb + object, ready to paste into a weekly report"],
  "outputs": ["only content files from evidence that they can hand over: doc/docx/pdf/md/…"],
  "stillOwe": ["only titles from the open-work list"]
}

Rules:
- 干了什么 is the main column. Scripts, tests, tmp, hidden files are not work.
- Do not list .py / .rs / .js or generator files as outputs.
- Downloaded raw materials are not deliverables unless the work WAS compiling them.
- No invented files or debts. Empty arrays are fine. No sycophancy.
- Ignore sandbox, tool names, Care nudges, greetings.
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPrefs {
    /// 0 = unset / off. 1 = Mon … 7 = Sun.
    #[serde(default)]
    pub weekday: u8,
    /// `this_week` | `last_7_days`
    #[serde(default = "default_span")]
    pub default_span: String,
    /// Local calendar date last time the invite was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_date: Option<String>,
}

fn default_span() -> String {
    "this_week".into()
}

impl Default for ReviewPrefs {
    fn default() -> Self {
        Self {
            weekday: 0,
            default_span: default_span(),
            dismissed_date: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewJson {
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub done: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub still_owe: Vec<String>,
}

pub fn prefs_path() -> PathBuf {
    hermes_core::data_path("reviews").join("prefs.json")
}

pub fn load_prefs() -> ReviewPrefs {
    let p = prefs_path();
    let Ok(raw) = fs::read_to_string(&p) else {
        return ReviewPrefs::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_prefs(prefs: &ReviewPrefs) -> std::io::Result<()> {
    let dir = hermes_core::data_path("reviews");
    fs::create_dir_all(&dir)?;
    let tmp = dir.join("prefs.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(prefs).unwrap_or_else(|_| b"{}".to_vec()))?;
    fs::rename(tmp, prefs_path())?;
    Ok(())
}

/// Local Monday of the week containing `day`.
pub fn week_start(day: NaiveDate) -> NaiveDate {
    let wd = day.weekday().num_days_from_monday() as i64;
    day - chrono::Duration::days(wd)
}

pub fn span_range(span: &str, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match span {
        "last_7_days" => (today - chrono::Duration::days(6), today),
        _ => (week_start(today), today),
    }
}

pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

/// Monday=1 … Sunday=7.
pub fn weekday_today() -> u8 {
    match Local::now().weekday() {
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
        Weekday::Sun => 7,
    }
}

pub fn invite_due(prefs: &ReviewPrefs, reviewed_this_span: bool) -> bool {
    if prefs.weekday == 0 || prefs.weekday > 7 {
        return false;
    }
    if weekday_today() != prefs.weekday {
        return false;
    }
    if reviewed_this_span {
        return false;
    }
    let today = today_local().to_string();
    prefs.dismissed_date.as_deref() != Some(today.as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewIndexEntry {
    pub path: String,
    pub from: String,
    pub to: String,
    pub created_at: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub done: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub still_owe: Vec<String>,
}

pub fn index_path() -> PathBuf {
    hermes_core::data_path("reviews").join("index.json")
}

pub fn load_index() -> Vec<ReviewIndexEntry> {
    let Ok(raw) = fs::read_to_string(index_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn reviewed_span(from: NaiveDate, to: NaiveDate) -> bool {
    let a = from.to_string();
    let b = to.to_string();
    load_index()
        .iter()
        .any(|e| e.from == a && e.to == b)
}

/// One 交差账 per interval. Re-steaming the same span replaces the old row.
pub fn write_review_file(
    from: NaiveDate,
    to: NaiveDate,
    body: &ReviewJson,
) -> std::io::Result<PathBuf> {
    let dir = hermes_core::data_path("reviews");
    fs::create_dir_all(&dir)?;
    let md = format_markdown(from, to, body);
    let stamp = Utc::now().format("%Y%m%d");
    let name = format!("{stamp}_{from}_{to}.md");
    let path = dir.join(&name);
    fs::write(&path, &md)?;
    let a = from.to_string();
    let b = to.to_string();
    let path_str = path.to_string_lossy().into_owned();
    let mut idx = load_index();
    for e in idx.iter().filter(|e| e.from == a && e.to == b && e.path != path_str) {
        let _ = fs::remove_file(&e.path);
    }
    idx.retain(|e| !(e.from == a && e.to == b));
    idx.insert(
        0,
        ReviewIndexEntry {
            path: path_str,
            from: a,
            to: b,
            created_at: Utc::now().to_rfc3339(),
            focus: body.focus.clone(),
            done: body.done.clone(),
            outputs: body.outputs.clone(),
            still_owe: body.still_owe.clone(),
        },
    );
    idx.truncate(40);
    let tmp = dir.join("index.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&idx).unwrap_or_default())?;
    fs::rename(tmp, index_path())?;
    Ok(path)
}

pub fn listed_reviews() -> Vec<ReviewIndexEntry> {
    load_index().into_iter().map(hydrate_entry).collect()
}

fn hydrate_entry(mut e: ReviewIndexEntry) -> ReviewIndexEntry {
    if !e.focus.is_empty() || !e.done.is_empty() || !e.outputs.is_empty() || !e.still_owe.is_empty()
    {
        return e;
    }
    let Ok(raw) = fs::read_to_string(&e.path) else {
        return e;
    };
    let body = parse_review_md(&raw);
    e.focus = body.focus;
    e.done = body.done;
    e.outputs = body.outputs;
    e.still_owe = body.still_owe;
    e
}

/// Parse the markdown we ourselves write (old index rows have no structured fields).
pub fn parse_review_md(raw: &str) -> ReviewJson {
    let mut focus = String::new();
    let mut section = "";
    let mut done = Vec::new();
    let mut outputs = Vec::new();
    let mut still_owe = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("# ") {
            continue;
        }
        if t == "## 做成了什么" {
            section = "done";
            continue;
        }
        if t == "## 产出" {
            section = "outputs";
            continue;
        }
        if t == "## 还欠" {
            section = "owe";
            continue;
        }
        if t.starts_with("## ") {
            section = "other";
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("- ") {
            match section {
                "done" => done.push(rest.to_string()),
                "outputs" => outputs.push(rest.trim_matches('`').to_string()),
                "owe" => still_owe.push(rest.to_string()),
                _ => {}
            }
            continue;
        }
        if section.is_empty() {
            if !focus.is_empty() {
                focus.push(' ');
            }
            focus.push_str(t);
        }
    }
    ReviewJson {
        focus,
        done,
        outputs,
        still_owe,
    }
}

pub fn format_markdown(from: NaiveDate, to: NaiveDate, body: &ReviewJson) -> String {
    let mut s = format!("# 回顾 {from} – {to}\n\n");
    if !body.focus.trim().is_empty() {
        s.push_str(body.focus.trim());
        s.push_str("\n\n");
    }
    if !body.done.is_empty() {
        s.push_str("## 做成了什么\n\n");
        for d in &body.done {
            s.push_str(&format!("- {d}\n"));
        }
        s.push('\n');
    }
    if !body.outputs.is_empty() {
        s.push_str("## 产出\n\n");
        for p in &body.outputs {
            s.push_str(&format!("- `{p}`\n"));
        }
        s.push('\n');
    }
    if !body.still_owe.is_empty() {
        s.push_str("## 还欠\n\n");
        for o in &body.still_owe {
            s.push_str(&format!("- {o}\n"));
        }
    }
    s
}

/// Evidence pack for the model (capped).
pub fn evidence_user_prompt(
    from: NaiveDate,
    to: NaiveDate,
    sessions: &[(String, String)],
    outputs: &[String],
    owed: &[Commitment],
    done: &[Commitment],
) -> String {
    let mut buf = format!("Period: {from} to {to} (local dates).\n\n");
    buf.push_str("## Session titles / snippets\n");
    if sessions.is_empty() {
        buf.push_str("(none)\n");
    }
    for (title, snip) in sessions.iter().take(24) {
        buf.push_str(&format!("- {title}: {snip}\n"));
    }
    buf.push_str("\n## Paths written in this period\n");
    if outputs.is_empty() {
        buf.push_str("(none recorded)\n");
    }
    for p in outputs.iter().take(30) {
        buf.push_str(&format!("- {p}\n"));
    }
    buf.push_str("\n## Open work still owed\n");
    if owed.is_empty() {
        buf.push_str("(none)\n");
    }
    for c in owed {
        buf.push_str(&format!("- {}\n", c.title));
    }
    buf.push_str("\n## Marked done recently\n");
    if done.is_empty() {
        buf.push_str("(none)\n");
    }
    for c in done.iter().filter(|c| c.status == Status::Done) {
        buf.push_str(&format!("- {}\n", c.title));
    }
    buf
}

pub fn parse_review_json(raw: &str) -> ReviewJson {
    let t = raw.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let t = extract_json_object(t).unwrap_or(t);
    serde_json::from_str(t).unwrap_or_default()
}

fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&s[start..=end])
}

pub fn body_blank(body: &ReviewJson) -> bool {
    body.focus.trim().is_empty()
        && body.done.is_empty()
        && body.outputs.is_empty()
        && body.still_owe.is_empty()
}

/// When the model does not return a usable JSON body, still write a 交差账
/// from evidence so the page is never a date with nothing under it.
pub fn fallback_review(
    sessions: &[(String, String)],
    outputs: &[String],
    owed: &[Commitment],
    done: &[Commitment],
) -> ReviewJson {
    let titles: Vec<&str> = sessions
        .iter()
        .map(|(t, _)| t.trim())
        .filter(|t| !t.is_empty())
        .take(4)
        .collect();
    let focus = if titles.is_empty() {
        "这段日子对话很少。下面是还能对上的交差。".into()
    } else {
        format!("这段主要在：{}。", titles.join("、"))
    };
    ReviewJson {
        focus,
        done: done
            .iter()
            .filter(|c| c.status == Status::Done)
            .map(|c| c.title.clone())
            .collect(),
        outputs: outputs
            .iter()
            .filter(|p| is_content_deliverable(p))
            .cloned()
            .collect(),
        still_owe: owed.iter().map(|c| c.title.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_weekday_is_off() {
        assert_eq!(ReviewPrefs::default().weekday, 0);
        assert!(!invite_due(&ReviewPrefs::default(), false));
    }

    #[test]
    fn week_start_is_monday() {
        let wed = NaiveDate::from_ymd_opt(2026, 8, 12).unwrap();
        assert_eq!(week_start(wed), NaiveDate::from_ymd_opt(2026, 8, 10).unwrap());
    }

    #[test]
    fn parse_json_in_prose() {
        let raw = "好的，这是回顾：\n{\"focus\":\"收改稿\",\"done\":[\"周五交稿\"],\"outputs\":[],\"stillOwe\":[]}\n完。";
        let body = parse_review_json(raw);
        assert_eq!(body.focus, "收改稿");
        assert_eq!(body.done, vec!["周五交稿"]);
    }

    #[test]
    fn parse_own_markdown() {
        let md = "# 回顾 2026-08-10 – 2026-08-14\n\n本周在收改稿。\n\n## 做成了什么\n\n- 周五交改稿\n\n## 产出\n\n- `/tmp/draft.md`\n\n## 还欠\n\n- 约设计\n";
        let body = parse_review_md(md);
        assert_eq!(body.focus, "本周在收改稿。");
        assert_eq!(body.done, vec!["周五交改稿"]);
        assert_eq!(body.outputs, vec!["/tmp/draft.md"]);
        assert_eq!(body.still_owe, vec!["约设计"]);
    }
}
