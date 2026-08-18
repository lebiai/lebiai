//! First-run onboarding seed: the user's display name + work scenarios,
//! persisted as a single pinned memory (tag `onboarding-seed`).
//!
//! Single source of truth: the engine already loads pinned memories into
//! every dialogue turn, so the first conversation carries the user's name
//! and scenarios. The welcome page derives its card order from the same
//! memory via `onboarding_seed_get` — no separate UI copy.

use serde::{Deserialize, Serialize};
use tauri::State;

use hermes_memory::{Confidence, MemoryFrontmatter, MemoryStore, Scope, Source};

use crate::error::GuiError;
use crate::state::AppState;

const SEED_TAG: &str = "onboarding-seed";

/// Scenario tags (also used by the welcome page card order).
pub const SCENARIO_TAGS: [&str; 5] = ["write", "think", "research", "track", "other"];

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSeedView {
    pub display_name: String,
    pub scenarios: Vec<String>,
}

/// Find the seed memory (if any) across user-scope memories.
fn find_seed(state: &AppState) -> Option<hermes_memory::LoadedMemory> {
    let memories = state.memory_store.list_active().ok()?;
    memories
        .into_iter()
        .find(|m| m.frontmatter.tags.iter().any(|t| t == SEED_TAG))
}

fn scenario_label(tag: &str) -> &'static str {
    match tag {
        "write" => "写东西",
        "think" => "想清楚",
        "research" => "查资料",
        "track" => "盯进度",
        _ => "其他",
    }
}

/// Persist the onboarding seed as one pinned memory. Replaces any previous
/// seed (idempotent — re-running onboarding updates, not duplicates).
#[tauri::command]
pub fn onboarding_seed_set(
    state: State<'_, AppState>,
    display_name: String,
    scenarios: Vec<String>,
) -> Result<OnboardingSeedView, GuiError> {
    let name = display_name.trim().to_string();
    let mut tags: Vec<String> = vec![SEED_TAG.to_string()];
    let mut labels: Vec<&'static str> = Vec::new();
    for s in &scenarios {
        let tag = s.trim().to_string();
        if SCENARIO_TAGS.contains(&tag.as_str()) && !tags.contains(&tag) {
            labels.push(scenario_label(&tag));
            tags.push(tag);
        }
    }

    let body = if name.is_empty() {
        format!("用户的工作场景：{}。", labels.join("、"))
    } else if labels.is_empty() {
        format!("用户自称「{name}」。")
    } else {
        format!(
            "用户自称「{name}」，主要在{}这类事上需要搭子一起做。",
            labels.join("、")
        )
    };

    // Replace any previous seed so onboarding stays a single source of truth.
    if let Some(old) = find_seed(&state) {
        let _ = state
            .memory_store
            .delete(Scope::User, &old.frontmatter.id)
            .map_err(|e| GuiError::Internal(e.to_string()));
    }
    let mut fm = MemoryFrontmatter::new(Source::User, Confidence::High, tags, "general".into());
    fm.pinned = true;
    // Structured name (single source for the UI); the body stays human-readable
    // for the engine's pinned-memory context.
    fm.extra.insert(
        serde_yaml::Value::String("display_name".into()),
        serde_yaml::Value::String(name.clone()),
    );
    state
        .memory_store
        .put(Scope::User, fm.clone(), &body)
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let mut view_scenarios: Vec<String> = scenarios
        .iter()
        .filter(|s| SCENARIO_TAGS.iter().any(|t| *t == s.trim()))
        .cloned()
        .collect();
    view_scenarios.sort();
    Ok(OnboardingSeedView {
        display_name: name,
        scenarios: view_scenarios,
    })
}

/// Read the onboarding seed (name + scenario tags) for the welcome page.
#[tauri::command]
pub fn onboarding_seed_get(
    state: State<'_, AppState>,
) -> Result<Option<OnboardingSeedView>, GuiError> {
    let Some(mem) = find_seed(&state) else {
        return Ok(None);
    };
    // Prefer the structured field; fall back to legacy bodies written before
    // it existed (parse up to the first closing bracket — bodies end with 「。」).
    let name = mem
        .frontmatter
        .extra
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| display_name_from_body(&mem.body));
    let mut scenarios: Vec<String> = mem
        .frontmatter
        .tags
        .iter()
        .filter(|t| *t != SEED_TAG && SCENARIO_TAGS.contains(&t.as_str()))
        .cloned()
        .collect();
    scenarios.sort();
    Ok(Some(OnboardingSeedView {
        display_name: name,
        scenarios,
    }))
}

/// Legacy seed bodies embed the name as `用户自称「…」…`; read up to the
/// first closing bracket (bodies may end with a period or a scenario clause).
fn display_name_from_body(body: &str) -> String {
    body.lines()
        .next()
        .and_then(|l| l.strip_prefix("用户自称「"))
        .and_then(|rest| rest.split('」').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::display_name_from_body;

    #[test]
    fn parses_name_only_body() {
        assert_eq!(display_name_from_body("用户自称「张强」。"), "张强");
    }

    #[test]
    fn parses_name_with_scenario_clause() {
        assert_eq!(
            display_name_from_body("用户自称「张强」，主要在写东西这类事上需要搭子一起做。"),
            "张强"
        );
    }

    #[test]
    fn empty_for_scenarios_only_body() {
        assert_eq!(
            display_name_from_body("用户的工作场景：写东西、查资料。"),
            ""
        );
    }

    #[test]
    fn empty_for_unknown_body() {
        assert_eq!(display_name_from_body("今天天气不错。"), "");
    }
}
