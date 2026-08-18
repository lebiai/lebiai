//! Pending-review inbox REST (Flutter Evolve surface).
//! Mirrors `hermes-gui/src/commands/inbox.rs` over HTTP.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use hermes_memory::{MemoryFrontmatter, MemoryStore, Scope, Source};
use hermes_reflect::{
    log_append, ActionTaken, CandidateKind, InboxItem, InboxPayload, InboxSource, ReflectLogEntry,
};
use hermes_skills::{SkillFrontmatter, SkillStore};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
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

pub async fn list_pending() -> Result<Json<Vec<InboxItemView>>, ApiError> {
    let items = hermes_reflect::inbox_list().map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(items.into_iter().map(item_to_view).collect()))
}

pub async fn count_pending() -> Result<Json<serde_json::Value>, ApiError> {
    let n = hermes_reflect::inbox_count().map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "count": n })))
}

#[derive(Deserialize)]
pub struct IdQuery {
    pub id: String,
}

pub async fn reject_pending(Query(q): Query<IdQuery>) -> Result<Json<()>, ApiError> {
    let item = hermes_reflect::inbox_get(&q.id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("pending item {}", q.id)))?;
    log_inbox_action(&item, ActionTaken::Reject);
    hermes_reflect::inbox_remove(&q.id).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptBody {
    pub id: String,
}

pub async fn accept_pending(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AcceptBody>,
) -> Result<Json<()>, ApiError> {
    let item = hermes_reflect::inbox_get(&body.id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("pending item {}", body.id)))?;
    log_inbox_action(&item, ActionTaken::Accept);

    match item.payload {
        InboxPayload::Memory(c) => {
            let mut fm = MemoryFrontmatter::new(Source::Reflection, c.confidence, c.tags, c.zone);
            fm.supersedes = c.supersedes;
            state
                .memory_store
                .put(c.scope, fm.clone(), &c.fact)
                .or_else(|e| {
                    if matches!(c.scope, Scope::Project) {
                        state.memory_store.put(Scope::User, fm, &c.fact)
                    } else {
                        Err(e)
                    }
                })
                .map_err(|e| ApiError::Internal(e.to_string()))?;
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
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }
    }

    hermes_reflect::inbox_remove(&body.id).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(()))
}

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
