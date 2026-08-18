//! C-SESS: work-episode normalization for continuity ("越用越像你的手感").
//!
//! **Product rule:** a work episode must stay useful **even if the session
//! JSONL is deleted**. Never store "见会话记录" / internal nudge text.
//! Prefer **no** episode over a hollow one.

use hermes_core::companion::{tags, zones};
use hermes_memory::{Confidence, Scope};

use crate::output::{MemoryCandidate, ReflectionOutput};

const EPISODE_MARKER: &str = "【工作情节】";
const MIN_SUMMARY_CHARS: usize = 16;
const MIN_EPISODE_BODY_CHARS: usize = 36;

/// Care delivery nudge markers. `[lebi-AI Care]` is the current brand;
/// `[Hermes Care]` stays recognized so pre-branding transcripts and stored
/// memories are still filtered from episode content.
const CARE_MARKERS: &[&str] = &["[lebi-AI Care]", "[Hermes Care]"];

/// True when this candidate is tagged/shaped as a work episode (not quality).
pub fn is_work_episode(c: &MemoryCandidate) -> bool {
    let zone = c.zone.trim().to_lowercase();
    if zone == zones::WORK || zone == "episode" {
        return true;
    }
    if c.tags.iter().any(|t| {
        let t = t.to_lowercase();
        t == tags::WORK_EPISODE || t == "episode" || t.contains("work-episode")
    }) {
        return true;
    }
    c.fact.contains(EPISODE_MARKER)
}

/// Internal / synthetic lines that must never become memory content.
pub fn is_internal_noise_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    const PREFIXES: &[&str] = &[
        "[lebi-AI Care]",
        "[lebi-AI ",
        "[Hermes Care]",
        "[Hermes ",
        "[Context:",
        "Tool call denied",
        "Tool call truncated",
        "You've reached the tool-call budget",
        "Generation stopped",
    ];
    PREFIXES.iter().any(|p| t.starts_with(p) || t.contains(p))
}

/// Hollow episodes are useless after the session is deleted — drop them.
pub fn episode_is_self_contained(fact: &str) -> bool {
    if is_internal_noise_text(fact) {
        return false;
    }
    let f = fact.trim();
    if f.contains("见会话记录")
        || f.contains("见该会话")
        || f.contains("见上一段对话")
        || f.contains("打开全文")
    {
        return false;
    }
    // Template leftovers with no real content.
    if f.contains("情境：本会话") && f.contains("做法：见") {
        return false;
    }
    if CARE_MARKERS.iter().any(|m| f.contains(m)) || f.contains("[Hermes ") {
        return false;
    }
    let without_marker = f.replace(EPISODE_MARKER, "");
    without_marker.chars().count() >= MIN_EPISODE_BODY_CHARS
}

/// Fix zone/tags so accepted memories land in the right palace zone.
pub fn normalize_candidate(c: &mut MemoryCandidate) {
    let fact = c.fact.as_str();
    let tags_lower: Vec<String> = c.tags.iter().map(|t| t.to_lowercase()).collect();

    if fact.contains(EPISODE_MARKER)
        || tags_lower
            .iter()
            .any(|t| t == tags::WORK_EPISODE || t == "episode")
    {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::WORK.to_string();
        }
        ensure_tag(&mut c.tags, tags::WORK_EPISODE);
        return;
    }

    if tags_lower
        .iter()
        .any(|t| t == tags::STANDARD || t == "standard")
    {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::STANDARDS.to_string();
        }
        ensure_tag(&mut c.tags, tags::STANDARD);
        return;
    }

    if tags_lower
        .iter()
        .any(|t| t == tags::PREFERENCE || t == "preference" || t == "prefers")
    {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::PREFERENCES.to_string();
        }
        ensure_tag(&mut c.tags, tags::PREFERENCE);
    }

    if c.zone.trim().is_empty() {
        c.zone = zones::GENERAL.to_string();
    }
}

fn ensure_tag(tags_list: &mut Vec<String>, tag: &str) {
    if !tags_list.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
        tags_list.push(tag.to_string());
    }
}

fn has_quality_work_episode(out: &ReflectionOutput) -> bool {
    out.memory_candidates
        .iter()
        .any(|c| is_work_episode(c) && episode_is_self_contained(&c.fact))
}

