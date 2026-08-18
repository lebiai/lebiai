use std::path::Path;
use std::time::Duration;

use hermes_core::{last_human_send, LlmProvider, Session};
use hermes_memory::{Confidence, FsMemoryStore, MemoryFrontmatter, MemoryStore, Scope, Source};
use hermes_reflect::{log_append, ActionTaken, CandidateKind, ReflectLogEntry, ReflectionOutput};
use hermes_skills::{FsSkillStore, SkillFrontmatter, SkillStore};
use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionResult {
    pub summary: String,
    pub skill_candidates: Vec<SkillCandidateView>,
    pub memory_candidates: Vec<MemoryCandidateView>,
    pub conflicts: Vec<ConflictView>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkillCandidateView {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
    pub rationale: String,
    pub confidence: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCandidateView {
    pub fact: String,
    pub tags: Vec<String>,
    pub zone: String,
    pub scope: String,
    pub confidence: String,
    pub rationale: String,
    pub supersedes: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub with: String,
    pub kind: String,
    pub explain: String,
    pub options: Vec<String>,
}

/// Outcome of quit-driven / leave-session reflection.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum SessionEndReflectionOutcome {
    /// Below `reflect.min_turns`, empty session, unchanged since last distill.
    Skipped {
        reason: String,
        user_turns: usize,
        min_turns: usize,
    },
    /// Quiet path (default): quality candidates enqueued to pending-review inbox.
    Enqueued {
        /// Newly added this run (after quality gate + dedup).
        added: usize,
        /// Total items waiting in the inbox.
        total: usize,
    },
    /// Legacy noisy path when `reflect.pop_inbox_on_leave = true`.
    Completed { reflection: ReflectionResult },
}

fn count_user_text_turns(session: &Session) -> usize {
    session
        .messages
        .iter()
        .filter(|m| m.is_human_send())
        .count()
}

fn reflection_result_from_output(output: ReflectionOutput) -> ReflectionResult {
    ReflectionResult {
        summary: output.summary,
        skill_candidates: output
            .skill_candidates
            .iter()
            .map(|c| SkillCandidateView {
                name: c.name.clone(),
                description: c.description.clone(),
                triggers: c.triggers.clone(),
                body: c.body.clone(),
                rationale: c.rationale.clone(),
                confidence: format!("{:?}", c.confidence),
            })
            .collect(),
        memory_candidates: output
            .memory_candidates
            .iter()
            .map(|c| MemoryCandidateView {
                fact: c.fact.clone(),
                tags: c.tags.clone(),
                zone: c.zone.clone(),
                scope: format!("{:?}", c.scope),
                confidence: format!("{:?}", c.confidence),
                rationale: c.rationale.clone(),
                supersedes: c.supersedes.clone(),
            })
            .collect(),
        conflicts: output
            .conflicts
            .iter()
            .map(|c| ConflictView {
                with: c.with.clone(),
                kind: c.kind.clone(),
                explain: c.explain.clone(),
                options: c.options.clone(),
            })
            .collect(),
    }
}

async fn reflect_session_output(
    state: &AppState,
    session_id: &str,
    quick: bool,
) -> Result<hermes_reflect::ReflectionOutput, GuiError> {
    let session = {
        let sessions = state.sessions.lock().await;
        let active = sessions
            .get(session_id)
            .ok_or_else(|| GuiError::NotFound("session not found".into()))?;
        active.session.clone()
    };

    let skills = state.skill_store.list().unwrap_or_default();
    let memories = state.memory_store.list_active().unwrap_or_default();

    let provider = state.provider.read().unwrap().clone();
    let output = if quick {
        hermes_reflect::reflect_quick(provider.as_ref(), &session, &skills, &memories).await
    } else {
        hermes_reflect::reflect(provider.as_ref(), &session, &skills, &memories).await
    }
    .map_err(|e| GuiError::Internal(e.to_string()))?;

    Ok(output)
}

#[tauri::command]
pub async fn run_reflection(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ReflectionResult, GuiError> {
    // Manual Reflect panel: full budget, no min_turns gate; also enqueue quietly.
    let output = reflect_session_output(&state, &session_id, false).await?;
    let _ = hermes_reflect::enqueue_from_reflection(
        &output,
        hermes_reflect::InboxSource::ManualReflect,
    );
    Ok(reflection_result_from_output(output))
}

/// Cheap check before showing the "tidying…" banner.
#[tauri::command]
pub async fn session_needs_distill(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, GuiError> {
    let min_turns = state.config.read().unwrap().reflect.min_turns;
    let sessions = state.sessions.lock().await;
    let Some(active) = sessions.get(&session_id) else {
        return Ok(false);
    };
    if active.session.messages.is_empty() {
        return Ok(false);
    }
    if count_user_text_turns(&active.session) < min_turns {
        return Ok(false);
    }
    if hermes_store::channel_of_session_path(&active.path).is_some() {
        // Idle scan owns channel logs — leaving the viewer is not a leave-work.
        return Ok(false);
    }
    Ok(hermes_reflect::needs_distill(&active.session))
}

/// Leave-session path: quiet by default — reflect in background, enqueue to inbox.
/// Set `reflect.pop_inbox_on_leave = true` for legacy modal review.
#[tauri::command]
pub async fn run_session_end_reflection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionEndReflectionOutcome, GuiError> {
    let min_turns = state.config.read().unwrap().reflect.min_turns;
    let pop_on_leave = state.config.read().unwrap().reflect.pop_inbox_on_leave;

    let session_snapshot = {
        let sessions = state.sessions.lock().await;
        let active = sessions
            .get(&session_id)
            .ok_or_else(|| GuiError::NotFound("session not found".into()))?;
        if active.session.messages.is_empty() {
            return Ok(SessionEndReflectionOutcome::Skipped {
                reason: "empty_session".into(),
                user_turns: 0,
                min_turns,
            });
        }
        if hermes_store::channel_of_session_path(&active.path).is_some() {
            return Ok(SessionEndReflectionOutcome::Skipped {
                reason: "channel_idle_owns".into(),
                user_turns: count_user_text_turns(&active.session),
                min_turns,
            });
        }
        active.session.clone()
    };

    let user_turns = count_user_text_turns(&session_snapshot);
    if user_turns < min_turns {
        tracing::info!(
            user_turns,
            min_turns,
            "GUI session-end reflection skipped (below min_turns)"
        );
        return Ok(SessionEndReflectionOutcome::Skipped {
            reason: "below_min_turns".into(),
            user_turns,
            min_turns,
        });
    }

    // Independent of distill: leftover debts, quiet suggested rows.
    crate::commands::commitment::spawn_residue_scan(app, &state, session_snapshot.clone());

    if !hermes_reflect::needs_distill(&session_snapshot) {
        tracing::info!(
            session_id = %session_id,
            "GUI session-end reflection skipped (unchanged since last distill)"
        );
        return Ok(SessionEndReflectionOutcome::Skipped {
            reason: "unchanged".into(),
            user_turns,
            min_turns,
        });
    }

    let Some(_distill_lock) = hermes_reflect::DistillSessionGuard::try_acquire(&session_id) else {
        return Ok(SessionEndReflectionOutcome::Skipped {
            reason: "in_progress".into(),
            user_turns,
            min_turns,
        });
    };

    const SESSION_END_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(75);

    let output = match tokio::time::timeout(
        SESSION_END_TIMEOUT,
        reflect_session_output(&state, &session_id, true),
    )
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "session-end reflection failed; no invented memories");
            return Ok(SessionEndReflectionOutcome::Skipped {
                reason: "failed".into(),
                user_turns,
                min_turns,
            });
        }
        Err(_) => {
            tracing::warn!("session-end reflection timed out; no invented memories");
            return Ok(SessionEndReflectionOutcome::Skipped {
                reason: "timeout".into(),
                user_turns,
                min_turns,
            });
        }
    };

    let distill_id = hermes_reflect::new_distill_id();
    let (_seq, through_at) = hermes_core::last_human_send(&session_snapshot.messages);
    let through_at_s = through_at.map(|t| t.to_rfc3339());
    let mark = hermes_reflect::EnqueueMark {
        session_id: Some(session_snapshot.meta.id.clone()),
        distill_id: Some(distill_id.clone()),
        through_at: through_at_s,
    };

    let added = hermes_reflect::enqueue_from_reflection_marked(
        &output,
        hermes_reflect::InboxSource::SessionEnd,
        mark,
    )
    .map_err(|e| GuiError::Internal(e.to_string()))?;
    let outcome = if added > 0 { "enqueued" } else { "empty" };
    hermes_reflect::record_success(&session_snapshot, &distill_id, outcome);
    let total = hermes_reflect::inbox_count().unwrap_or(0);

    if pop_on_leave {
        let reflection = reflection_result_from_output(output);
        if reflection_view_has_candidates(&reflection) {
            return Ok(SessionEndReflectionOutcome::Completed { reflection });
        }
    }

    Ok(SessionEndReflectionOutcome::Enqueued { added, total })
}

