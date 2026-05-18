//! Memory data types.
//!
//! A "memory" is a single durable fact / lesson / convention the agent has
//! decided is worth keeping across sessions. One memory = one short
//! statement. Aggregation, summarisation, and conflict resolution happen at
//! the curation layer (later), not by stuffing many points into one file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a memory lives on disk. Mirrors `hermes_skills::Scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// `~/.small-rust-hermes/memories/`
    User,
    /// `./.small-rust-hermes/memories/`
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Distilled by the reflection pipeline.
    Reflection,
    /// Written or edited by a human.
    User,
    /// Imported from another project / agent.
    Imported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Stable identifier. Use [`MemoryFrontmatter::new`] to allocate.
    pub id: String,

    pub created: chrono::DateTime<chrono::Utc>,

    pub source: Source,

    pub confidence: Confidence,

    /// `true` ⇒ load on every session (analogous to Claude Code's `core.md`).
    /// `false` ⇒ episodic; included by relevance only.
    #[serde(default)]
    pub pinned: bool,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "default_zone")]
    pub zone: String,

    /// IDs of older memories this one replaces. Listed by [`list_active`]
    /// causes those ids to be filtered out.
    ///
    /// [`list_active`]: crate::store::MemoryStore::list_active
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,

    /// Forward-compat for fields we don't model yet.
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

fn default_zone() -> String {
    "general".to_string()
}

impl MemoryFrontmatter {
    /// Build a fresh memory with a UUIDv4-based id and `created = now`.
    pub fn new(source: Source, confidence: Confidence, tags: Vec<String>, zone: String) -> Self {
        Self {
            id: format!("mem_{}", uuid::Uuid::new_v4().simple()),
            created: chrono::Utc::now(),
            source,
            confidence,
            pinned: false,
            tags,
            zone,
            supersedes: Vec::new(),
            extra: serde_yaml::Mapping::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedMemory {
    pub frontmatter: MemoryFrontmatter,
    pub body: String,
    pub source_path: PathBuf,
    pub scope: Scope,
}

impl LoadedMemory {
    pub fn id(&self) -> &str {
        &self.frontmatter.id
    }
}
