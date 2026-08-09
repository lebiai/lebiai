//! Pending-review inbox: quiet evolution queue.
//!
//! Reflection extracts candidates in the background; they land here for
//! batch review. Default UX: no modal on leave — badge + panel only.
//!
//! File: `~/.lebi-ai/pending-review.json`

use std::fs;
use std::path::PathBuf;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::episode::{episode_is_self_contained, is_internal_noise_text, is_work_episode};
use crate::output::{MemoryCandidate, ReflectionOutput, SkillCandidate};

const MAX_ITEMS: usize = 100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InboxSource {
    SessionEnd,
    Micro,
    ManualReflect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxPayload {
    Memory(MemoryCandidate),
    Skill(SkillCandidate),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub source: InboxSource,
    pub fingerprint: String,
    pub payload: InboxPayload,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct InboxFile {
    items: Vec<InboxItem>,
}

fn path() -> PathBuf {
    hermes_core::data_path("pending-review.json")
}

fn load_file() -> Result<InboxFile> {
    let p = path();
    if !p.exists() {
        return Ok(InboxFile::default());
    }
    let raw = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
    if raw.trim().is_empty() {
        return Ok(InboxFile::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", p.display()))
}

fn save_file(file: &InboxFile) -> Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(file)?;
    fs::write(&p, raw).with_context(|| format!("write {}", p.display()))
}

fn hash_str(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn fingerprint_memory(c: &MemoryCandidate) -> String {
    hash_str(&format!("{}|{}", c.fact.trim(), c.zone.trim()))
}

fn fingerprint_skill(c: &SkillCandidate) -> String {
    let body: String = c.body.trim().chars().take(200).collect();
    hash_str(&format!("{}|{}", c.name.trim(), body))
}

/// Quality gate: only durable, non-noise, self-contained when episode.
pub fn memory_passes_gate(c: &MemoryCandidate) -> bool {
    let fact = c.fact.trim();
    if fact.is_empty() || fact.chars().count() < 8 {
        return false;
    }
    if is_internal_noise_text(fact) {
        return false;
    }
    if is_work_episode(c) && !episode_is_self_contained(fact) {
        return false;
    }
    true
}

pub fn skill_passes_gate(c: &SkillCandidate) -> bool {
    !c.name.trim().is_empty() && !c.body.trim().is_empty() && c.body.trim().chars().count() >= 20
}

/// Enqueue from a reflection output. Returns how many **new** items were added.
pub fn enqueue_from_reflection(output: &ReflectionOutput, source: InboxSource) -> Result<usize> {
    let mut file = load_file()?;
    let existing: std::collections::HashSet<String> =
        file.items.iter().map(|i| i.fingerprint.clone()).collect();
    let mut added = 0usize;

    for c in &output.memory_candidates {
        if !memory_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_memory(c);
        if existing.contains(&fp) || file.items.iter().any(|i| i.fingerprint == fp) {
            continue;
        }
        let id = format!("pend_m_{fp}");
        file.items.push(InboxItem {
            id,
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Memory(c.clone()),
        });
        added += 1;
    }

    for c in &output.skill_candidates {
        if !skill_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_skill(c);
        if file.items.iter().any(|i| i.fingerprint == fp) {
            continue;
        }
        let id = format!("pend_s_{fp}");
        file.items.push(InboxItem {
            id,
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Skill(c.clone()),
        });
        added += 1;
    }

    // Cap size: drop oldest first.
    if file.items.len() > MAX_ITEMS {
        let drop_n = file.items.len() - MAX_ITEMS;
        file.items.drain(0..drop_n);
    }

    if added > 0 {
        save_file(&file)?;
    }
    Ok(added)
}

pub fn list() -> Result<Vec<InboxItem>> {
    let mut items = load_file()?.items;
    items.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(items)
}

pub fn count() -> Result<usize> {
    Ok(load_file()?.items.len())
}

pub fn get(id: &str) -> Result<Option<InboxItem>> {
    Ok(load_file()?.items.into_iter().find(|i| i.id == id))
}

pub fn remove(id: &str) -> Result<bool> {
    let mut file = load_file()?;
    let before = file.items.len();
    file.items.retain(|i| i.id != id);
    if file.items.len() == before {
        return Ok(false);
    }
    save_file(&file)?;
    Ok(true)
}

pub fn clear() -> Result<()> {
    let p = path();
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, Scope};

    fn good_mem(fact: &str) -> MemoryCandidate {
        MemoryCandidate {
            fact: fact.into(),
            tags: vec!["work-episode".into()],
            zone: "work".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "t".into(),
            supersedes: vec![],
        }
    }

    #[test]
    fn gate_rejects_care_nudge_and_hollow() {
        let bad = good_mem(
            "【工作情节】[lebi-AI Care] Tool work\n- 情境：本会话\n- 做法：见会话记录\n- 产出：见会话记录\n- 用户反馈/修正：无\n- 可复用点：x",
        );
        assert!(!memory_passes_gate(&bad));
        let good = good_mem(
            "【工作情节】季度复盘\n- 情境：用户要三段结构做复盘\n- 做法：先结论后证据再动作\n- 产出：outputs/retro.md\n- 用户反馈/修正：无\n- 可复用点：先结论后证据",
        );
        assert!(memory_passes_gate(&good));
    }

    #[test]
    fn fingerprint_dedups() {
        let a = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        let b = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        assert_eq!(a, b);
    }
}