fn reflection_view_has_candidates(r: &ReflectionResult) -> bool {
    !r.skill_candidates.is_empty() || !r.memory_candidates.is_empty() || !r.conflicts.is_empty()
}

/// Scan WeChat logs that have been quiet long enough and distill them.
/// Sidebar `list_sessions` may fire often — one scan at a time, 60s cooldown.
pub fn spawn_idle_wechat_distill(state: &AppState) {
    const COOLDOWN: Duration = Duration::from_secs(60);
    if !state.idle_wechat_distill.try_begin(COOLDOWN) {
        return;
    }
    let provider = state.provider.read().unwrap().clone();
    let memory = state.memory_store.clone();
    let skills = state.skill_store.clone();
    let min_turns = state.config.read().unwrap().reflect.min_turns;
    let gate = state.idle_wechat_distill.clone();
    tokio::spawn(async move {
        let _guard = IdleDistillFinish(gate);
        let root = hermes_core::data_path("sessions").join("wechat");
        if !root.exists() {
            return;
        }
        let Ok(paths) = hermes_store::list_sessions(&root) else {
            return;
        };
        for path in paths {
            if let Err(e) = distill_wechat_file_if_idle(
                &path,
                provider.as_ref(),
                memory.as_ref(),
                skills.as_ref(),
                min_turns,
            )
            .await
            {
                tracing::debug!(error = %e, path = %path.display(), "idle wechat distill skipped");
            }
        }
    });
}

