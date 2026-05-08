//! hermes-memory: memory domain (parse, store, supersedes chain).
//!
//! Mirrors `hermes-skills`: filesystem-backed, frontmatter-typed, two-scope.
//! Relevance / matching / conflict detection live one layer up in
//! `hermes-reflect` (next iteration).

pub mod memory;
pub mod store;

pub use memory::{Confidence, LoadedMemory, MemoryFrontmatter, Scope, Source};
pub use store::{
    standard_project_root, standard_user_root, FsMemoryStore, MemoryStore, MemoryStoreError,
};