fn summary_worth_episode(summary: &str) -> bool {
    let s = summary.trim();
    if s.chars().count() < MIN_SUMMARY_CHARS {
        return false;
    }
    if is_internal_noise_text(s) {
        return false;
    }
    let lower = s.to_lowercase();
    const SKIP: &[&str] = &[
        "hello",
        "hi ",
        "greeting",
        "no work",
        "nothing",
        "闲聊",
        "打招呼",
        "无实质",
        "empty",
        "chatted",
        "small talk",
    ];
    !SKIP.iter().any(|k| lower.contains(k))
}

/// Build a **self-contained** episode body from a real summary (no session pointers).
/// Disabled for low-signal summaries — seeding hollow episodes flooded the inbox.
pub fn seed_episode_from_summary(summary: &str) -> Option<MemoryCandidate> {
    // Product decision 2026-08-11: do not auto-seed episodes from summary text.
    // Prefer LLM-produced candidates that pass quality gates, or explicit preferences.
    let _ = summary;
    None
}

#[allow(dead_code)]
fn seed_episode_from_summary_legacy(summary: &str) -> Option<MemoryCandidate> {
    if !summary_worth_episode(summary) {
        return None;
    }
    let summary = summary.trim();
    let one_line: String = summary
        .lines()
        .next()
        .unwrap_or(summary)
        .chars()
        .take(100)
        .collect();
    // Entire summary is stored so deleting the session does not gut the memory.
    let fact = format!(
        "{EPISODE_MARKER}{one_line}\n\
         - 情境：{summary}\n\
         - 做法：{summary}\n\
         - 产出：写入记忆时以本条为准（不依赖原会话文件）\n\
         - 用户反馈/修正：无\n\
         - 可复用点：{one_line}"
    );
    if !episode_is_self_contained(&fact) {
        return None;
    }
    Some(MemoryCandidate {
        fact,
        tags: vec![tags::WORK_EPISODE.to_string()],
        zone: zones::WORK.to_string(),
        scope: Scope::User,
        confidence: Confidence::Medium,
        rationale: "C-SESS: self-contained work episode from reflection summary — usable after session delete".into(),
        supersedes: vec![],
    })
}

/// Run after every full/quick reflection parse.
pub fn finalize_reflection_output(out: ReflectionOutput) -> ReflectionOutput {
    finalize_reflection_output_with(out, &[])
}

