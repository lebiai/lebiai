use serde::Serialize;
use tauri::State;

use hermes_skills::SkillStore;

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
pub fn delete_skill(
    state: State<'_, AppState>,
    name: String,
    scope: String,
) -> Result<bool, GuiError> {
    let s = match scope.as_str() {
        "Project" => hermes_skills::Scope::Project,
        _ => hermes_skills::Scope::User,
    };
    state
        .skill_store
        .delete(s, &name)
        .map_err(|e| GuiError::Internal(e.to_string()))
}
