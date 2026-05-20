use serde::Serialize;
use tauri::State;

use hermes_memory::{Confidence, FsMemoryStore, MemoryFrontmatter, MemoryStore, Scope, Source};
use hermes_skills::{SkillFrontmatter, SkillStore};

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

#[tauri::command]
pub async fn run_reflection(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ReflectionResult, GuiError> {
    let sessions = state.sessions.lock().await;
    let active = sessions
        .get(&session_id)
        .ok_or_else(|| GuiError::NotFound("session not found".into()))?;

    let skills = state.skill_store.list().unwrap_or_default();
    let memories = state.memory_store.list_active().unwrap_or_default();

    let output = hermes_reflect::reflect(
        state.provider.as_ref(),
        &active.session,
        &skills,
        &memories,
    )
    .await
    .map_err(|e| GuiError::Internal(e.to_string()))?;

    Ok(ReflectionResult {
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
    })
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
    state
        .skill_store
        .put(hermes_skills::Scope::User, fm, &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
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
) -> Result<(), GuiError> {
    let s = parse_scope(&scope);
    let conf = parse_confidence(&confidence);
    let mut fm = MemoryFrontmatter::new(Source::Reflection, conf, tags, "general".to_string());
    fm.supersedes = supersedes;
    put_memory_with_fallback(&state.memory_store, s, fm, &fact)?;
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
) -> Result<(), GuiError> {
    let s = parse_scope(&scope);
    let conf = parse_confidence(&confidence);

    match action.as_str() {
        "keep_new" => {
            let mut sup = supersedes;
            if !sup.iter().any(|id| id == &old_id) {
                sup.push(old_id);
            }
            let mut fm =
                MemoryFrontmatter::new(Source::Reflection, conf, tags, "general".to_string());
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &fact)?;
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
            let mut fm =
                MemoryFrontmatter::new(Source::Reflection, conf, tags, "general".to_string());
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, s, fm, &body)?;
        }
        "scope_split" => {
            let opposite = match s {
                Scope::User => Scope::Project,
                Scope::Project => Scope::User,
            };
            let mut sup = supersedes;
            sup.retain(|id| id != &old_id);
            let mut fm =
                MemoryFrontmatter::new(Source::Reflection, conf, tags, "general".to_string());
            fm.supersedes = sup;
            put_memory_with_fallback(&state.memory_store, opposite, fm, &fact)?;
        }
        "keep_old" | "skip" => {
            // No write — user chose to keep the existing memory or drop the candidate.
        }
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