/// Same as [`finalize_reflection_output`], then attach `supersedes` for same-slot actives.
pub fn finalize_reflection_output_with(
    mut out: ReflectionOutput,
    active: &[hermes_memory::LoadedMemory],
) -> ReflectionOutput {
    for c in &mut out.memory_candidates {
        normalize_candidate(c);
    }

    out.memory_candidates.retain(|c| {
        if hermes_memory::is_worthless_for_living(&c.fact) {
            return false;
        }
        !is_work_episode(c) || episode_is_self_contained(&c.fact)
    });

    for c in &mut out.memory_candidates {
        if !c.supersedes.is_empty() {
            continue;
        }
        let ids = hermes_memory::same_slot_ids(active, &c.zone, &c.tags, &c.fact);
        c.supersedes = ids;
    }

    if !has_quality_work_episode(&out) {
        if let Some(seed) = seed_episode_from_summary(&out.summary) {
            out.memory_candidates.insert(0, seed);
        }
    }

    out.memory_candidates.sort_by_key(|c| {
        if is_work_episode(c) {
            0u8
        } else if c.zone == zones::STANDARDS
            || c.tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(tags::STANDARD))
        {
            1
        } else {
            2
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_episode_marker_sets_zone_and_tag() {
        let mut c = MemoryCandidate {
            fact: "【工作情节】项目复盘\n- 情境：季度复盘结构用三段".into(),
            tags: vec![],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "t".into(),
            supersedes: vec![],
        };
        normalize_candidate(&mut c);
        assert_eq!(c.zone, zones::WORK);
        assert!(c.tags.iter().any(|t| t == tags::WORK_EPISODE));
        assert!(is_work_episode(&c));
    }

    #[test]
    fn finalize_does_not_auto_seed_from_summary() {
        // Quality: do not invent hollow episodes from summary prose alone.
        let out = ReflectionOutput {
            summary: "Drafted a project retro with three sections and tightened the opening".into(),
            skill_candidates: vec![],
            memory_candidates: vec![],
            conflicts: vec![],
        };
        let out = finalize_reflection_output(out);
        assert!(out.memory_candidates.is_empty());
    }

    #[test]
    fn finalize_does_not_seed_short_summary() {
        let out = ReflectionOutput {
            summary: "hi".into(),
            ..Default::default()
        };
        let out = finalize_reflection_output(out);
        assert!(out.memory_candidates.is_empty());
    }

    #[test]
    fn finalize_drops_hollow_and_care_nudge_episodes() {
        let hollow = MemoryCandidate {
            fact: "【工作情节】[lebi-AI Care] Tool work may have produced\n- 情境：本会话\n- 做法：见会话记录\n- 产出：见会话记录\n- 用户反馈/修正：无\n- 可复用点：x".into(),
            tags: vec![tags::WORK_EPISODE.into()],
            zone: zones::WORK.into(),
            scope: Scope::User,
            confidence: Confidence::Medium,
            rationale: "bad".into(),
            supersedes: vec![],
        };
        let out = ReflectionOutput {
            summary: "hi".into(),
            memory_candidates: vec![hollow],
            ..Default::default()
        };
        let out = finalize_reflection_output(out);
        assert!(
            out.memory_candidates.is_empty(),
            "hollow Care-nudge episodes must be dropped"
        );
    }

    #[test]
    fn finalize_attaches_supersedes_for_same_slot() {
        let existing = hermes_memory::LoadedMemory {
            frontmatter: hermes_memory::MemoryFrontmatter {
                id: "mem_short".into(),
                created: chrono::Utc::now(),
                source: hermes_memory::Source::Reflection,
                confidence: hermes_memory::Confidence::High,
                pinned: false,
                tags: vec!["preference".into()],
                zone: "general".into(),
                supersedes: vec![],
                extra: Default::default(),
            },
            body: "用户偏好写文档时使用短句、先结论后细节的写作结构。".into(),
            source_path: std::path::PathBuf::from("x.md"),
            scope: hermes_memory::Scope::User,
        };
        let cand = MemoryCandidate {
            fact: "写成品：短句、先结论；科技稿用犀利观点风。".into(),
            tags: vec!["standard".into()],
            zone: "standards".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "修订同一格".into(),
            supersedes: vec![],
        };
        let out = ReflectionOutput {
            summary: "revised writing standard".into(),
            memory_candidates: vec![cand],
            ..Default::default()
        };
        let out = finalize_reflection_output_with(out, &[existing]);
        assert_eq!(out.memory_candidates.len(), 1);
        assert!(out.memory_candidates[0]
            .supersedes
            .iter()
            .any(|id| id == "mem_short"));
    }

    #[test]
    fn finalize_does_not_duplicate_quality_episode() {
        let existing = MemoryCandidate {
            fact: "【工作情节】季度复盘\n- 情境：用户要三段结构\n- 做法：先结论后证据\n- 产出：outputs/retro.md\n- 用户反馈/修正：无\n- 可复用点：先结论后证据".into(),
            tags: vec![tags::WORK_EPISODE.into()],
            zone: zones::WORK.into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "x".into(),
            supersedes: vec![],
        };
        assert!(episode_is_self_contained(&existing.fact));
        let out = ReflectionOutput {
            summary: "Did substantial work on the report again today with revisions".into(),
            memory_candidates: vec![existing],
            ..Default::default()
        };
        let out = finalize_reflection_output(out);
        assert_eq!(out.memory_candidates.len(), 1);
    }

    #[test]
    fn noise_text_detected() {
        assert!(is_internal_noise_text(
            "[lebi-AI Care] Tool work may have produced a deliverable."
        ));
        // Pre-branding marker still filtered for existing transcripts.
        assert!(is_internal_noise_text(
            "[Hermes Care] Tool work may have produced a deliverable."
        ));
        assert!(!is_internal_noise_text("帮我写一份本周复盘"));
    }
}