struct IdleDistillFinish(std::sync::Arc<crate::state::IdleDistillGate>);

impl Drop for IdleDistillFinish {
    fn drop(&mut self) {
        self.0.finish();
    }
}

async fn distill_wechat_file_if_idle(
    path: &Path,
    provider: &dyn LlmProvider,
    memory: &FsMemoryStore,
    skills: &FsSkillStore,
    min_turns: usize,
) -> anyhow::Result<()> {
    let session = hermes_store::read_session(path)?;
    if count_user_text_turns(&session) < min_turns {
        return Ok(());
    }
    if !hermes_reflect::needs_distill(&session) {
        return Ok(());
    }
    if !wechat_session_is_idle(&session, path) {
        return Ok(());
    }
    let Some(_lock) = hermes_reflect::DistillSessionGuard::try_acquire(&session.meta.id) else {
        return Ok(());
    };
    let skill_list = skills.list().unwrap_or_default();
    let mem_list = memory.list_active().unwrap_or_default();
    let output = tokio::time::timeout(
        Duration::from_secs(75),
        hermes_reflect::reflect_quick(provider, &session, &skill_list, &mem_list),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timeout"))??;
    let distill_id = hermes_reflect::new_distill_id();
    let (_seq, through_at) = last_human_send(&session.messages);
    let mark = hermes_reflect::EnqueueMark {
        session_id: Some(session.meta.id.clone()),
        distill_id: Some(distill_id.clone()),
        through_at: through_at.map(|t| t.to_rfc3339()),
    };
    let added = hermes_reflect::enqueue_from_reflection_marked(
        &output,
        hermes_reflect::InboxSource::SessionEnd,
        mark,
    )?;
    hermes_reflect::record_success(
        &session,
        &distill_id,
        if added > 0 { "enqueued" } else { "empty" },
    );
    Ok(())
}

fn wechat_session_is_idle(session: &Session, path: &Path) -> bool {
    let (_seq, at) = last_human_send(&session.messages);
    if let Some(at) = at {
        return chrono::Utc::now().signed_duration_since(at).num_seconds() >= 15 * 60;
    }
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|e| e.as_secs() >= 15 * 60)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::{ContentBlock, Message, Role, SessionMeta};

    #[test]
    fn count_user_text_turns_ignores_non_text_user() {
        let mut session = Session::new(SessionMeta::new("m", "p"));
        session.messages.push(Message::user_text("hi"));
        session.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            at: None,
        });
        session.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: "ok".into(),
                is_error: false,
            }],
            at: None,
        });
        session.messages.push(Message::user_text("again"));
        assert_eq!(count_user_text_turns(&session), 2);
    }
}

