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
    /// 这一条**点头之后会归到哪**（`docs/spec/projects.md` §5.1 规矩 4）。
    /// 判据与落盘同一处（`candidate::owner_for` → `memory::owner_label`），这里只搬不算；
    /// 技能候选没有归属 → `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
}

fn source_label(s: InboxSource) -> &'static str {
    match s {
        InboxSource::SessionEnd => "session_end",
        InboxSource::Micro => "micro",
        InboxSource::ManualReflect => "manual",
    }
}

fn item_to_view(item: InboxItem) -> InboxItemView {
    // 归属要在 `payload` 被 move 走之前算——与 `accept_pending_review` 同一个判据。
    let owners = match &item.payload {
        InboxPayload::Memory(c) => {
            let id = hermes_reflect::candidate::owner_for(c, item.session_owner.as_deref());
            let name = id
                .as_deref()
                .map(|o| crate::commands::memory::owner_label(Some(o)));
            (id, name)
        }
        InboxPayload::Skill(_) => (None, None),
    };
    let (owner_id, owner_name) = owners;
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
            owner_id,
            owner_name,
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
            owner_id,
            owner_name,
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

/// 点头之后回给界面的一句话：**这句话落在了谁名下**（`docs/spec/projects.md` §5.1 规矩 4）。
/// 技能候选没有归属 → `None`。
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AcceptOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
    /// 库里已经有一条同样的：**没重复落盘**，条目照样从待审里移除。
    /// 界面要说的是「这条已经记过」，不是「失败」。
    pub already_known: bool,
}

#[tauri::command]
pub async fn accept_pending_review(
    state: State<'_, AppState>,
    id: String,
) -> Result<AcceptOutcome, GuiError> {
    let item = hermes_reflect::inbox_get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?
        .ok_or_else(|| GuiError::NotFound(format!("pending item {id}")))?;
    log_inbox_action(&item, ActionTaken::Accept);

    let mut owner_id = None;
    let mut already_known = false;
    match &item.payload {
        InboxPayload::Memory(c) => {
            // 归属的判定与落盘都在 `hermes-reflect` 里，GUI / server / CLI 同一份实现。
            let written =
                hermes_reflect::inbox_accept_memory_item(state.memory_store.as_ref(), &item, c)
                    .map_err(|e| GuiError::Internal(e.to_string()))?;
            already_known = written.is_none();
            // 回执用同一个判据读一遍（不是算第二遍）：`owner_for` 与落盘共用 `resolve_owner`。
            owner_id = hermes_reflect::candidate::owner_for(c, item.session_owner.as_deref());
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
    Ok(AcceptOutcome {
        already_known,
        owner_name: owner_id
            .as_deref()
            .map(|o| crate::commands::memory::owner_label(Some(o))),
        owner_id,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, Scope};
    use hermes_reflect::MemoryCandidate;

    /// `zone = general` + 点名本人 = 「这顶帽子的手艺」那一档（`resolve_owner` 的判据，
    /// 这里不重判，只借一个已知会归到人的形状）。
    fn candidate(owner: Option<&str>) -> MemoryCandidate {
        MemoryCandidate {
            fact: "核数字要给出处".into(),
            tags: vec![],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "你连着两期都这么裁".into(),
            supersedes: vec![],
            owner: owner.map(str::to_string),
        }
    }

    fn item(payload: InboxPayload, session_owner: Option<&str>) -> InboxItem {
        InboxItem {
            id: "i1".into(),
            created_at: chrono::Utc::now(),
            source: InboxSource::SessionEnd,
            fingerprint: "fp".into(),
            payload,
            distill_id: None,
            session_id: None,
            session_owner: session_owner.map(str::to_string),
            through_at: None,
        }
    }

    /// 归属必须在**列表**里就带上：用户是在点头之前看这一屏做决定的
    /// （`docs/spec/projects.md` §5.1 规矩 4）。判据与落盘同一处，这里只钉住它确实出去了。
    #[test]
    fn the_pending_card_carries_the_owner_it_will_be_filed_under() {
        let view = item_to_view(item(
            InboxPayload::Memory(candidate(Some("wang-hai-yan"))),
            Some("wang-hai-yan"),
        ));
        assert_eq!(view.owner_id.as_deref(), Some("wang-hai-yan"));
        assert!(view.owner_name.is_some(), "归属得给个人话名字");
    }

    /// 落全局的候选（没有工位，或判据本来就判到全局）→ 两个字段都不带，
    /// 界面据此说「会归到：全局」。**不许**在这里补一个假归属。
    #[test]
    fn global_candidates_carry_no_owner_field() {
        for view in [
            item_to_view(item(InboxPayload::Memory(candidate(None)), None)),
            item_to_view(item(
                InboxPayload::Memory(candidate(None)),
                Some("wang-hai-yan"),
            )),
        ] {
            assert_eq!(view.owner_id, None);
            assert_eq!(view.owner_name, None);
        }
    }

    /// 技能候选没有归属这条轴，不许硬塞一个。
    #[test]
    fn skill_candidates_have_no_owner() {
        let view = item_to_view(item(
            InboxPayload::Skill(hermes_reflect::SkillCandidate {
                name: "s".into(),
                description: "d".into(),
                triggers: vec![],
                body: "b".into(),
                confidence: Confidence::High,
                rationale: "r".into(),
            }),
            Some("wang-hai-yan"),
        ));
        assert_eq!(view.owner_id, None);
        assert_eq!(view.owner_name, None);
    }
}
