use chrono::NaiveDate;
use hermes_commitments::{
    body_blank, evidence_user_prompt, fallback_review, format_markdown, invite_due, listed_reviews,
    load_prefs, parse_review_json, reviewed_span, save_prefs, span_range, today_local,
    write_review_file, CommitmentStore, REVIEW_SYSTEM,
};
use hermes_core::{can_use_main, CompletionRequest, ContentBlock, Message};
use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPrefsView {
    pub weekday: u8,
    pub default_span: String,
    pub invite_due: bool,
    pub reviewed: bool,
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResultView {
    pub markdown: String,
    pub focus: String,
    pub done: Vec<String>,
    pub outputs: Vec<String>,
    pub still_owe: Vec<String>,
    pub path: String,
    pub from: String,
    pub to: String,
    pub created_at: String,
    pub empty: bool,
}

#[tauri::command]
pub fn get_review_prefs() -> ReviewPrefsView {
    let prefs = load_prefs();
    let today = today_local();
    let (from, to) = span_range(&prefs.default_span, today);
    let reviewed = reviewed_span(from, to);
    ReviewPrefsView {
        weekday: prefs.weekday,
        default_span: prefs.default_span.clone(),
        invite_due: invite_due(&prefs, reviewed),
        reviewed,
        from: from.to_string(),
        to: to.to_string(),
    }
}

#[tauri::command]
pub fn list_reviews() -> Vec<ReviewResultView> {
    listed_reviews()
        .into_iter()
        .map(|e| {
            let markdown = std::fs::read_to_string(&e.path).unwrap_or_default();
            ReviewResultView {
                markdown,
                focus: e.focus,
                done: e.done,
                outputs: e.outputs,
                still_owe: e.still_owe,
                path: e.path,
                from: e.from,
                to: e.to,
                created_at: e.created_at,
                empty: false,
            }
        })
        .collect()
}

#[tauri::command]
pub fn set_review_prefs(weekday: u8, default_span: String) -> Result<ReviewPrefsView, GuiError> {
    let mut prefs = load_prefs();
    prefs.weekday = if weekday > 7 { 0 } else { weekday };
    if default_span == "last_7_days" || default_span == "this_week" {
        prefs.default_span = default_span;
    }
    save_prefs(&prefs).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(get_review_prefs())
}

#[tauri::command]
pub fn dismiss_review_invite() -> Result<ReviewPrefsView, GuiError> {
    let mut prefs = load_prefs();
    prefs.dismissed_date = Some(today_local().to_string());
    save_prefs(&prefs).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(get_review_prefs())
}

#[tauri::command]
pub async fn run_period_review(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<ReviewResultView, GuiError> {
    if !can_use_main() {
        return Err(GuiError::Config("license_locked".into()));
    }
    let from = NaiveDate::parse_from_str(&from, "%Y-%m-%d")
        .map_err(|_| GuiError::Internal("bad from date".into()))?;
    let to = NaiveDate::parse_from_str(&to, "%Y-%m-%d")
        .map_err(|_| GuiError::Internal("bad to date".into()))?;
    if to < from {
        return Err(GuiError::Internal("区间反了".into()));
    }

    let (sessions, outputs) = gather_sessions(from, to);
    let store = CommitmentStore::standard();
    let owed = store.list_owed().unwrap_or_default();
    let done = store.list_recent_done(21).unwrap_or_default();

    if sessions.is_empty() && outputs.is_empty() && owed.is_empty() && done.is_empty() {
        return Ok(ReviewResultView {
            markdown: String::new(),
            focus: String::new(),
            done: Vec::new(),
            outputs: Vec::new(),
            still_owe: owed.iter().map(|c| c.title.clone()).collect(),
            path: String::new(),
            from: from.to_string(),
            to: to.to_string(),
            created_at: String::new(),
            empty: true,
        });
    }

    let user = evidence_user_prompt(from, to, &sessions, &outputs, &owed, &done);
    let provider = state.provider.read().unwrap().clone();
    let req = CompletionRequest {
        model: String::new(),
        system: Some(REVIEW_SYSTEM.to_string()),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: 2048,
        temperature: Some(0.2),
        enable_caching: false,
    };
    let resp = provider
        .complete(req)
        .await
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    let parsed = parse_review_json(&resp.text());
    // Only keep outputs that appeared in evidence.
    let allowed: std::collections::HashSet<_> = outputs.iter().cloned().collect();
    let kept_outputs: Vec<String> = parsed
        .outputs
        .into_iter()
        .filter(|p| allowed.iter().any(|a| a == p || a.ends_with(p)))
        .collect();
    let owed_titles: std::collections::HashSet<_> = owed.iter().map(|c| c.title.as_str()).collect();
    let still_owe: Vec<String> = parsed
        .still_owe
        .into_iter()
        .filter(|t| owed_titles.contains(t.as_str()) || owed.iter().any(|c| c.title.contains(t)))
        .collect();
    let mut body = hermes_commitments::ReviewJson {
        focus: parsed.focus,
        done: parsed.done,
        outputs: kept_outputs,
        still_owe,
    };
    if body_blank(&body) {
        body = fallback_review(&sessions, &outputs, &owed, &done);
    }
    let path = write_review_file(from, to, &body).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(ReviewResultView {
        markdown: format_markdown(from, to, &body),
        focus: body.focus,
        done: body.done,
        outputs: body.outputs,
        still_owe: body.still_owe,
        path: path.to_string_lossy().into_owned(),
        from: from.to_string(),
        to: to.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        empty: false,
    })
}

fn gather_sessions(from: NaiveDate, to: NaiveDate) -> (Vec<(String, String)>, Vec<String>) {
    let root = hermes_core::data_path("sessions");
    let mut sessions = Vec::new();
    let mut outputs = Vec::new();
    let Ok(paths) = hermes_store::list_sessions(&root) else {
        return (sessions, outputs);
    };
    for path in paths {
        let Ok(s) = hermes_store::read_session(&path) else {
            continue;
        };
        let day = s.meta.created_at.date_naive();
        let last = hermes_core::last_human_send(&s.messages)
            .1
            .map(|t| t.date_naive());
        let in_range = (day >= from && day <= to) || last.is_some_and(|d| d >= from && d <= to);
        if !in_range {
            continue;
        }
        let title = s
            .meta
            .title
            .clone()
            .unwrap_or_else(|| hermes_core::derive_title_from_messages(&s.messages));
        let mut snip = String::new();
        for m in s.messages.iter().rev() {
            if !m.is_human_send() {
                continue;
            }
            for b in &m.content {
                if let Some(t) = b.as_text() {
                    let t = t.trim();
                    if t.is_empty() {
                        continue;
                    }
                    snip = t.chars().take(160).collect();
                    break;
                }
            }
            if !snip.is_empty() {
                break;
            }
        }
        sessions.push((title, snip));
        for m in &s.messages {
            for b in &m.content {
                if let ContentBlock::ToolUse { name, input, .. } = b {
                    if name == "write" || name == "edit" {
                        if let Some(p) = input
                            .get("path")
                            .or_else(|| input.get("file_path"))
                            .and_then(|v| v.as_str())
                        {
                            if hermes_commitments::is_content_deliverable(p)
                                && !outputs.iter().any(|x| x == p)
                            {
                                outputs.push(p.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    (sessions, outputs)
}
