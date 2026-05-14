//! Reflection accept/reject log + meta-reflection stats.
//!
//! Each candidate that surfaces during reflection is recorded with the
//! user's action (accept / reject / defer / merge / scope_split / keep_old
//! / keep_new / skip) and the timestamp. Aggregating across sessions tells
//! us whether the reflection prompt itself is producing useful candidates
//! — low acceptance is a signal to improve the prompt (meta-reflection).

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

pub fn default_log_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    Ok(home.join(".small-rust-hermes").join("reflect-log.jsonl"))
}

/// Append one entry. Best-effort — failures are logged but not propagated
/// because a log write must never block the reflection UX.
pub fn append(entry: ReflectLogEntry) {
    if let Err(e) = try_append(entry) {
        tracing::warn!(error=%e, "failed to write reflect-log entry");
    }
}

fn try_append(entry: ReflectLogEntry) -> Result<()> {
    let path = default_log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&entry)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Read all entries (newest last). Missing file is not an error.
pub fn read_all() -> Result<Vec<ReflectLogEntry>> {
    let path = default_log_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let f = std::fs::File::open(&path)?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines().map_while(|r| r.ok()) {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ReflectLogEntry>(&line) {
            Ok(e) => out.push(e),
            Err(e) => tracing::debug!(error=%e, "skipping bad reflect-log line"),
        }
    }
    Ok(out)
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

/// Aggregate stats across the most recent `last_n` entries (or all if
/// `None`).
pub fn stats(last_n: Option<usize>) -> Result<Stats> {
    let mut all = read_all()?;
    if let Some(n) = last_n {
        if all.len() > n {
            all.drain(..all.len() - n);
        }
    }
    let mut s = Stats::default();
    for e in &all {
        s.total += 1;
        match e.action {
            ActionTaken::Accept => s.accepted += 1,
            ActionTaken::Reject => s.rejected += 1,
            ActionTaken::Defer => s.deferred += 1,
            _ => s.other += 1,
        }
    }
    Ok(s)
}

/// Return the `n` most recent log entries (newest last).
pub fn recent_outcomes(n: usize) -> Result<Vec<ReflectLogEntry>> {
    let mut all = read_all()?;
    let start = all.len().saturating_sub(n);
    Ok(all.drain(start..).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
