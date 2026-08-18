use hermes_commitments::{
    parse_due, scan_merge_pairs, scan_residue, session_has_owe_language, Commitment,
    CommitmentError, SaveMode, SaveOutcome, Source, Status, OPEN_CROWD,
};
use hermes_core::Session;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitmentView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub done_when: Option<String>,
    pub soft_due: Option<String>,
    pub note: Option<String>,
    pub session_id: Option<String>,
    pub overdue: bool,
    pub due_today: bool,
    pub due_date: Option<String>,
}

impl From<&Commitment> for CommitmentView {
    fn from(c: &Commitment) -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            id: c.id.clone(),
            title: c.title.clone(),
            status: match c.status {
                Status::Open => "open".into(),
                Status::Suggested => "suggested".into(),
                Status::Waiting => "waiting".into(),
                Status::Done => "done".into(),
                Status::Dropped => "dropped".into(),
            },
            done_when: c.done_when.clone(),
            soft_due: c.soft_due.clone(),
            note: c.note.clone(),
            session_id: c.session_id.clone(),
            overdue: c.is_overdue(today),
            due_today: c.is_due_today(today),
            due_date: c.soft_due_date.map(|d| d.to_string()),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MergeHint {
    pub keep_id: String,
    pub keep_title: String,
    pub other_id: String,
    pub other_title: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitmentList {
    pub items: Vec<CommitmentView>,
    pub owed_count: usize,
    pub overdue_count: usize,
    pub crowded: bool,
    #[serde(default)]
    pub recent_done: Vec<CommitmentView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_hint: Option<MergeHint>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CreateCommitmentOutcome {
    Created {
        item: CommitmentView,
    },
    Near {
        existing: CommitmentView,
        score: f64,
    },
}

#[tauri::command]
pub fn list_commitments(state: State<'_, AppState>) -> Result<CommitmentList, GuiError> {
    let live = state
        .commitment_store
        .list_live()
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    let today = chrono::Local::now().date_naive();
    let owed_count = live.iter().filter(|c| c.status.is_owed()).count();
    let overdue_count = live.iter().filter(|c| c.is_overdue(today)).count();
    let merge_hint =
        if let Some((a, b, _)) = state.commitment_store.lexical_merge_pair().ok().flatten() {
            Some(MergeHint {
                keep_id: a.id,
                keep_title: a.title,
                other_id: b.id,
                other_title: b.title,
            })
        } else if let Ok(Some((aid, bid))) = state.commitment_store.semantic_pair() {
            let a = live.iter().find(|c| c.id == aid);
            let b = live.iter().find(|c| c.id == bid);
            match (a, b) {
                (Some(a), Some(b)) if a.status.is_owed() && b.status.is_owed() => Some(MergeHint {
                    keep_id: a.id.clone(),
                    keep_title: a.title.clone(),
                    other_id: b.id.clone(),
                    other_title: b.title.clone(),
                }),
                _ => None,
            }
        } else {
            None
        };
    let recent_done = state
        .commitment_store
        .list_recent_done(14)
        .unwrap_or_default();
    Ok(CommitmentList {
        items: live.iter().map(CommitmentView::from).collect(),
        owed_count,
        overdue_count,
        crowded: owed_count >= OPEN_CROWD,
        recent_done: recent_done
            .iter()
            .take(12)
            .map(CommitmentView::from)
            .collect(),
        merge_hint,
    })
}

fn apply_due(item: &mut Commitment, phrase: Option<String>) -> Result<(), GuiError> {
    let Some(phrase) = phrase
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return Err(GuiError::Internal("due_required".into()));
    };
    let today = chrono::Local::now().date_naive();
    let (kept, date) = parse_due(&phrase, today).map_err(|e| match e {
        hermes_commitments::DueError::Vague => GuiError::Internal("due_vague".into()),
        hermes_commitments::DueError::Unparsed => GuiError::Internal("due_required".into()),
    })?;
    item.soft_due = Some(kept);
    item.soft_due_date = Some(date);
    Ok(())
}

fn map_store(e: CommitmentError) -> GuiError {
    match e {
        CommitmentError::MissingDue => GuiError::Internal("due_required".into()),
        CommitmentError::VagueDue => GuiError::Internal("due_vague".into()),
        other => GuiError::Internal(other.to_string()),
    }
}

#[tauri::command]
pub fn create_commitment(
    state: State<'_, AppState>,
    title: String,
    merge_into: Option<String>,
    force_new: bool,
    session_id: Option<String>,
    soft_due: Option<String>,
) -> Result<CreateCommitmentOutcome, GuiError> {
    if let Some(id) = merge_into.as_deref() {
        let mut incoming =
            Commitment::new(title, Source::User).map_err(|e| GuiError::Internal(e.to_string()))?;
        let _ = apply_due(&mut incoming, soft_due);
        return match state
            .commitment_store
            .fold_into(id, &incoming)
            .map_err(map_store)?
        {
            SaveOutcome::Folded { into } => Ok(CreateCommitmentOutcome::Created {
                item: CommitmentView::from(&into),
            }),
            _ => Err(GuiError::Internal("merge failed".into())),
        };
    }
    let mut item =
        Commitment::new(title, Source::User).map_err(|e| GuiError::Internal(e.to_string()))?;
    apply_due(&mut item, soft_due)?;
    item.session_id = session_id.filter(|s| !s.trim().is_empty());
    let mode = if force_new {
        SaveMode::ForceNew
    } else {
        SaveMode::Ask
    };
    match state.commitment_store.save(item, mode).map_err(map_store)? {
        SaveOutcome::Created(c) | SaveOutcome::Folded { into: c } => {
            Ok(CreateCommitmentOutcome::Created {
                item: CommitmentView::from(&c),
            })
        }
        SaveOutcome::Near { existing, score } => Ok(CreateCommitmentOutcome::Near {
            existing: CommitmentView::from(&existing),
            score,
        }),
    }
}

#[tauri::command]
pub fn accept_commitment(
    state: State<'_, AppState>,
    id: String,
    soft_due: Option<String>,
) -> Result<CommitmentView, GuiError> {
    if let Some(due) = soft_due.filter(|s| !s.trim().is_empty()) {
        state
            .commitment_store
            .patch_soft_due(&id, due)
            .map_err(map_store)?;
    }
    let c = state
        .commitment_store
        .accept_suggested(&id)
        .map_err(map_store)?;
    Ok(CommitmentView::from(&c))
}

#[tauri::command]
pub fn reject_commitment(
    state: State<'_, AppState>,
    id: String,
) -> Result<CommitmentView, GuiError> {
    let c = state
        .commitment_store
        .reject_suggested(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(CommitmentView::from(&c))
}

#[tauri::command]
pub fn close_commitment(
    state: State<'_, AppState>,
    id: String,
    dropped: bool,
) -> Result<CommitmentView, GuiError> {
    let status = if dropped {
        Status::Dropped
    } else {
        Status::Done
    };
    let c = state
        .commitment_store
        .close(&id, status)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(CommitmentView::from(&c))
}

#[tauri::command]
pub fn merge_commitments(
    state: State<'_, AppState>,
    keep_id: String,
    other_id: String,
) -> Result<CommitmentView, GuiError> {
    let c = state
        .commitment_store
        .merge_ids(&keep_id, &other_id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(CommitmentView::from(&c))
}

#[tauri::command]
pub fn split_commitment(
    state: State<'_, AppState>,
    id: String,
    titles: Vec<String>,
) -> Result<Vec<CommitmentView>, GuiError> {
    let out = state
        .commitment_store
        .split(&id, &titles)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(out.iter().map(CommitmentView::from).collect())
}

#[tauri::command]
pub fn update_commitment(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    done_when: Option<String>,
    soft_due: Option<String>,
    note: Option<String>,
    waiting: Option<bool>,
) -> Result<CommitmentView, GuiError> {
    if let Some(t) = title.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        state
            .commitment_store
            .retitle(&id, t.to_string())
            .map_err(|e| GuiError::Internal(e.to_string()))?;
    }
    if done_when.is_some() {
        state
            .commitment_store
            .set_done_when(&id, done_when)
            .map_err(|e| GuiError::Internal(e.to_string()))?;
    }
    if let Some(due) = soft_due.filter(|s| !s.trim().is_empty()) {
        state
            .commitment_store
            .patch_soft_due(&id, due)
            .map_err(map_store)?;
    }
    match waiting {
        Some(true) => {
            state
                .commitment_store
                .set_waiting(&id, note)
                .map_err(|e| GuiError::Internal(e.to_string()))?;
        }
        Some(false) => {
            state
                .commitment_store
                .reopen(&id)
                .map_err(|e| GuiError::Internal(e.to_string()))?;
            if note.is_some() {
                state
                    .commitment_store
                    .patch_note(&id, note)
                    .map_err(|e| GuiError::Internal(e.to_string()))?;
            }
        }
        None if note.is_some() => {
            state
                .commitment_store
                .patch_note(&id, note)
                .map_err(|e| GuiError::Internal(e.to_string()))?;
        }
        None => {}
    }
    let c = state
        .commitment_store
        .get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?
        .ok_or_else(|| GuiError::Internal("not found".into()))?;
    Ok(CommitmentView::from(&c))
}

#[tauri::command]
pub fn find_session_path(
    _state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<String>, GuiError> {
    let root = hermes_core::data_path("sessions");
    if !root.exists() {
        return Ok(None);
    }
    let paths =
        hermes_store::list_sessions(&root).map_err(|e| GuiError::Internal(e.to_string()))?;
    for path in paths {
        if let Ok(s) = hermes_store::read_session(&path) {
            if s.meta.id == session_id {
                return Ok(Some(path.to_string_lossy().into_owned()));
            }
        }
    }
    Ok(None)
}

/// Quiet leave-session scan. Never blocks leaving. 0–1 suggested rows.
pub fn spawn_residue_scan(app: AppHandle, state: &AppState, session: Session) {
    if !session_has_owe_language(&session) {
        return;
    }
    let store = state.commitment_store.clone();
    let provider = state.provider.read().unwrap().clone();
    tokio::spawn(async move {
        let existing = match store.list_live() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "residue: cannot list open work");
                return;
            }
        };
        let items = match scan_residue(provider.as_ref(), &session, &existing).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "residue scan failed");
                return;
            }
        };
        let mut added = 0u32;
        for it in items {
            let Ok(mut c) = Commitment::new(it.title, Source::Residue) else {
                continue;
            };
            c.status = Status::Suggested;
            c.suggested_at = Some(chrono::Utc::now());
            c.session_id = Some(session.meta.id.clone());
            c.done_when = it.done_when.filter(|s| !s.trim().is_empty());
            c.soft_due = it.soft_due.filter(|s| !s.trim().is_empty());
            c.soft_due_date = it
                .soft_due_date
                .as_deref()
                .and_then(|s| chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
            match store.save(c, SaveMode::Ask) {
                Ok(SaveOutcome::Created(_)) => added += 1,
                Ok(SaveOutcome::Near { .. } | SaveOutcome::Folded { .. }) => {}
                Err(e) => tracing::warn!(error = %e, "residue save skipped"),
            }
        }
        if added > 0 {
            let _ = app.emit("hermes://zaiban-changed", added);
        }
        // Semantic same-debt pair (conservative). Result is only a cue via refresh.
        if let Ok(live) = store.list_live() {
            match scan_merge_pairs(provider.as_ref(), &live).await {
                Ok(pair) => {
                    let _ = store.set_semantic_pair(pair);
                    let _ = app.emit("hermes://zaiban-changed", 0u32);
                }
                Err(e) => tracing::warn!(error = %e, "merge-pair scan failed"),
            }
        }
    });
}
