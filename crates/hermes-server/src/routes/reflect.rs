//! Reflection REST — GUI reflect 能力的 HTTP 子集（供 Flutter）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use hermes_memory::{Confidence, FsMemoryStore, MemoryFrontmatter, MemoryStore, Scope, Source};
use hermes_reflect::{log_append, ActionTaken, CandidateKind, ReflectLogEntry};
use hermes_skills::{SkillFrontmatter, SkillStore};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
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

pub async fn run_reflection(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<ReflectionResult>, ApiError> {
    let sessions = state.sessions.lock().await;
    let active = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound("session not found".into()))?;
    // Clone the session out so we can drop the lock before the await.
    let session = active.session.clone();
    drop(sessions);

    let skills = state.skill_store.list().unwrap_or_default();
    let memories = state.memory_store.list_active().unwrap_or_default();

    let provider = state.provider.read().unwrap().clone();
    let output = hermes_reflect::reflect(provider.as_ref(), &session, &skills, &memories)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ReflectionResult {
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
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptSkillBody {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
}

pub async fn accept_skill_candidate(
    State(state): State<Arc<AppState>>,
    Json(b): Json<AcceptSkillBody>,
) -> Result<Json<()>, ApiError> {
    let fm = SkillFrontmatter {
        name: b.name,
        description: b.description,
        triggers: b.triggers,
        version: None,
        license: None,
        always_active: false,
        extra: Default::default(),
    };
    let skill_name = fm.name.clone();
    state
        .skill_store
        .put(hermes_skills::Scope::User, fm, &b.body)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    log_append(ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: "server:accept_skill".into(),
        kind: CandidateKind::Skill,
        action: ActionTaken::Accept,
        label: skill_name,
    });
    Ok(Json(()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptMemoryBody {
    pub fact: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub confidence: String,
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub zone: Option<String>,
}

pub async fn accept_memory_candidate(
    State(state): State<Arc<AppState>>,
    Json(b): Json<AcceptMemoryBody>,
) -> Result<Json<()>, ApiError> {
    let s = parse_scope(&b.scope);
    let conf = parse_confidence(&b.confidence);
    let zone = b
        .zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, b.tags, zone);
    fm.supersedes = b.supersedes;
    let label = b.fact.lines().next().unwrap_or("").to_string();
    put_memory_with_fallback(&state.memory_store, s, fm, &b.fact)?;
    log_append(ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: "server:accept_memory".into(),
        kind: CandidateKind::Memory,
        action: ActionTaken::Accept,
        label,
    });
    Ok(Json(()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandleConflictBody {
    pub fact: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub confidence: String,
    pub supersedes: Vec<String>,
    pub old_id: String,
    pub action: String,
    pub merged_body: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,
}

pub async fn handle_conflict(
    State(state): State<Arc<AppState>>,
    Json(b): Json<HandleConflictBody>,
) -> Result<Json<()>, ApiError> {
    let s = parse_scope(&b.scope);
    let conf = parse_confidence(&b.confidence);
    let zone = b
        .zone
        .map(|z| z.trim().to_string())
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| "general".to_string());
    let label = b.fact.lines().next().unwrap_or("").to_string();
    let log = |action: ActionTaken| {
        log_append(ReflectLogEntry {
            at: chrono::Utc::now(),
            session_id: "server:conflict".into(),
            kind: CandidateKind::ConflictMemory,
            action,
            label: label.clone(),
        });
    };

    match b.action.as_str() {
        "keep_new" => {
            let mut sup = b.supersedes;
            if !sup.iter().any(|id| id == &b.old_id) {
                sup.push(b.old_id);
            }
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, b.tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &b.fact)?;
            log(ActionTaken::Accept);
        }
        "merge" => {
            let body = b
                .merged_body
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .ok_or_else(|| ApiError::Internal("merge requires a non-empty body".into()))?;
            let mut sup = b.supersedes;
            if !sup.iter().any(|id| id == &b.old_id) {
                sup.push(b.old_id);
            }
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, b.tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &body)?;
            log(ActionTaken::Merge);
        }
        "scope_split" => {
            let opposite = match s {
                Scope::User => Scope::Project,
                Scope::Project => Scope::User,
            };
            let mut sup = b.supersedes;
            sup.retain(|id| id != &b.old_id);
            let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, b.tags, zone);
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, opposite, fm, &b.fact)?;
            log(ActionTaken::ScopeSplit);
        }
        "keep_old" | "skip" => log(ActionTaken::Reject),
        other => {
            return Err(ApiError::Internal(format!(
                "unknown conflict action: {other}"
            )));
        }
    }
    Ok(Json(()))
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
) -> Result<(), ApiError> {
    match store.put(scope, fm.clone(), body) {
        Ok(_) => Ok(()),
        Err(e) if matches!(scope, Scope::Project) => {
            tracing::warn!(error=%e, "project scope unavailable, falling back to user");
            store
                .put(Scope::User, fm, body)
                .map(|_| ())
                .map_err(|e| ApiError::Internal(e.to_string()))
        }
        Err(e) => Err(ApiError::Internal(e.to_string())),
    }
}
