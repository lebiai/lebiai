//! hermes-memory: memory domain (parse, store, supersedes chain).
//!
//! Mirrors `hermes-skills`: filesystem-backed, frontmatter-typed, two-scope.
//! Relevance / matching / conflict detection live one layer up in
//! `hermes-reflect` (next iteration).

#[cfg(feature = "embed")]
pub mod embed;
pub mod memory;
pub mod palace;
pub mod profile;
pub mod relevance;
pub mod stats;
pub mod store;

#[cfg(feature = "embed")]
pub use embed::EmbedIndex;
pub use memory::{Confidence, LoadedMemory, MemoryFrontmatter, Scope, Source};
pub use palace::{
    build_palace_index_simple, get_zone, group_by_zone, load_palace_index, load_zone_summary,
    palace_index_path, save_palace_index, save_zone_summary, zone_cache_dir, ZONE_CORE,
    ZONE_EPISODE, ZONE_GENERAL, ZONE_WORK,
};
pub use profile::{load_profile, profile_path, save_profile};
pub use relevance::{
    search_memories, search_memories_effective, search_memories_scored,
    search_memories_with_effectiveness,
};
pub use stats::{
    load_effectiveness, record as record_memory_stat, MemoryEffectiveness, MemoryEvent,
    MemoryStatEntry,
};
pub use store::{
    standard_project_root, standard_user_root, FsMemoryStore, MemoryStore, MemoryStoreError,
};
