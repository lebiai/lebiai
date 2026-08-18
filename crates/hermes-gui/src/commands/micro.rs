//! GUI micro-reflection surface.
//!
//! **Architecture (correct path):**
//! - Turn streaming uses Tauri `Channel` and ends with `Done`.
//! - Micro-reflection is **not** part of that stream. It runs in a background
//!   task and notifies the UI via a **global Tauri event**
//!   (`hermes://micro-reflection`), filtered by `sessionId` on the frontend.
//! - Gate / LLM / apply / recompile live in `hermes_reflect::run_micro_after_turn`
//!   — this module only wires AppState + emit.

use hermes_core::Message;
use hermes_memory::{Confidence, MemoryStore};
use hermes_reflect::{
    enqueue_from_reflection, run_micro_after_turn, update_cooldown_after, InboxSource,
    MicroApplyConfig, MicroApplyResult, MicroRunOutcome, MicroRunRequest, ReflectionOutput,
};
use hermes_skills::LoadedSkill;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::state::{AppState, LiveLlmGate};
use std::collections::HashMap;
use std::sync::Arc;

/// Frontend event name. Keep stable — UI listens by this string.
pub const MICRO_EVENT: &str = "hermes://micro-reflection";

/// Payload pushed to the webview after a micro pass that produced something
/// user-visible (auto-accept and/or pending candidates).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroReflectionEvent {
    /// Session that produced the turn (frontend ignores if not active).
    pub session_id: String,
    pub summary: String,
    pub memory_count: usize,
    pub skill_count: usize,
    pub auto_accepted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflection: Option<PendingReflection>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReflection {
    pub summary: String,
    pub skill_candidates: Vec<PendingSkill>,
    pub memory_candidates: Vec<PendingMemory>,
    pub conflicts: Vec<PendingConflict>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSkill {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
    pub rationale: String,
    pub confidence: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMemory {
    pub fact: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub confidence: String,
    pub rationale: String,
    pub supersedes: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingConflict {
    pub with: String,
    pub kind: String,
    pub explain: String,
    pub options: Vec<String>,
}

impl PendingReflection {
    fn from_output(o: &ReflectionOutput) -> Self {
        Self {
            summary: o.summary.clone(),
            skill_candidates: o
                .skill_candidates
                .iter()
                .map(|c| PendingSkill {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    triggers: c.triggers.clone(),
                    body: c.body.clone(),
                    rationale: c.rationale.clone(),
                    confidence: format!("{:?}", c.confidence),
                })
                .collect(),
            memory_candidates: o
                .memory_candidates
                .iter()
                .map(|c| PendingMemory {
                    fact: c.fact.clone(),
                    tags: c.tags.clone(),
                    scope: format!("{:?}", c.scope),
                    confidence: format!("{:?}", c.confidence),
                    rationale: c.rationale.clone(),
                    supersedes: c.supersedes.clone(),
                })
                .collect(),
            conflicts: o
                .conflicts
                .iter()
                .map(|c| PendingConflict {
                    with: c.with.clone(),
                    kind: c.kind.clone(),
                    explain: c.explain.clone(),
                    options: c.options.clone(),
                })
                .collect(),
        }
    }
}

fn event_from_applied(session_id: String, applied: &MicroApplyResult) -> MicroReflectionEvent {
    let pending = applied.pending_as_output();
    let reflection = if applied.has_pending() {
        Some(PendingReflection::from_output(&pending))
    } else {
        None
    };
    let summary = if applied.summary.is_empty() {
        "Micro-reflection complete".into()
    } else {
        applied.summary.clone()
    };
    MicroReflectionEvent {
        session_id,
        summary,
        memory_count: applied.pending_memory_count(),
        skill_count: applied.pending_skill_count(),
        auto_accepted: applied.auto_accepted,
        reflection,
    }
}

/// Schedule micro-reflection for a finished turn. Returns immediately.
///
/// Call **after** turn messages are persisted and the stream has emitted
/// `Done`. Never blocks the next user input.
#[allow(clippy::too_many_arguments)]
pub fn spawn_after_turn(
    app: AppHandle,
    provider: Arc<dyn hermes_core::LlmProvider>,
    memory_store: std::sync::Arc<hermes_memory::FsMemoryStore>,
    active_memories: Arc<Mutex<Vec<hermes_memory::LoadedMemory>>>,
    pinned_memories: Arc<Mutex<Vec<hermes_memory::LoadedMemory>>>,
    cooldown: Arc<Mutex<HashMap<String, usize>>>,
    skills: Vec<LoadedSkill>,
    memories_snapshot: Vec<hermes_memory::LoadedMemory>,
    turn_messages: Vec<Message>,
    session_key: String,
    session_id_for_log: String,
    auto_accept: bool,
    min_confidence: Confidence,
    live_llm: Arc<LiveLlmGate>,
) {
    if turn_messages.is_empty() {
        return;
    }

    tokio::spawn(async move {
        live_llm.wait_idle().await;
        let turns_since = {
            let map = cooldown.lock().await;
            *map.get(&session_key).unwrap_or(&0)
        };

        let apply = MicroApplyConfig::new(
            session_id_for_log.clone(),
            auto_accept,
            min_confidence,
            false, // filled inside run_micro_after_turn from messages
        )
        .inbox_only();

        let outcome = run_micro_after_turn(MicroRunRequest {
            provider: provider.as_ref(),
            store: memory_store.as_ref(),
            turn_messages: &turn_messages,
            skills: &skills,
            memories: &memories_snapshot,
            turns_since_last: turns_since,
            apply,
            recompile_on_auto_accept: true,
        })
        .await;

        // Always update cooldown, even on LLM error (count as "ran" only on Ok).
        {
            let mut map = cooldown.lock().await;
            let entry = map.entry(session_key.clone()).or_insert(0);
            match &outcome {
                Ok(o) => update_cooldown_after(o, entry),
                Err(_) => {
                    // Failed attempt: still reset so we don't spin every turn.
                    *entry = 0;
                }
            }
        }

        let applied = match outcome {
            Ok(MicroRunOutcome::Applied(a)) => a,
            Ok(MicroRunOutcome::Empty | MicroRunOutcome::Skipped) => return,
            Err(e) => {
                tracing::debug!(error=%e, "GUI micro-reflection failed");
                return;
            }
        };

        // Refresh in-memory context used by the next turn's system prompt.
        if applied.auto_accepted > 0 {
            if let Ok(fresh) = memory_store.list_active() {
                let pinned: Vec<_> = fresh
                    .iter()
                    .filter(|m| m.frontmatter.pinned)
                    .cloned()
                    .collect();
                *active_memories.lock().await = fresh;
                *pinned_memories.lock().await = pinned;
            }
        }

        // Persist pending candidates into the pending-review inbox (Micro
        // source) so nothing is lost across restarts — the event below is
        // only an in-session notification.
        if applied.has_pending() {
            let pending = applied.pending_as_output();
            match enqueue_from_reflection(&pending, InboxSource::Micro) {
                Ok(added) => {
                    if added > 0 {
                        tracing::info!(added, "micro pending enqueued to inbox");
                    }
                }
                Err(e) => tracing::warn!(error=%e, "enqueue micro pending to inbox failed"),
            }
        }

        // Only emit when there is something to show (auto write or pending).
        if applied.auto_accepted == 0 && !applied.has_pending() {
            return;
        }

        let payload = event_from_applied(session_key, &applied);
        if let Err(e) = app.emit(MICRO_EVENT, payload) {
            tracing::warn!(error=%e, "emit micro-reflection event failed");
        }
    });
}

/// Read reflect config from live AppState.
pub fn reflect_policy(state: &AppState) -> (bool, Confidence) {
    let auto = state.config.read().unwrap().reflect.auto_accept_memories;
    let min: Confidence = state
        .config
        .read()
        .unwrap()
        .reflect
        .auto_accept_min_confidence
        .parse()
        .unwrap_or(Confidence::Medium);
    (auto, min)
}
