//! Structured output of one reflection pass.
//!
//! The reflection LLM emits a single JSON object matching this schema. The
//! CLI / TUI then walks each candidate, asks the user, and persists the
//! accepted ones.

use serde::{Deserialize, Serialize};

use hermes_memory::{Confidence, Scope};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReflectionOutput {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub skill_candidates: Vec<SkillCandidate>,
    #[serde(default)]
    pub memory_candidates: Vec<MemoryCandidate>,
    #[serde(default)]
    pub conflicts: Vec<ConflictCandidate>,
}

impl ReflectionOutput {
    pub fn is_empty(&self) -> bool {
        self.skill_candidates.is_empty()
            && self.memory_candidates.is_empty()
            && self.conflicts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCandidate {
    /// kebab-case identifier; also the skill directory name.
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Markdown body for the SKILL.md (without frontmatter).
    pub body: String,
    pub rationale: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    /// One short statement. Goes into the memory file's body.
    pub fact: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub scope: Scope,
    pub confidence: Confidence,
    pub rationale: String,
    /// Existing memory ids this candidate replaces. Goes into
    /// `frontmatter.supersedes`.
    #[serde(default)]
    pub supersedes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictCandidate {
    /// Existing memory id this candidate is in tension with.
    pub with: String,
    /// Free-form: typically `contradiction` / `redundancy` / `scope_overlap`.
    pub kind: String,
    pub explain: String,
    /// Recommended resolution choices. v1 keeps these as free-form strings
    /// so the LLM can suggest novel options; the CLI displays them
    /// verbatim.
    #[serde(default)]
    pub options: Vec<String>,
}
