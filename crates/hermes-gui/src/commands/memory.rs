use serde::Serialize;
use tauri::State;

use hermes_memory::{Confidence, MemoryFrontmatter, MemoryStore, Scope, Source};

/// A derived, rebuildable grouping of memories by subject. The card carries the
/// summary; the memories it points at stay the source of truth. See
/// `docs/records/20260914-memory-topic-cards.md`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TopicCardItem {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub member_ids: Vec<String>,
    pub built_at: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TopicCardsView {
    pub cards: Vec<TopicCardItem>,
    /// Memories exist that no card points at — the cards are behind, and the
    /// UI says so instead of pretending to be current.
    pub stale: bool,
    pub unassigned: usize,
    pub active_count: usize,
}

fn cards_view(
    cards: &hermes_memory::TopicCards,
    active: &[hermes_memory::LoadedMemory],
) -> TopicCardsView {
    TopicCardsView {
        cards: cards
            .cards
            .iter()
            .map(|c| TopicCardItem {
                id: c.id.clone(),
                title: c.title.clone(),
                summary: c.summary.clone(),
                member_ids: c.members.clone(),
                built_at: c.built_at.clone(),
            })
            .collect(),
        stale: hermes_memory::topics::is_stale(cards, active),
        unassigned: hermes_memory::topics::unassigned(cards, active).len(),
        active_count: active.len(),
    }
}

/// Current cards, pruned against the live memories, plus whether they are behind.
#[tauri::command]
pub fn list_topic_cards(state: State<'_, AppState>) -> Result<TopicCardsView, GuiError> {
    let active = state
        .memory_store
        .list_active()
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    // 管理面今天没有工位语境：看的是「全局那份」，所以只算全局可见的记忆
    // （阶段 3 给面板接上工位后，这里换成那个工位的可见集合）。
    let view = hermes_memory::visible_owned(&active, None);
    let stored = hermes_memory::topics::load(None).unwrap_or_default();
    let cards = hermes_memory::topics::prune(&stored, &view);
    Ok(cards_view(&cards, &view))
}

/// Group the active memories into topic cards.
///
/// `rebuild = false` folds only the memories no card points at into the
/// existing cards. `rebuild = true` re-cuts every theme from the full set.
#[tauri::command]
pub async fn build_topic_cards(
    state: State<'_, AppState>,
    rebuild: bool,
) -> Result<TopicCardsView, GuiError> {
    let provider = state.provider.read().unwrap().clone();
    let active = state
        .memory_store
        .list_active()
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    if active.is_empty() {
        return Err(GuiError::Internal("还没有记忆可以整理".into()));
    }
    // 整理的是「全局那份」：只把全局可见的记忆喂给模型，卡也写回全局那份
    // （阶段 3 给面板接上工位后，这里换成那个工位的可见集合 + 它那份文件）。
    let view = hermes_memory::visible_owned(&active, None);
    let stored = hermes_memory::topics::load(None).unwrap_or_default();
    let fresh = hermes_reflect::build_topic_cards(provider.as_ref(), &view, &stored, rebuild)
        .await
        .map_err(|e| GuiError::Provider(e.to_string()))?;
    hermes_memory::topics::save(None, &fresh).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(cards_view(&fresh, &view))
}

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub body: String,
    pub scope: String,
    pub pinned: bool,
    pub confidence: String,
    pub tags: Vec<String>,
    pub zone: String,
    pub created_at: String,
    pub source: String,
}

fn to_item(m: &hermes_memory::LoadedMemory) -> MemoryItem {
    MemoryItem {
        id: m.frontmatter.id.clone(),
        body: m.body.clone(),
        scope: format!("{:?}", m.scope),
        pinned: m.frontmatter.pinned,
        confidence: format!("{:?}", m.frontmatter.confidence),
        tags: m.frontmatter.tags.clone(),
        zone: m.frontmatter.zone.clone(),
        created_at: m.frontmatter.created.to_rfc3339(),
        source: format!("{:?}", m.frontmatter.source),
    }
}

