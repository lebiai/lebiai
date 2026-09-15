//! Reflection accept/reject log + meta-reflection stats.
//!
//! Each candidate that surfaces during reflection is recorded with the
//! user's action (accept / reject / defer / merge / scope_split / keep_old
//! / keep_new / skip) and the timestamp. Aggregating across sessions tells
//! us whether the reflection prompt itself is producing useful candidates
//! — low acceptance is a signal to improve the prompt (meta-reflection).
//!
//! The file target is **always passed in** (`*_at`); the pathless wrappers
//! exist only for application entry points that already resolved the root.

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::jsonl;

const FILE: &str = "reflect-log.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectLogEntry {
    pub at: DateTime<Utc>,
    pub session_id: String,
    pub kind: CandidateKind,
    pub action: ActionTaken,
    /// Free-text identifier for the candidate (skill name / memory id / etc.)
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Skill,
    Memory,
    /// A memory candidate that came with a conflict.
    ConflictMemory,
    OrphanConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionTaken {
    /// User accepted the candidate as-is (`a` / `n` keep_new).
    Accept,
    /// Micro-reflection auto-accepted (eligible: Medium confidence, no
    /// conflicts, no supersedes).
    AutoAccept,
    /// User rejected (`r`) or kept the old one (`o`) or skipped (`k`).
    Reject,
    /// `d` defer.
    Defer,
    /// Conflict-only:`m` merge.
    Merge,
    /// Conflict-only:`s` scope split.
    ScopeSplit,
    /// User pressed Ctrl-C / EOF on the candidate.
    Cancelled,
}

/// `reflect-log.jsonl` inside an explicit data root.
pub fn path_in(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// Append one entry into `root`. Best-effort — failures are logged but not
/// propagated because a log write must never block the reflection UX.
pub fn append_at(root: &Path, entry: ReflectLogEntry) {
    if let Err(e) = jsonl::append_line(&path_in(root), &entry) {
        tracing::warn!(error=%e, "failed to write reflect-log entry");
    }
}

/// Append into the process data root — application entry points only.
pub fn append(entry: ReflectLogEntry) {
    append_at(&hermes_core::data_root(), entry);
}

/// Read all entries in `root` (newest last). Missing file is not an error.
pub fn read_all_at(root: &Path) -> Result<Vec<ReflectLogEntry>> {
    jsonl::read_lines(&path_in(root), "reflect-log")
}

/// Read all entries from the process data root — application entry points only.
pub fn read_all() -> Result<Vec<ReflectLogEntry>> {
    read_all_at(&hermes_core::data_root())
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub total: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub deferred: usize,
    pub other: usize,
}

impl Stats {
    pub fn acceptance_rate(&self) -> Option<f64> {
        if self.total == 0 {
            return None;
        }
        Some(self.accepted as f64 / self.total as f64)
    }
}

/// Aggregate stats across the most recent `last_n` entries in `root` (or all
/// if `None`).
pub fn stats_at(root: &Path, last_n: Option<usize>) -> Result<Stats> {
    let mut all = read_all_at(root)?;
    if let Some(n) = last_n {
        if all.len() > n {
            all.drain(..all.len() - n);
        }
    }
    let mut s = Stats::default();
    for e in &all {
        s.total += 1;
        match e.action {
            ActionTaken::Accept | ActionTaken::AutoAccept => s.accepted += 1,
            ActionTaken::Reject => s.rejected += 1,
            ActionTaken::Defer => s.deferred += 1,
            _ => s.other += 1,
        }
    }
    Ok(s)
}

/// Aggregate stats from the process data root — application entry points only.
pub fn stats(last_n: Option<usize>) -> Result<Stats> {
    stats_at(&hermes_core::data_root(), last_n)
}

/// Return the `n` most recent entries in `root` (newest last).
pub fn recent_outcomes_at(root: &Path, n: usize) -> Result<Vec<ReflectLogEntry>> {
    let mut all = read_all_at(root)?;
    let start = all.len().saturating_sub(n);
    Ok(all.drain(start..).collect())
}

/// The `n` most recent entries from the process data root — entry points only.
pub fn recent_outcomes(n: usize) -> Result<Vec<ReflectLogEntry>> {
    recent_outcomes_at(&hermes_core::data_root(), n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> ReflectLogEntry {
        ReflectLogEntry {
            at: Utc::now(),
            session_id: "sess".into(),
            kind: CandidateKind::Memory,
            action: ActionTaken::AutoAccept,
            label: label.into(),
        }
    }

    #[test]
    fn acceptance_rate_calc() {
        let s = Stats {
            total: 10,
            accepted: 3,
            rejected: 5,
            deferred: 2,
            other: 0,
        };
        assert!((s.acceptance_rate().unwrap() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn append_read_and_stats_in_explicit_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        append_at(root, entry("first"));
        append_at(root, entry("second"));
        assert!(read_all_at(root).unwrap().len() == 2);
        assert_eq!(stats_at(root, None).unwrap().accepted, 2);
        assert_eq!(stats_at(root, Some(1)).unwrap().total, 1);
        assert_eq!(recent_outcomes_at(root, 1).unwrap()[0].label, "second");

        // An unrelated root stays untouched: the log target is not global.
        let other = tempfile::tempdir().unwrap();
        assert!(read_all_at(other.path()).unwrap().is_empty());
    }

    #[test]
    fn concurrent_appends_never_tear_a_line() {
        const THREADS: usize = 16;
        const PER_THREAD: usize = 8;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let start = std::sync::Arc::new(std::sync::Barrier::new(THREADS));

        std::thread::scope(|scope| {
            for t in 0..THREADS {
                let root = root.clone();
                let start = start.clone();
                scope.spawn(move || {
                    start.wait();
                    for i in 0..PER_THREAD {
                        append_at(&root, entry(&format!("concurrent entry {t}-{i}")));
                    }
                });
            }
        });

        let raw = std::fs::read_to_string(path_in(&root)).unwrap();
        assert_eq!(
            raw.matches('\n').count(),
            THREADS * PER_THREAD,
            "each append must terminate exactly one line"
        );
        assert_eq!(
            read_all_at(&root).unwrap().len(),
            THREADS * PER_THREAD,
            "every append must survive as its own parseable entry"
        );
    }
}
