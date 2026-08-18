//! Pending-review inbox Tauri commands (quiet evolution queue).

use hermes_memory::{MemoryFrontmatter, MemoryStore, Scope, Source};
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

fn put_memory_with_fallback(
    store: &dyn MemoryStore,
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

    match item.payload {
        InboxPayload::Memory(c) => {
            let mut fm = MemoryFrontmatter::new(Source::Reflection, c.confidence, c.tags, c.zone);
            fm.supersedes = c.supersedes;
            if let Some(id) = item.distill_id.clone() {
                fm.extra.insert(
                    serde_yaml::Value::String("distill_id".into()),
                    serde_yaml::Value::String(id),
                );
            }
            if let Some(sid) = item.session_id.clone() {
                fm.extra.insert(
                    serde_yaml::Value::String("source_session".into()),
                    serde_yaml::Value::String(sid),
                );
            }
            if let Some(t) = item.through_at.clone() {
                fm.extra.insert(
                    serde_yaml::Value::String("through_at".into()),
                    serde_yaml::Value::String(t),
                );
            }
            put_memory_with_fallback(state.memory_store.as_ref(), c.scope, fm, &c.fact)?;
        }
        InboxPayload::Skill(c) => {
            let fm = SkillFrontmatter {
                name: c.name,
                description: c.description,
                triggers: c.triggers,
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
