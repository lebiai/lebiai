use serde::Serialize;
use tauri::State;

use hermes_skills::{SkillFrontmatter, SkillStore};

use crate::error::GuiError;
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

fn to_item(s: &hermes_skills::LoadedSkill) -> SkillItem {
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

#[tauri::command]
pub fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillItem>, GuiError> {
    let skills = state.skill_store.list().map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(skills.iter().map(to_item).collect())
}

#[tauri::command]
pub fn get_skill(state: State<'_, AppState>, name: String) -> Result<Option<SkillItem>, GuiError> {
    let skill = state
        .skill_store
        .get(&name)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(skill.as_ref().map(to_item))
}

#[tauri::command]
pub fn save_skill(
    state: State<'_, AppState>,
    name: String,
    description: String,
    triggers: Vec<String>,
    body: String,
    scope: String,
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
        .put(parse_scope(&scope), fm, &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn delete_skill(
    state: State<'_, AppState>,
    name: String,
    scope: String,
) -> Result<bool, GuiError> {
    state
        .skill_store
        .delete(parse_scope(&scope), &name)
        .map_err(|e| GuiError::Internal(e.to_string()))
}
