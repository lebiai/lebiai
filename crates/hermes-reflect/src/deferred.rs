//! Deferred candidate queue — candidates the user skipped/deferred during
//! reflection, persisted to disk so they can be re-evaluated in future sessions.
//!
//! The file target is **always passed in**: `save_at` / `load_at` / `clear_at`
//! take the data root as an argument, so no code path can drift into the
//! process-wide default root (which, on a user machine, is their real data).
//! The pathless wrappers below exist only for application entry points that
//! have already resolved that root once.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::jsonl;
use crate::output::{MemoryCandidate, SkillCandidate};

const FILE: &str = "deferred.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DeferredCandidate {
    #[serde(rename = "skill")]
    Skill(SkillCandidate),
    /// 记忆候选：连**来源会话的工位归属**一起存——批准发生在下一次会话里，
    /// 那时已经没有原会话可查（Task 1.8b）。
    ///
    /// 技能没有归属轴（`SkillCandidate` 没有 `owner`），所以只有这一支带它。
    #[serde(rename = "memory")]
    Memory {
        #[serde(flatten)]
        candidate: MemoryCandidate,
        /// 入队时该会话的工位（`hermes_core::persona::memory_owner_for`）。
        /// 老文件没有这个键 → `None`（安全方向：落全局）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_owner: Option<String>,
    },
}

/// `deferred.jsonl` inside an explicit data root.
pub fn path_in(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// Append a deferred candidate into `root`. Best-effort write.
pub fn save_at(root: &Path, candidate: DeferredCandidate) {
    if let Err(e) = jsonl::append_line(&path_in(root), &candidate) {
        tracing::warn!(error=%e, "failed to save deferred candidate");
    }
}

/// Load all deferred candidates from `root`. Missing file is not an error.
pub fn load_at(root: &Path) -> Result<Vec<DeferredCandidate>> {
    jsonl::read_lines(&path_in(root), "deferred")
}

/// Clear all deferred candidates in `root` (after re-evaluation).
pub fn clear_at(root: &Path) -> Result<()> {
    let path = path_in(root);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Append into the process data root — application entry points only.
pub fn save(candidate: DeferredCandidate) {
    save_at(&hermes_core::data_root(), candidate);
}

/// Load from the process data root — application entry points only.
pub fn load() -> Result<Vec<DeferredCandidate>> {
    load_at(&hermes_core::data_root())
}

/// Clear in the process data root — application entry points only.
pub fn clear() -> Result<()> {
    clear_at(&hermes_core::data_root())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, Scope};

    fn mem(fact: &str) -> DeferredCandidate {
        with_owner(fact, None)
    }

    fn with_owner(fact: &str, session_owner: Option<&str>) -> DeferredCandidate {
        DeferredCandidate::Memory {
            candidate: MemoryCandidate {
                fact: fact.into(),
                tags: vec!["rust".into()],
                zone: "preferences".into(),
                scope: Scope::User,
                confidence: Confidence::High,
                rationale: "explicit".into(),
                supersedes: vec![],
                owner: None,
            },
            session_owner: session_owner.map(str::to_string),
        }
    }

    #[test]
    fn save_load_clear_cycle_in_explicit_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        save_at(root, mem("user prefers anyhow"));
        save_at(
            root,
            DeferredCandidate::Skill(SkillCandidate {
                name: "rust-error".into(),
                description: "switch unwrap to anyhow".into(),
                triggers: vec!["rust".into()],
                body: "step 1\nstep 2".into(),
                rationale: "reusable".into(),
                confidence: Confidence::Medium,
            }),
        );

        let loaded = load_at(root).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(matches!(loaded[0], DeferredCandidate::Memory { .. }));
        assert!(matches!(loaded[1], DeferredCandidate::Skill(_)));

        clear_at(root).unwrap();
        assert!(!path_in(root).exists());
        assert!(load_at(root).unwrap().is_empty());
    }

    /// 来源工位必须真的落进文件——批准发生在下一次会话，那时没有原会话可查。
    #[test]
    fn the_source_workspace_is_written_to_the_line() {
        let dir = tempfile::tempdir().unwrap();
        save_at(
            dir.path(),
            with_owner("林碳报告只引 IEA 的数据口径", Some("xiao-xie")),
        );

        let raw = std::fs::read_to_string(path_in(dir.path())).unwrap();
        assert!(
            raw.contains(r#""session_owner":"xiao-xie""#),
            "线格式里要看得到工位：{raw}"
        );
        let loaded = load_at(dir.path()).unwrap();
        let DeferredCandidate::Memory { session_owner, .. } = &loaded[0] else {
            panic!("memory candidate");
        };
        assert_eq!(session_owner.as_deref(), Some("xiao-xie"));

        // 没有工位的条目不许在文件里留一个 `session_owner: null` 键。
        save_at(dir.path(), mem("另一条没有工位的记忆"));
        let raw = std::fs::read_to_string(path_in(dir.path())).unwrap();
        assert_eq!(raw.matches("session_owner").count(), 1, "{raw}");
    }

    /// 老 `deferred.jsonl`（没有 `session_owner` 键）必须照样读得进来。
    #[test]
    fn an_old_line_without_the_new_key_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let old = concat!(
            r#"{"kind":"memory","fact":"旧行","tags":["rust"],"zone":"preferences","#,
            r#""scope":"user","confidence":"high","rationale":"explicit","supersedes":[]}"#,
            "\n"
        );
        std::fs::write(path_in(dir.path()), old).unwrap();

        let loaded = load_at(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        let DeferredCandidate::Memory {
            candidate,
            session_owner,
        } = &loaded[0]
        else {
            panic!("memory candidate");
        };
        assert_eq!(candidate.fact, "旧行");
        assert_eq!(*session_owner, None, "老文件没有这个键 → None（安全方向）");
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
                        save_at(&root, mem(&format!("concurrent fact {t}-{i}")));
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
            load_at(&root).unwrap().len(),
            THREADS * PER_THREAD,
            "every append must survive as its own parseable record"
        );
    }
}