#[tauri::command]
pub fn list_memories(state: State<'_, AppState>) -> Result<Vec<MemoryItem>, GuiError> {
    let memories = state
        .memory_store
        .list_active()
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    let mut items: Vec<MemoryItem> = memories.iter().map(to_item).collect();
    sort_newest_first(&mut items);
    Ok(items)
}

/// Newest first — the user comes here to check what it just learned.
///
/// The engine sorts by id (`FsMemoryStore::list`), and ids are random short
/// codes, so the store order is meaningless to a reader. Sorting lives here, in
/// the view command: the engine's own order still feeds context assembly.
fn sort_newest_first(items: &mut [MemoryItem]) {
    use std::cmp::Ordering;
    items.sort_by(|a, b| {
        let ka = chrono::DateTime::parse_from_rfc3339(&a.created_at).ok();
        let kb = chrono::DateTime::parse_from_rfc3339(&b.created_at).ok();
        match (ka, kb) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        }
    });
}

#[tauri::command]
pub fn create_memory(
    state: State<'_, AppState>,
    body: String,
    tags: Vec<String>,
    scope: String,
    zone: Option<String>,
    pinned: bool,
) -> Result<MemoryItem, GuiError> {
    let s = match scope.as_str() {
        "Project" => Scope::Project,
        _ => Scope::User,
    };
    let zone = hermes_reflect::candidate::zone_or_general(zone.as_deref());
    let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, tags, zone);
    fm.pinned = pinned;
    state
        .memory_store
        .put(s, fm.clone(), &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let item = MemoryItem {
        id: fm.id.clone(),
        body,
        scope: format!("{:?}", s),
        pinned: fm.pinned,
        confidence: format!("{:?}", fm.confidence),
        tags: fm.tags.clone(),
        zone: fm.zone.clone(),
        created_at: fm.created.to_rfc3339(),
        source: "User".into(),
    };
    Ok(item)
}

#[tauri::command]
pub fn delete_memory(
    state: State<'_, AppState>,
    id: String,
    scope: String,
) -> Result<bool, GuiError> {
    let s = match scope.as_str() {
        "Project" => Scope::Project,
        _ => Scope::User,
    };
    state
        .memory_store
        .delete(s, &id)
        .map_err(|e| GuiError::Internal(e.to_string()))
}

#[tauri::command]
pub fn toggle_pin_memory(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<MemoryItem>, GuiError> {
    let mem = state
        .memory_store
        .get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    let Some(mem) = mem else { return Ok(None) };

    let mut fm = mem.frontmatter.clone();
    fm.pinned = !fm.pinned;
    state
        .memory_store
        .put(mem.scope, fm, &mem.body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let updated = state
        .memory_store
        .get(&id)
        .map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(updated.as_ref().map(to_item))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, created: &str) -> MemoryItem {
        MemoryItem {
            id: id.to_string(),
            body: String::new(),
            scope: "User".into(),
            pinned: false,
            confidence: "High".into(),
            tags: vec![],
            zone: "general".into(),
            created_at: created.to_string(),
            source: "User".into(),
        }
    }

    fn ids(items: &[MemoryItem]) -> Vec<&str> {
        items.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn newest_learned_lands_on_top() {
        // The store hands them over in random-id order; the panel must not.
        let mut items = vec![
            item("mem_aaa", "2026-08-03T12:34:00+00:00"),
            item("mem_zzz", "2026-09-13T15:17:36+00:00"),
            item("mem_mmm", "2026-08-17T05:39:15+00:00"),
        ];
        sort_newest_first(&mut items);
        assert_eq!(ids(&items), vec!["mem_zzz", "mem_mmm", "mem_aaa"]);
    }

    #[test]
    fn same_timestamp_and_garbage_stamps_do_not_panic() {
        let mut items = vec![
            item("mem_1", "2026-09-13T10:00:00+00:00"),
            item("mem_2", "2026-09-13T10:00:00+00:00"),
            item("mem_3", "not a timestamp"),
        ];
        sort_newest_first(&mut items);
        // Parseable stamps first (newest first), unparseable ones last, and the
        // tie keeps a stable, deterministic order.
        assert_eq!(ids(&items), vec!["mem_1", "mem_2", "mem_3"]);
    }
}
