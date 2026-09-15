//! Pending-review inbox Tauri commands (quiet evolution queue).

use hermes_reflect::{
    log_append, ActionTaken, CandidateKind, InboxItem, InboxPayload, InboxSource, ReflectLogEntry,
};
use hermes_skills::{SkillFrontmatter, SkillStore};
use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemView {
    pub id: String,
    pub created_at: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub zone: Option<String>,
    pub tags: Vec<String>,
    pub confidence: Option<String>,
    pub rationale: Option<String>,
    pub skill_name: Option<String>,
    pub skill_description: Option<String>,
    pub skill_triggers: Option<Vec<String>>,
}

fn source_label(s: InboxSource) -> &'static str {
    match s {
        InboxSource::SessionEnd => "session_end",
        InboxSource::Micro => "micro",
        InboxSource::ManualReflect => "manual",
    }
}

fn item_to_view(item: InboxItem) -> InboxItemView {
    match item.payload {
        InboxPayload::Memory(c) => InboxItemView {
            id: item.id,
            created_at: item.created_at.to_rfc3339(),
            source: source_label(item.source).into(),
            kind: "memory".into(),
            title: c
                .fact
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect(),
            body: c.fact,
            zone: Some(c.zone),
            tags: c.tags,
            confidence: Some(format!("{:?}", c.confidence)),
            rationale: Some(c.rationale),
            skill_name: None,
            skill_description: None,
            skill_triggers: None,
        },
        InboxPayload::Skill(c) => InboxItemView {
            id: item.id,
            created_at: item.created_at.to_rfc3339(),
            source: source_label(item.source).into(),
            kind: "skill".into(),
            title: c.name.clone(),
            body: c.body,
            zone: None,
            tags: c.triggers.clone(),
            confidence: Some(format!("{:?}", c.confidence)),
            rationale: Some(c.rationale),
            skill_name: Some(c.name),
            skill_description: Some(c.description),
            skill_triggers: Some(c.triggers),
        },
    }
}

#[tauri::command]
pub async fn list_pending_review() -> Result<Vec<InboxItemView>, GuiError> {
    let items = hermes_reflect::inbox_list().map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(items.into_iter().map(item_to_view).collect())
}

#[tauri::command]
pub async fn count_pending_review() -> Result<usize, GuiError> {
    hermes_reflect::inbox_count().map_err(|e| GuiError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn reject_pending_review(id: String) -> Result<(), GuiError> {
    let item = hermes_reflect::inbox_get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?
        .ok_or_else(|| GuiError::NotFound(format!("pending item {id}")))?;
    log_inbox_action(&item, ActionTaken::Reject);
    hermes_reflect::inbox_remove(&id).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn accept_pending_review(state: State<'_, AppState>, id: String) -> Result<(), GuiError> {
    let item = hermes_reflect::inbox_get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?
        .ok_or_else(|| GuiError::NotFound(format!("pending item {id}")))?;
    log_inbox_action(&item, ActionTaken::Accept);

    match &item.payload {
        InboxPayload::Memory(c) => {
            // 归属的判定与落盘都在 `hermes-reflect` 里，GUI / server / CLI 同一份实现。
            hermes_reflect::inbox_accept_memory_item(state.memory_store.as_ref(), &item, c)
                .map_err(|e| GuiError::Internal(e.to_string()))?;
        }
        InboxPayload::Skill(c) => {
            let fm = SkillFrontmatter {
                name: c.name.clone(),
                description: c.description.clone(),
                triggers: c.triggers.clone(),
                version: None,
                license: None,
                always_active: false,
                extra: Default::default(),
            };
            state
                .skill_store
                .put(hermes_skills::Scope::User, fm, &c.body)
                .map_err(|e| GuiError::Internal(e.to_string()))?;
        }
    }

    hermes_reflect::inbox_remove(&id).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(())
}

/// Record accept/reject of an inbox candidate in the reflect log so
/// meta-reflection sees the user's real decision.
fn log_inbox_action(item: &InboxItem, action: ActionTaken) {
    let (kind, label) = match &item.payload {
        InboxPayload::Memory(c) => (
            CandidateKind::Memory,
            c.fact.lines().next().unwrap_or("").to_string(),
        ),
        InboxPayload::Skill(c) => (CandidateKind::Skill, c.name.clone()),
    };
    log_append(ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: format!("inbox:{}", item.id),
        kind,
        action,
        label,
    });
}
