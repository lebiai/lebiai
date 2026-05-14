//! hermes-memory: memory domain (parse, store, supersedes chain).
//!
//! Mirrors `hermes-skills`: filesystem-backed, frontmatter-typed, two-scope.
//! Relevance / matching / conflict detection live one layer up in
//! `hermes-reflect` (next iteration).

#[cfg(feature = "embed")]
pub mod embed;
pub mod memory;
pub mod relevance;
pub mod store;

#[cfg(feature = "embed")]
pub use embed::EmbedIndex;
pub use memory::{Confidence, LoadedMemory, MemoryFrontmatter, Scope, Source};
pub use relevance::{search_memories, search_memories_scored};
pub use store::{
    standard_project_root, standard_user_root, FsMemoryStore, MemoryStore, MemoryStoreError,
};
