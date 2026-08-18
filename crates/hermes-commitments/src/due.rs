//! Parse a human due phrase into a local calendar day.
//! Vague words are refused — no deadline, no debt.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DueError {
    /// 尽快 / 以后 — not a day.
    Vague,
    /// Could not read a day.
    Unparsed,
}

pub fn parse_due(phrase: &str, today: NaiveDate) -> Result<(String, NaiveDate), DueError> {
    let raw = phrase.trim();
    if raw.is_empty() {
        return Err(DueError::Unparsed);
    }
    if is_vague(raw) {
        return Err(DueError::Vague);
    }
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok((raw.to_string(), d));
    }
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y/%m/%d") {
        return Ok((raw.to_string(), d));
    }
    let compact = raw.replace(['前', '之'], "");
    let compact = compact.trim();

    if matches_any(compact, &["今天", "今日", "今晚", "today"]) {
        return Ok((raw.to_string(), today));
    }
    if matches_any(compact, &["明天", "明日", "tomorrow"]) {
        return Ok((raw.to_string(), today + Duration::days(1)));
    }
    if matches_any(compact, &["这周", "本周", "这星期", "本星期"]) {
        return Ok((raw.to_string(), this_week_friday(today)));
    }
    if matches_any(compact, &["下周", "下星期", "下礼拜"]) {
        return Ok((raw.to_string(), this_week_friday(today) + Duration::days(7)));
    }
    if let Some(wd) = parse_weekday(compact) {
        return Ok((raw.to_string(), this_or_next_weekday(today, wd)));
    }
    if let Some(d) = parse_md(compact, today) {
        return Ok((raw.to_string(), d));
    }
    Err(DueError::Unparsed)
}

fn is_vague(s: &str) -> bool {
    const WORDS: &[&str] = &[
        "以后",
        "有空",
        "尽快",
        "回头",
        "再说",
        "随时",
        "抽空",
        "得空",
        "有时间",
        "空了",
        "later",
        "soon",
        "someday",
        "whenever",
        "asap",
    ];
    let t = s.trim().to_lowercase();
    WORDS.iter().any(|w| t == *w || t == format!("{w}吧"))
}

fn matches_any(s: &str, words: &[&str]) -> bool {
    words.iter().any(|w| s.eq_ignore_ascii_case(w))
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    let s = s.replace("星期", "").replace("礼拜", "").replace("周", "");
    match s.trim() {
        "一" | "1" | "mon" | "monday" => Some(Weekday::Mon),
        "二" | "2" | "tue" | "tuesday" => Some(Weekday::Tue),
        "三" | "3" | "wed" | "wednesday" => Some(Weekday::Wed),
        "四" | "4" | "thu" | "thursday" => Some(Weekday::Thu),
        "五" | "5" | "fri" | "friday" => Some(Weekday::Fri),
        "六" | "6" | "sat" | "saturday" => Some(Weekday::Sat),
        "日" | "天" | "7" | "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn this_week_friday(today: NaiveDate) -> NaiveDate {
    let mon = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    mon + Duration::days(4)
}

fn this_or_next_weekday(today: NaiveDate, target: Weekday) -> NaiveDate {
    let delta = (target.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
    today + Duration::days(delta as i64)
}

fn parse_md(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.replace('月', "-").replace(['日', '号'], "");
    let parts: Vec<&str> = s.split(['-', '/', '.']).filter(|p| !p.is_empty()).collect();
    let (m, d) = match parts.as_slice() {
        [m, d] => ((*m).parse::<u32>().ok()?, (*d).parse::<u32>().ok()?),
        _ => return None,
    };
    let year = today.year();
    let mut date = NaiveDate::from_ymd_opt(year, m, d)?;
    if date < today - Duration::days(14) {
        date = NaiveDate::from_ymd_opt(year + 1, m, d)?;
    }
    Some(date)
}

/// Paths that can be handed over as work content — not scripts or scratch.
pub fn is_content_deliverable(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or(p.as_str());
    if name.starts_with('.') {
        return false;
    }
    let lower = name.to_lowercase();
    if lower.contains("write_test") || lower.contains(".tmp") || lower.starts_with("tmp_") {
        return false;
    }
    if p.contains("/tmp/") || p.contains("/var/folders/") {
        return false;
    }
    let ext = lower.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "doc"
            | "docx"
            | "pdf"
            | "md"
            | "txt"
            | "rtf"
            | "xlsx"
            | "xls"
            | "csv"
            | "pptx"
            | "ppt"
            | "pages"
            | "numbers"
            | "key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn vague_refused() {
        let t = day(2026, 8, 17);
        assert_eq!(parse_due("尽快", t), Err(DueError::Vague));
        assert_eq!(parse_due("以后", t), Err(DueError::Vague));
        assert_eq!(parse_due("asap", t), Err(DueError::Vague));
    }

    #[test]
    fn today_and_week() {
        // 2026-08-17 is Monday.
        let mon = day(2026, 8, 17);
        assert_eq!(parse_due("今天", mon).unwrap().1, mon);
        assert_eq!(parse_due("这周", mon).unwrap().1, day(2026, 8, 21));
        assert_eq!(parse_due("下周", mon).unwrap().1, day(2026, 8, 28));
        assert_eq!(parse_due("周五", mon).unwrap().1, day(2026, 8, 21));
        assert_eq!(parse_due("周五前", mon).unwrap().1, day(2026, 8, 21));
    }

    #[test]
    fn friday_on_saturday_rolls() {
        let sat = day(2026, 8, 22);
        assert_eq!(parse_due("周五", sat).unwrap().1, day(2026, 8, 28));
        assert_eq!(parse_due("这周", sat).unwrap().1, day(2026, 8, 21));
    }

    #[test]
    fn md_and_iso() {
        let t = day(2026, 8, 17);
        assert_eq!(parse_due("8/20", t).unwrap().1, day(2026, 8, 20));
        assert_eq!(parse_due("8月20日", t).unwrap().1, day(2026, 8, 20));
        assert_eq!(parse_due("2026-08-20", t).unwrap().1, day(2026, 8, 20));
    }

    #[test]
    fn content_files_only() {
        assert!(is_content_deliverable("outputs/稿.docx"));
        assert!(is_content_deliverable("~/Desktop/热点.doc"));
        assert!(!is_content_deliverable("outputs/make_galbot_docx.py"));
        assert!(!is_content_deliverable("/tmp/outside-lebi.txt"));
        assert!(!is_content_deliverable(
            "/Users/a/Desktop/.lebi_write_test.txt"
        ));
    }
}
