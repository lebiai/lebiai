//! hermes-memory: memory domain (parse, store, supersedes chain).
//!
//! Mirrors `hermes-skills`: filesystem-backed, frontmatter-typed, two-scope.
//! Relevance / matching / conflict detection live one layer up in
//! `hermes-reflect` (next iteration).

pub mod distill;
#[cfg(feature = "embed")]
pub mod embed;
pub mod memory;
pub mod palace;
pub mod profile;
pub mod relevance;
pub mod scoped;
pub mod slot;
pub mod stats;
pub mod store;
pub mod topics;

#[cfg(feature = "embed")]
pub use embed::EmbedIndex;
pub use memory::{
    filter_visible, views_for, visible_owned, visible_to, Confidence, LoadedMemory,
    MemoryFrontmatter, Scope, Source,
};
pub use palace::{get_zone, group_by_zone};
pub use profile::{load_profile, profile_path, save_profile};
pub use relevance::{
    search_memories, search_memories_effective, search_memories_scored,
    search_memories_with_effectiveness,
};
pub use scoped::{resolve_owner, OwnerDefault, ScopedMemoryStore};
pub use slot::{infer_slot, is_worthless_for_living, living_rules, same_slot_ids, WorkSlot};
pub use stats::{
    load_effectiveness, record as record_memory_stat, MemoryEffectiveness, MemoryEvent,
    MemoryStatEntry,
};
pub use store::{
    standard_project_root, standard_user_root, FsMemoryStore, MemoryStore, MemoryStoreError,
    DEFAULT_DEDUP_THRESHOLD,
};
pub use topics::{TopicCard, TopicCards};

/// 本 crate 的测试专用夹具（`#[cfg(test)]`）。
///
/// 数据根在**进程**里是全局的（`LEBI_DATA_DIR`），所以「重定向数据根」这件事
/// 必须全 crate 抢**同一把**锁：两把锁互不串行，一个测试的还原动作会落到另一个
/// 测试的正文中间——最坏的结果不是红，而是**写到用户真实数据根**
/// （`~/.lebi-ai`）。`profile.rs` 与 `topics.rs` 共用这一份。
#[cfg(test)]
pub(crate) mod test_env {
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// 把数据根指到一个临时目录，跑完（**含 panic 展开**）自己还原。
    ///
    /// 不还原的话，同一个测试二进制后面的用例会看到一个脏数据根——那种红不是
    /// 信号，是噪声（`docs/records/20260914-personas.md` Ruling 1.8b-e）。
    pub(crate) fn with_data_dir(f: impl FnOnce(&Path)) {
        struct Restore {
            prev: Option<String>,
            _locked: std::sync::MutexGuard<'static, ()>,
            dir: tempfile::TempDir,
        }

        impl Drop for Restore {
            fn drop(&mut self) {
                match self.prev.take() {
                    Some(v) => std::env::set_var(hermes_core::paths::ENV_DATA_DIR, v),
                    None => std::env::remove_var(hermes_core::paths::ENV_DATA_DIR),
                }
            }
        }

        let restore = Restore {
            // 先拿锁再读 `prev`：等锁期间别的用例可能已经改过 / 还原过环境。
            // 中毒也要拿到锁——前一个测试失败不该让这一个跟着变红（真假难分）。
            _locked: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            prev: std::env::var(hermes_core::paths::ENV_DATA_DIR).ok(),
            dir: tempfile::tempdir().unwrap(),
        };
        std::env::set_var(hermes_core::paths::ENV_DATA_DIR, restore.dir.path());
        f(restore.dir.path());
        drop(restore);
    }
}
