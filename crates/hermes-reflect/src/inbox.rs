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
    /// Distill run that produced this item (not shown in the review UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_at: Option<String>,
}

/// Marks stamped on a full-session distill so a later run can replace this
/// session's previous pending items.
#[derive(Debug, Clone, Default)]
pub struct EnqueueMark {
    pub session_id: Option<String>,
    pub distill_id: Option<String>,
    pub through_at: Option<String>,
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
    if fact.is_empty() || fact.chars().count() < 12 {
        return false;
    }
    if is_internal_noise_text(fact) {
        return false;
    }
    if is_low_value_memory(fact) || hermes_memory::is_worthless_for_living(fact) {
        return false;
    }
    if is_work_episode(c) && !episode_is_self_contained(fact) {
        return false;
    }
    // Hollow template leftovers from offline fallbacks.
    if fact.contains("以当时对话中的实际操作为准")
        || fact.contains("若有文件路径以对话中写明的为准")
        || fact.contains("写入记忆时以本条为准（不依赖原会话文件）")
            && fact.contains("做法：")
            && fact.matches("做法：").count() >= 1
            && !fact.contains("outputs/")
            && !fact.contains("偏好")
    {
        // Allow only if there is a concrete path or preference signal.
        let has_path = fact.contains('/') || fact.contains(".md") || fact.contains(".docx");
        let has_pref = fact.contains("偏好") || fact.contains("习惯") || fact.contains("标准");
        if !has_path && !has_pref {
            return false;
        }
    }
    true
}

/// Greeting / one-shot task paste / pure URL — not worth durable memory.
fn is_low_value_memory(fact: &str) -> bool {
    let f = fact.trim();
    let lower = f.to_lowercase();
    // Pure URL-ish
    if f.starts_with("http://") || f.starts_with("https://") {
        return true;
    }
    // Greeting-only episodes
    let greet = ["你好", "您好", "hello", "hi", "嗨", "在吗", "开场寒暄"];
    if greet.iter().any(|g| f == *g || lower == *g) {
        return true;
    }
    if f.contains("开场寒暄") && f.contains("未进行任何实际工作") {
        return true;
    }
    // Template with only a raw user dump and no real method
    if f.contains("【工作情节】")
        && f.contains("以当时对话中的实际操作为准")
        && !f.contains("偏好")
        && !f.contains("标准")
        && !f.contains("outputs/")
    {
        // Title is just URL or very short task
        let first = f.lines().next().unwrap_or("");
        if first.contains("http") || first.chars().count() < 24 {
            return true;
        }
    }
    false
}

pub fn skill_passes_gate(c: &SkillCandidate) -> bool {
    !c.name.trim().is_empty() && !c.body.trim().is_empty() && c.body.trim().chars().count() >= 20
}

/// Enqueue from a reflection output. Returns how many **new** items were added.
pub fn enqueue_from_reflection(output: &ReflectionOutput, source: InboxSource) -> Result<usize> {
    enqueue_from_reflection_marked(output, source, EnqueueMark::default())
}

/// Same as [`enqueue_from_reflection`], then drop this session's older pending
/// items so a full re-distill replaces rather than stacks.
pub fn enqueue_from_reflection_marked(
    output: &ReflectionOutput,
    source: InboxSource,
    mark: EnqueueMark,
) -> Result<usize> {
    let mut file = load_file()?;
    let mut incoming: Vec<InboxItem> = Vec::new();

    for c in &output.memory_candidates {
        if !memory_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_memory(c);
        if incoming.iter().any(|i| i.fingerprint == fp)
            || file.items.iter().any(|i| {
                i.fingerprint == fp && i.session_id.as_deref() != mark.session_id.as_deref()
            })
        {
            continue;
        }
        incoming.push(InboxItem {
            id: format!("pend_m_{fp}"),
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Memory(c.clone()),
            distill_id: mark.distill_id.clone(),
            session_id: mark.session_id.clone(),
            through_at: mark.through_at.clone(),
        });
    }

    for c in &output.skill_candidates {
        if !skill_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_skill(c);
        if incoming.iter().any(|i| i.fingerprint == fp)
            || file.items.iter().any(|i| {
                i.fingerprint == fp && i.session_id.as_deref() != mark.session_id.as_deref()
            })
        {
            continue;
        }
        incoming.push(InboxItem {
            id: format!("pend_s_{fp}"),
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Skill(c.clone()),
            distill_id: mark.distill_id.clone(),
            session_id: mark.session_id.clone(),
            through_at: mark.through_at.clone(),
        });
    }

    // Empty re-distill must not wipe unreviewed items from this session.
    if incoming.is_empty() {
        return Ok(0);
    }

    if let Some(sid) = mark.session_id.as_deref() {
        file.items.retain(|i| i.session_id.as_deref() != Some(sid));
    }
    let added = incoming.len();
    file.items.extend(incoming);

    if file.items.len() > MAX_ITEMS {
        let drop_n = file.items.len() - MAX_ITEMS;
        file.items.drain(0..drop_n);
    }

    save_file(&file)?;
    Ok(added)
}

/// Drop items that no longer pass quality gates (garbage cleanup).
pub fn prune_low_quality() -> Result<usize> {
    let mut file = load_file()?;
    let before = file.items.len();
    file.items.retain(|item| match &item.payload {
        InboxPayload::Memory(c) => memory_passes_gate(c),
        InboxPayload::Skill(c) => skill_passes_gate(c),
    });
    let removed = before.saturating_sub(file.items.len());
    if removed > 0 {
        save_file(&file)?;
    }
    Ok(removed)
}

pub fn list() -> Result<Vec<InboxItem>> {
    let _ = prune_low_quality();
    let mut items = load_file()?.items;
    items.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(items)
}

pub fn count() -> Result<usize> {
    let _ = prune_low_quality();
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
        let env = MemoryCandidate {
            fact: "本机 Python 环境已安装 python-docx 1.2.0，可用来生成 .docx".into(),
            tags: vec!["environment".into()],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::Medium,
            rationale: "env".into(),
            supersedes: vec![],
        };
        assert!(!memory_passes_gate(&env));
    }

    #[test]
    fn fingerprint_dedups() {
        let a = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        let b = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        assert_eq!(a, b);
    }
}
