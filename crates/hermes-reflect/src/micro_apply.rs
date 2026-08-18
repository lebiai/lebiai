//! Apply a micro-reflection [`ReflectionOutput`] to the memory store.
//!
//! Shared by CLI and GUI so auto-accept, near-duplicate rejection, conflict
//! gating, deferred queue, and accept logging stay one implementation.
//!
//! Skills are never auto-written — they always become pending (and are also
//! appended to `deferred.jsonl` for session-end re-evaluation).

use std::path::PathBuf;

use hermes_memory::{
    Confidence, MemoryFrontmatter, MemoryStore, MemoryStoreError, Source, DEFAULT_DEDUP_THRESHOLD,
};

use crate::deferred::{self, DeferredCandidate};
use crate::log::{self, ActionTaken, CandidateKind, ReflectLogEntry};
use crate::output::{ConflictCandidate, MemoryCandidate, ReflectionOutput, SkillCandidate};

/// Policy for applying micro-reflection candidates.
#[derive(Debug, Clone)]
pub struct MicroApplyConfig {
    pub session_id: String,
    /// From `config.reflect.auto_accept_memories` (P0 default: false).
    pub auto_accept_memories: bool,
    /// Minimum confidence for auto-accept (bypassed when `explicit_intent`).
    pub min_confidence: Confidence,
    /// User turn taught the agent ("记住…", "always…") — bypass confidence floor.
    pub explicit_intent: bool,
    /// Cosine threshold for near-duplicate rejection. Default = store default.
    pub dedup_threshold: f64,
    /// CLI session-end re-reads `deferred.jsonl`. GUI/server use inbox only.
    pub queue_deferred: bool,
}

impl MicroApplyConfig {
    pub fn new(
        session_id: impl Into<String>,
        auto_accept_memories: bool,
        min_confidence: Confidence,
        explicit_intent: bool,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            auto_accept_memories,
            min_confidence,
            explicit_intent,
            dedup_threshold: DEFAULT_DEDUP_THRESHOLD,
            queue_deferred: true,
        }
    }

    /// Desktop / server: pending goes to inbox. Do not grow a second file.
    pub fn inbox_only(mut self) -> Self {
        self.queue_deferred = false;
        self
    }
}

/// Outcome of applying one micro-reflection pass.
#[derive(Debug, Clone, Default)]
pub struct MicroApplyResult {
    pub summary: String,
    /// Memories successfully auto-written this pass.
    pub auto_accepted: usize,
    pub auto_accepted_paths: Vec<PathBuf>,
    /// Skill candidates still needing human approval.
    pub pending_skills: Vec<SkillCandidate>,
    /// Memory candidates still needing human approval (or deferred).
    pub pending_memories: Vec<MemoryCandidate>,
    /// Conflicts that block auto-accept for the whole batch.
    pub pending_conflicts: Vec<ConflictCandidate>,
    /// Near-duplicates skipped (not pending, not written).
    pub skipped_near_duplicates: usize,
}

impl MicroApplyResult {
    pub fn pending_memory_count(&self) -> usize {
        self.pending_memories.len()
    }

    pub fn pending_skill_count(&self) -> usize {
        self.pending_skills.len()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending_skills.is_empty()
            || !self.pending_memories.is_empty()
            || !self.pending_conflicts.is_empty()
    }

    /// Rebuild a [`ReflectionOutput`] of only pending (non-auto) candidates.
    pub fn pending_as_output(&self) -> ReflectionOutput {
        ReflectionOutput {
            summary: self.summary.clone(),
            skill_candidates: self.pending_skills.clone(),
            memory_candidates: self.pending_memories.clone(),
            conflicts: self.pending_conflicts.clone(),
        }
    }
}

