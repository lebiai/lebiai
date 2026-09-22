//! C-SESS: work-episode normalization for continuity ("越用越像你的手感").
//!
//! **Product rule:** a work episode must stay useful **even if the session
//! JSONL is deleted**. Never store "见会话记录" / internal nudge text.
//! Prefer **no** episode over a hollow one.

use hermes_core::companion::{tags, zones};

use crate::output::{MemoryCandidate, ReflectionOutput};

const EPISODE_MARKER: &str = "【工作情节】";
const MIN_EPISODE_BODY_CHARS: usize = 36;

/// Care delivery nudge markers. `[lebi-AI Care]` is the current brand;
/// `[Hermes Care]` stays recognized so pre-branding transcripts and stored
/// memories are still filtered from episode content.
const CARE_MARKERS: &[&str] = &["[lebi-AI Care]", "[Hermes Care]"];

/// True when this candidate is tagged/shaped as a work episode (not quality).
pub fn is_work_episode(c: &MemoryCandidate) -> bool {
    let zone = c.zone.trim().to_lowercase();
    if zones::is_work(&zone) {
        return true;
    }
    // 精确词读唯一词表；`contains` 是对「work-episode-<后缀>」这类复合标签的
    // 宽松兜底（今天的既有语义，保持不变）。
    if c.tags
        .iter()
        .any(|t| tags::is_episode_tag(t) || t.to_lowercase().contains(tags::WORK_EPISODE))
    {
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

    if fact.contains(EPISODE_MARKER) || c.tags.iter().any(|t| tags::is_episode_tag(t)) {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::WORK.to_string();
        }
        ensure_tag(&mut c.tags, tags::WORK_EPISODE);
        return;
    }

    if c.tags.iter().any(|t| tags::is_standard_tag(t)) {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::STANDARDS.to_string();
        }
        ensure_tag(&mut c.tags, tags::STANDARD);
        return;
    }

    if c.tags.iter().any(|t| tags::is_preference_tag(t)) {
        if c.zone.trim().is_empty() || c.zone == "general" {
            c.zone = zones::PREFERENCES.to_string();
        }
        ensure_tag(&mut c.tags, tags::PREFERENCE);
    }

    if c.zone.trim().is_empty() {
        c.zone = zones::GENERAL.to_string();
    } else {
        c.zone = zones::normalize(&c.zone).to_string();
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

/// Build a **self-contained** episode body from a real summary (no session pointers).
/// Disabled for low-signal summaries — seeding hollow episodes flooded the inbox.
pub fn seed_episode_from_summary(summary: &str) -> Option<MemoryCandidate> {
    // Product decision 2026-08-11: do not auto-seed episodes from summary text.
    // Prefer LLM-produced candidates that pass quality gates, or explicit preferences.
    let _ = summary;
    None
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
    use hermes_memory::{Confidence, Scope};

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
            owner: None,
        };
        normalize_candidate(&mut c);
        assert_eq!(c.zone, zones::WORK);
        assert!(c.tags.iter().any(|t| t == tags::WORK_EPISODE));
        assert!(is_work_episode(&c));
    }

    /// 工单验证点（Task 1.11）：`" episode "` 这类带空格的 tag 从「匹配不上」变成
    /// 「匹配得上」——本地 `"episode"` 字面量删掉、改读 `companion::tags::is_episode_tag`
    /// （唯一词表，带 trim）带来的归一化方向修正。
    #[test]
    fn a_padded_episode_tag_fixes_the_zone() {
        let mut c = MemoryCandidate {
            fact: "带空格 tag 的候选记忆，够长到能过门槛的一条。".into(),
            tags: vec![" episode ".into()],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "t".into(),
            supersedes: vec![],
            owner: None,
        };
        normalize_candidate(&mut c);
        assert_eq!(c.zone, zones::WORK, "` episode ` 必须把 zone 归到 work");
        assert!(c.tags.iter().any(|t| t == tags::WORK_EPISODE));
        assert!(is_work_episode(&c));
    }

    /// 词表住在 `companion::tags`，这里只证明「读的是同一份」：只打 tag、zone 缺省时
    /// 归类仍然正确（含它带的 trim）。
    #[test]
    fn a_user_level_tag_alone_fixes_the_zone() {
        for (tag, expected) in [
            ("preference", zones::PREFERENCES),
            ("Preference", zones::PREFERENCES),
            ("prefers", zones::PREFERENCES),
            (" preference ", zones::PREFERENCES),
            ("standard", zones::STANDARDS),
            ("STANDARD", zones::STANDARDS),
        ] {
            let mut c = MemoryCandidate {
                fact: "只打 tag 不填 zone 的一条候选记忆，够长到能过门槛。".into(),
                tags: vec![tag.into()],
                zone: "general".into(),
                scope: Scope::User,
                confidence: Confidence::High,
                rationale: "t".into(),
                supersedes: vec![],
                owner: None,
            };
            normalize_candidate(&mut c);
            assert_eq!(c.zone, expected, "tag {tag:?} 必须把 zone 归到 {expected}");
        }

        // 无关 tag 不动 zone
        let mut c = MemoryCandidate {
            fact: "无关 tag 的候选记忆，同样够长到能过门槛。".into(),
            tags: vec!["news".into()],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "t".into(),
            supersedes: vec![],
            owner: None,
        };
        normalize_candidate(&mut c);
        assert_eq!(c.zone, zones::GENERAL);
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
            owner: None,
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
                owner: None,
                because: None,
                intentional: false,
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
            owner: None,
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
            owner: None,
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
