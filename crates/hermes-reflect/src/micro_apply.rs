//! Apply a micro-reflection [`ReflectionOutput`] to the memory store.
//!
//! Shared by CLI and GUI so auto-accept, near-duplicate rejection, conflict
//! gating, deferred queue, and accept logging stay one implementation.
//!
//! Skills are never auto-written — they always become pending (and are also
//! appended to `deferred.jsonl` for session-end re-evaluation).

use std::path::PathBuf;

use hermes_memory::{Confidence, MemoryStore, MemoryStoreError, DEFAULT_DEDUP_THRESHOLD};

use crate::candidate;
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
    /// Data root that `deferred.jsonl` / `reflect-log.jsonl` land in. Explicit
    /// so a caller (a test, a second root) can never drift into the process-wide
    /// default root by accident.
    pub data_root: PathBuf,
    /// 当前会话的人物归属。`None` = 无人物 / 自带角色 → 一切落全局。
    pub memory_owner: Option<String>,
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
            data_root: hermes_core::data_root(),
            memory_owner: None,
        }
    }

    /// Desktop / server: pending goes to inbox. Do not grow a second file.
    pub fn inbox_only(mut self) -> Self {
        self.queue_deferred = false;
        self
    }

    /// Write reflection side files into `data_root` instead of the process root.
    pub fn with_data_root(mut self, data_root: PathBuf) -> Self {
        self.data_root = data_root;
        self
    }

    /// 归属由会话决定，不由调用方到处传字面量。
    pub fn with_memory_owner(mut self, owner: Option<String>) -> Self {
        self.memory_owner = owner;
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

/// 落 deferred 的记忆候选要连**来源工位**一起写下去：批准发生在下一次会话
/// （CLI `review_deferred`），那时已经没有原会话可查（Task 1.8b）。
fn deferred_memory(c: &MemoryCandidate, config: &MicroApplyConfig) -> DeferredCandidate {
    DeferredCandidate::Memory {
        candidate: c.clone(),
        session_owner: config.memory_owner.clone(),
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
                deferred::save_at(&config.data_root, deferred_memory(c, config));
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
                    deferred::save_at(&config.data_root, deferred_memory(c, config));
                }
                result.pending_memories.push(c.clone());
                continue;
            }
        }

        // 落盘归属：判定只有一个地方（`candidate::frontmatter_for` → `resolve_owner`），
        // 这里只传参。
        // 反思路径的默认方向是 **Global**：模型没说清就落全局（宁可不隔离，不误隔离）。
        let fm = candidate::frontmatter_for(c, config.memory_owner.as_deref());
        match store.put(c.scope, fm, &c.fact) {
            Ok(path) => {
                tracing::info!(path=%path.display(), "micro auto-accepted memory");
                result.auto_accepted += 1;
                result.auto_accepted_paths.push(path);
                log::append_at(
                    &config.data_root,
                    ReflectLogEntry {
                        at: chrono::Utc::now(),
                        session_id: config.session_id.clone(),
                        kind: CandidateKind::Memory,
                        action: ActionTaken::AutoAccept,
                        label: c.fact.lines().next().unwrap_or("").to_string(),
                    },
                );
            }
            Err(e) => {
                tracing::warn!(error=%e, "micro auto-accept put failed");
                if config.queue_deferred {
                    deferred::save_at(&config.data_root, deferred_memory(c, config));
                }
                result.pending_memories.push(c.clone());
            }
        }
    }

    for c in &output.skill_candidates {
        if config.queue_deferred {
            deferred::save_at(&config.data_root, DeferredCandidate::Skill(c.clone()));
        }
        result.pending_skills.push(c.clone());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{FsMemoryStore, MemoryFrontmatter, Scope, Source};

    /// File size in bytes, 0 when missing. Used to prove a directory gained nothing.
    fn file_len(p: &std::path::Path) -> u64 {
        std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    }

    fn store() -> (tempfile::TempDir, FsMemoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = FsMemoryStore::new(dir.path().to_path_buf(), None);
        (dir, s)
    }

    /// Config bound to a test-owned root — reflection side files must never
    /// reach the process data root from a test.
    fn cfg_in(root: &std::path::Path, auto_accept: bool) -> MicroApplyConfig {
        MicroApplyConfig::new("sess", auto_accept, Confidence::Medium, false)
            .with_data_root(root.to_path_buf())
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
            owner: None,
        }
    }

    /// `zone: "general"` 是有意的：`memory_passes_gate` 把 `work` 当工作情节，
    /// 要求正文 ≥36 字且自洽（`episode_is_self_contained`）。本组测试验的是归属，
    /// 用中立 zone 才不会被情节门槛绊住。
    fn candidate(fact: &str, owner: Option<&str>) -> MemoryCandidate {
        MemoryCandidate {
            fact: fact.to_string(),
            tags: vec![],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: String::new(),
            supersedes: vec![],
            owner: owner.map(str::to_string),
        }
    }

    /// auto-accept 需要 `auto_accept_memories && confidence >= floor && supersedes 空 && 无冲突`。
    fn apply_with(
        dir: &std::path::Path,
        store: &dyn MemoryStore,
        session_owner: Option<&str>,
        cands: Vec<MemoryCandidate>,
    ) {
        let output = ReflectionOutput {
            summary: String::new(),
            skill_candidates: vec![],
            memory_candidates: cands,
            conflicts: vec![],
        };
        let cfg = MicroApplyConfig::new("s1", true, Confidence::High, true)
            .inbox_only()
            .with_data_root(dir.to_path_buf())
            .with_memory_owner(session_owner.map(str::to_string));
        apply_micro_output(output, store, &cfg);
    }

    fn owner_of(store: &dyn MemoryStore, needle: &str) -> Option<String> {
        store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains(needle))
            .unwrap_or_else(|| panic!("no memory containing {needle}"))
            .frontmatter
            .owner
    }

    #[test]
    fn ownership_follows_the_four_rules() {
        let (d, s) = store();
        apply_with(
            d.path(),
            &s,
            Some("xiao-xie"),
            vec![
                candidate("林碳报告只引 IEA 的数据口径", Some("xiao-xie")), // 本人物 → 归它
                candidate("以后交付都要 Word 文件放桌面", None),            // 没写 → 全局
                candidate("新闻稿标题不夸张、不用惊叹号", Some("wang-hai-yan")), // 写了别人 → 全局
            ],
        );
        assert_eq!(owner_of(&s, "IEA").as_deref(), Some("xiao-xie"));
        assert_eq!(owner_of(&s, "Word"), None);
        assert_eq!(owner_of(&s, "标题"), None, "模型不能替别人落盘");
    }

    #[test]
    fn a_builtin_session_always_writes_globally() {
        let (d, s) = store();
        // 自带角色 memory_owner() == None，即使模型写了 id 也不认
        apply_with(
            d.path(),
            &s,
            None,
            vec![candidate("林碳报告只引 IEA 的数据口径", Some("li-xian"))],
        );
        assert_eq!(owner_of(&s, "IEA"), None, "自带角色是地基，不搞专业分工");
    }

    /// Ruling 1.6-c 的偏好闸在反思路径同样生效：模型把用户偏好写成本工位
    /// 也不许落成本工位私有——否则那条偏好只在顶帽子下生效。
    #[test]
    fn a_user_preference_stays_global_even_with_the_session_persona_id() {
        let (d, s) = store();
        let mut by_zone = candidate("用户偏好：交付一律用 Word 文件", Some("xiao-xie"));
        by_zone.zone = "preferences".into();
        let mut by_tag = candidate("用户偏好：会议纪要当天给结论", Some("xiao-xie"));
        by_tag.zone = "general".into();
        by_tag.tags = vec!["preference".into()];
        let mut by_standard_zone = candidate("同行业稿件一律先给数据来源", Some("xiao-xie"));
        by_standard_zone.zone = "standards".into();
        let mut by_standard_tag = candidate("稿件里的数字都要标出处、给链接", Some("xiao-xie"));
        by_standard_tag.zone = "general".into();
        by_standard_tag.tags = vec!["standard".into()];
        apply_with(
            d.path(),
            &s,
            Some("xiao-xie"),
            vec![by_zone, by_tag, by_standard_zone, by_standard_tag],
        );
        assert_eq!(owner_of(&s, "Word"), None, "zone=preferences 必须落全局");
        assert_eq!(
            owner_of(&s, "会议纪要"),
            None,
            "只打 preference tag 也必须落全局"
        );
        assert_eq!(owner_of(&s, "数据来源"), None, "zone=standards 必须落全局");
        assert_eq!(
            owner_of(&s, "标出处"),
            None,
            "只打 standard tag 也必须落全局"
        );
    }

    #[test]
    fn auto_accept_off_defers_all_memories() {
        let (d, s) = store();
        let cfg = cfg_in(d.path(), false);
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
        let (d, s) = store();
        let cfg = cfg_in(d.path(), true);
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
        let (d, s) = store();
        let cfg = cfg_in(d.path(), true);
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
        let (d, s) = store();
        let cfg = cfg_in(d.path(), true);
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
        let (d, s) = store();
        let fm = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        s.put(
            Scope::User,
            fm,
            "The user prefers vim as their primary editor",
        )
        .unwrap();

        let cfg = cfg_in(d.path(), true);
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
    #[test]
    fn micro_apply_log_lands_in_injected_root_only() {
        let process_root = hermes_core::data_root();
        let before = file_len(&process_root.join("reflect-log.jsonl"));

        let (d, s) = store();
        let cfg = MicroApplyConfig::new("sess", true, Confidence::Medium, false)
            .with_data_root(d.path().to_path_buf());
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand("probe: log must not escape", Confidence::High)],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.auto_accepted, 1);

        assert!(
            d.path().join("reflect-log.jsonl").exists(),
            "accept log must land in the injected root"
        );
        assert_eq!(
            file_len(&process_root.join("reflect-log.jsonl")),
            before,
            "process data root must not gain a reflect-log line"
        );
    }

    #[test]
    fn micro_apply_deferred_lands_in_injected_root_only() {
        let process_root = hermes_core::data_root();
        let before = file_len(&process_root.join("deferred.jsonl"));

        let (d, s) = store();
        let cfg = MicroApplyConfig::new("sess", false, Confidence::Medium, false)
            .with_data_root(d.path().to_path_buf());
        let out = ReflectionOutput {
            summary: "s".into(),
            skill_candidates: vec![],
            memory_candidates: vec![mem_cand(
                "probe: deferred must not escape",
                Confidence::High,
            )],
            conflicts: vec![],
        };
        let r = apply_micro_output(out, &s, &cfg);
        assert_eq!(r.pending_memories.len(), 1);

        assert!(
            d.path().join("deferred.jsonl").exists(),
            "deferred candidate must land in the injected root"
        );
        assert_eq!(
            file_len(&process_root.join("deferred.jsonl")),
            before,
            "process data root must not gain a deferred line"
        );
    }
}