#[tauri::command]
pub async fn accept_skill_candidate(
    state: State<'_, AppState>,
    name: String,
    description: String,
    triggers: Vec<String>,
    body: String,
) -> Result<(), GuiError> {
    let fm = SkillFrontmatter {
        name,
        description,
        triggers,
        version: None,
        license: None,
        always_active: false,
        extra: Default::default(),
    };
    let skill_name = fm.name.clone();
    state
        .skill_store
        .put(hermes_skills::Scope::User, fm, &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    log_append(ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: "gui:accept_skill".into(),
        kind: CandidateKind::Skill,
        action: ActionTaken::Accept,
        label: skill_name,
    });
    Ok(())
}

#[tauri::command]
pub async fn accept_memory_candidate(
    state: State<'_, AppState>,
    fact: String,
    tags: Vec<String>,
    scope: String,
    confidence: String,
    supersedes: Vec<String>,
    zone: Option<String>,
) -> Result<(), GuiError> {
    let s = parse_scope(&scope);
    let conf = parse_confidence(&confidence);
    let zone = zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, tags, zone);
    fm.supersedes = supersedes;
    let label = fact.lines().next().unwrap_or("").to_string();
    put_memory_with_fallback(&state.memory_store, s, fm, &fact)?;
    log_append(ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: "gui:accept_memory".into(),
        kind: CandidateKind::Memory,
        action: ActionTaken::Accept,
        label,
    });
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn handle_conflict(
    state: State<'_, AppState>,
    fact: String,
    tags: Vec<String>,
    scope: String,
    confidence: String,
    supersedes: Vec<String>,
    old_id: String,
    action: String,
    merged_body: Option<String>,
    zone: Option<String>,
) -> Result<(), GuiError> {
    let s = parse_scope(&scope);
    let conf = parse_confidence(&confidence);
    let zone = zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let label = fact.lines().next().unwrap_or("").to_string();
    let log = |action: ActionTaken| {
        log_append(ReflectLogEntry {
            at: chrono::Utc::now(),
            session_id: "gui:conflict".into(),
            kind: CandidateKind::ConflictMemory,
            action,
            label: label.clone(),
        });
    };

    match action.as_str() {
        "keep_new" => {
            let mut sup = supersedes;
            if !sup.iter().any(|id| id == &old_id) {
                sup.push(old_id);
            }
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &fact)?;
            log(ActionTaken::Accept);
        }
        "merge" => {
            let body = merged_body
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty())
                .ok_or_else(|| GuiError::Internal("merge requires a non-empty body".into()))?;
            let mut sup = supersedes;
            if !sup.iter().any(|id| id == &old_id) {
                sup.push(old_id);
            }
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &body)?;
            log(ActionTaken::Merge);
        }
        "scope_split" => {
            let opposite = match s {
                Scope::User => Scope::Project,
                Scope::Project => Scope::User,
            };
            let mut sup = supersedes;
            sup.retain(|id| id != &old_id);
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, opposite, fm, &fact)?;
            log(ActionTaken::ScopeSplit);
        }
        "keep_old" | "skip" => log(ActionTaken::Reject),
        other => {
            return Err(GuiError::Internal(format!(
                "unknown conflict action: {other}"
            )));
        }
    }
    Ok(())
}

fn parse_scope(scope: &str) -> Scope {
    match scope {
        "Project" => Scope::Project,
        _ => Scope::User,
    }
}

fn parse_confidence(confidence: &str) -> Confidence {
    match confidence {
        "Low" => Confidence::Low,
        "High" => Confidence::High,
        _ => Confidence::Medium,
    }
}

fn put_memory_with_fallback(
    store: &FsMemoryStore,
    scope: Scope,
    fm: MemoryFrontmatter,
    body: &str,
) -> Result<(), GuiError> {
    match store.put(scope, fm.clone(), body) {
        Ok(_) => Ok(()),
        Err(e) if matches!(scope, Scope::Project) => {
            tracing::warn!(error=%e, "project scope unavailable, falling back to user");
            store
                .put(Scope::User, fm, body)
                .map(|_| ())
                .map_err(|e| GuiError::Internal(e.to_string()))
        }
        Err(e) => Err(GuiError::Internal(e.to_string())),
    }
}
