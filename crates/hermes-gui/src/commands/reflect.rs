use serde::Serialize;
use tauri::State;

use hermes_memory::{MemoryFrontmatter, MemoryStore, Source};
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
) -> Result<(), GuiError> {
    let s = match scope.as_str() {
        "Project" => hermes_memory::Scope::Project,
        _ => hermes_memory::Scope::User,
    };
    let conf = match confidence.as_str() {
        "Low" => hermes_memory::Confidence::Low,
        "High" => hermes_memory::Confidence::High,
        _ => hermes_memory::Confidence::Medium,
    };
    let fm = MemoryFrontmatter::new(Source::Reflection, conf, tags);
    state
        .memory_store
        .put(s, fm, &fact)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(())
}
