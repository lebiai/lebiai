//! Skills REST. 1:1 with `hermes-gui/src/commands/skills.rs`.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use hermes_skills::{LoadedSkill, SkillFrontmatter, SkillStore};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkillItem {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub scope: String,
    pub body: String,
}

fn to_item(s: &LoadedSkill) -> SkillItem {
    SkillItem {
        name: s.frontmatter.name.clone(),
        description: s.frontmatter.description.clone(),
        triggers: s.frontmatter.triggers.clone(),
        scope: format!("{:?}", s.scope),
        body: s.body.clone(),
    }
}

fn parse_scope(scope: &str) -> hermes_skills::Scope {
    match scope {
        "Project" => hermes_skills::Scope::Project,
        _ => hermes_skills::Scope::User,
    }
}

#[derive(Deserialize)]
pub struct ScopeQuery {
    pub scope: Option<String>,
}

pub async fn list_skills(State(state): State<Arc<AppState>>) -> Result<Json<Vec<SkillItem>>, ApiError> {
    let skills = state
        .skill_store
        .list()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(skills.iter().map(to_item).collect()))
}

pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Option<SkillItem>>, ApiError> {
    let skill = state
        .skill_store
        .get(&name)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(skill.as_ref().map(to_item)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSkillBody {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
    pub scope: String,
}

pub async fn save_skill(
    State(state): State<Arc<AppState>>,
    Json(b): Json<SaveSkillBody>,
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
    state
        .skill_store
        .put(parse_scope(&b.scope), fm, &b.body)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(()))
}

pub async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<bool>, ApiError> {
    let scope = q.scope.as_deref().unwrap_or("User");
    state
        .skill_store
        .delete(parse_scope(scope), &name)
        .map_err(|e| ApiError::Internal(e.to_string()))
        .map(Json)
}