/// Apply auto-accept rules and queue the rest for human review.
///
/// - Never blocks on I/O beyond sync store calls.
/// - Does **not** call the LLM (profile recompile stays with the caller).
pub fn apply_micro_output(
    output: ReflectionOutput,
    store: &dyn MemoryStore,
    config: &MicroApplyConfig,
) -> MicroApplyResult {
    let mut result = MicroApplyResult {
        summary: output.summary.clone(),
        pending_conflicts: output.conflicts.clone(),
        ..Default::default()
    };

    // Any conflict → whole memory batch needs human review (no auto-write).
    let has_conflicts = !output.conflicts.is_empty();

    for c in &output.memory_candidates {
        if !crate::inbox::memory_passes_gate(c) {
            continue;
        }
        let clears_floor = c.confidence >= config.min_confidence || config.explicit_intent;
        let eligible = config.auto_accept_memories
            && clears_floor
            && c.supersedes.is_empty()
            && !has_conflicts;

        if !eligible {
            if config.queue_deferred {
                deferred::save(DeferredCandidate::Memory(c.clone()));
            }
            result.pending_memories.push(c.clone());
            continue;
        }

        match store.check_near_duplicate(&c.fact, config.dedup_threshold) {
            Ok(()) => {}
            Err(MemoryStoreError::Conflict {
                existing_id,
                similarity,
            }) => {
                tracing::info!(
                    %existing_id,
                    similarity,
                    "micro auto-accept skipped near-duplicate"
                );
                result.skipped_near_duplicates += 1;
                continue;
            }
            Err(e) => {
                tracing::warn!(error=%e, "near-duplicate check failed; deferring candidate");
                if config.queue_deferred {
                    deferred::save(DeferredCandidate::Memory(c.clone()));
                }
                result.pending_memories.push(c.clone());
                continue;
            }
        }

        let zone = {
            let z = c.zone.trim();
            if z.is_empty() {
                "general".to_string()
            } else {
                z.to_string()
            }
        };
        let fm = MemoryFrontmatter::new(Source::Reflection, c.confidence, c.tags.clone(), zone);
        match store.put(c.scope, fm, &c.fact) {
            Ok(path) => {
                tracing::info!(path=%path.display(), "micro auto-accepted memory");
                result.auto_accepted += 1;
                result.auto_accepted_paths.push(path);
                log::append(ReflectLogEntry {
                    at: chrono::Utc::now(),
                    session_id: config.session_id.clone(),
                    kind: CandidateKind::Memory,
                    action: ActionTaken::AutoAccept,
                    label: c.fact.lines().next().unwrap_or("").to_string(),
                });
            }
            Err(e) => {
                tracing::warn!(error=%e, "micro auto-accept put failed");
                if config.queue_deferred {
                    deferred::save(DeferredCandidate::Memory(c.clone()));
                }
                result.pending_memories.push(c.clone());
            }
        }
    }

    for c in &output.skill_candidates {
        if config.queue_deferred {
            deferred::save(DeferredCandidate::Skill(c.clone()));
        }
        result.pending_skills.push(c.clone());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{FsMemoryStore, Scope};

    fn store() -> (tempfile::TempDir, FsMemoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = FsMemoryStore::new(dir.path().to_path_buf(), None);
        (dir, s)
    }

    fn mem_cand(fact: &str, conf: Confidence) -> MemoryCandidate {
        MemoryCandidate {
            fact: fact.into(),
            tags: vec![],
            zone: "general".into(),
            scope: Scope::User,
            confidence: conf,
            rationale: "test".into(),
            supersedes: vec![],
        }
    }

    #[test]
    fn auto_accept_off_defers_all_memories() {
        let (_d, s) = store();
        let cfg = MicroApplyConfig::new("sess", false, Confidence::Medium, false);
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand("user likes rust", Confidence::High)],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 0);
        assert_eq!(r.pending_memories.len(), 1);
        assert!(s.list_active().unwrap().is_empty());
    }

    #[test]
    fn auto_accept_writes_high_confidence() {
        let (_d, s) = store();
        let cfg = MicroApplyConfig::new("sess", true, Confidence::Medium, false);
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand(
                "The CI host is named build-prod-01 exclusively",
                Confidence::High,
            )],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 1);
        assert!(r.pending_memories.is_empty());
        assert_eq!(s.list_active().unwrap().len(), 1);
    }

    #[test]
    fn conflicts_block_auto_accept() {
        let (_d, s) = store();
        let cfg = MicroApplyConfig::new("sess", true, Confidence::Medium, false);
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand("new fact about widgets", Confidence::High)],
            conflicts: vec![ConflictCandidate {
                with: "mem_old".into(),
                kind: "stale".into(),
                explain: "old wrong".into(),
                options: vec![],
            }],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 0);
        assert_eq!(r.pending_memories.len(), 1);
        assert_eq!(r.pending_conflicts.len(), 1);
    }

    #[test]
    fn junk_does_not_auto_write_or_pending() {
        let (_d, s) = store();
        let cfg = MicroApplyConfig::new("sess", true, Confidence::Medium, false);
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand("hi", Confidence::High)],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 0);
        assert!(r.pending_memories.is_empty());
        assert!(s.list_active().unwrap().is_empty());
    }

    #[test]
    fn near_duplicate_is_skipped_not_pending() {
        let (_d, s) = store();
        let fm = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        s.put(
            Scope::User,
            fm,
            "The user prefers vim as their primary editor",
        )
        .unwrap();

        let cfg = MicroApplyConfig::new("sess", true, Confidence::Medium, false);
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand(
                "User prefers vim as the primary editor",
                Confidence::High,
            )],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 0);
        assert_eq!(r.skipped_near_duplicates, 1);
        assert!(r.pending_memories.is_empty());
        assert_eq!(s.list_active().unwrap().len(), 1);
    }
}
