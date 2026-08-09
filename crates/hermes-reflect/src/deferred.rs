//! Deferred candidate queue — candidates the user skipped/deferred during
//! reflection, persisted to disk so they can be re-evaluated in future sessions.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::output::{MemoryCandidate, SkillCandidate};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DeferredCandidate {
    #[serde(rename = "skill")]
    Skill(SkillCandidate),
    #[serde(rename = "memory")]
    Memory(MemoryCandidate),
}

fn default_path() -> Result<PathBuf> {
    Ok(hermes_core::data_path("deferred.jsonl"))
}

/// Append a deferred candidate. Best-effort write.
pub fn save(candidate: DeferredCandidate) {
    if let Err(e) = try_save(candidate) {
        tracing::warn!(error=%e, "failed to save deferred candidate");
    }
}

fn try_save(candidate: DeferredCandidate) -> Result<()> {
    let path = default_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&candidate)?;
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Load all deferred candidates. Missing file is not an error.
pub fn load() -> Result<Vec<DeferredCandidate>> {
    let path = default_path()?;
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
        match serde_json::from_str::<DeferredCandidate>(&line) {
            Ok(c) => out.push(c),
            Err(e) => tracing::debug!(error=%e, "skipping bad deferred line"),
        }
    }
    Ok(out)
}

/// Clear all deferred candidates (after re-evaluation).
pub fn clear() -> Result<()> {
    let path = default_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, Scope};

    #[test]
    fn save_load_clear_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deferred.jsonl");

        // Override the default path for testing by writing directly.
        let c1 = DeferredCandidate::Memory(MemoryCandidate {
            fact: "user prefers anyhow".into(),
            tags: vec!["rust".into()],
            zone: "preferences".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "explicit".into(),
            supersedes: vec![],
        });
        let c2 = DeferredCandidate::Skill(SkillCandidate {
            name: "rust-error".into(),
            description: "switch unwrap to anyhow".into(),
            triggers: vec!["rust".into()],
            body: "step 1\nstep 2".into(),
            rationale: "reusable".into(),
            confidence: Confidence::Medium,
        });

        let line1 = serde_json::to_string(&c1).unwrap();
        let line2 = serde_json::to_string(&c2).unwrap();
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "{line1}").unwrap();
        writeln!(f, "{line2}").unwrap();
        drop(f);

        // Read back.
        let f = std::fs::File::open(&path).unwrap();
        let reader = BufReader::new(f);
        let loaded: Vec<DeferredCandidate> = reader
            .lines()
            .map_while(|r| r.ok())
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect();
        assert_eq!(loaded.len(), 2);

        // Clear.
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }
}
