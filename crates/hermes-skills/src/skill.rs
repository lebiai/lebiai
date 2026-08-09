//! Skill data types. Aligned with Anthropic's `SKILL.md` convention so
//! skills can be exchanged between this agent and Claude Code skills.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a skill lives on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// `~/.lebi-ai/skills/`
    User,
    /// `./.lebi-ai/skills/` — only relevant inside a project root.
    Project,
}

/// Frontmatter portion of a `SKILL.md` file. Unknown keys are preserved in
/// `extra` for forward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,

    pub description: String,

    /// Tokens / phrases that hint when this skill should be considered.
    #[serde(default)]
    pub triggers: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// If true, the skill body is always injected into the system prompt.
    #[serde(default)]
    pub always_active: bool,

    /// Catch-all for fields we do not yet model (e.g. `model`, `allowed_tools`).
    /// Preserved on roundtrip.
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

/// A skill loaded from disk: frontmatter + body + provenance.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub source: PathBuf,
    pub scope: Scope,
}

impl LoadedSkill {
    pub fn name(&self) -> &str {
        &self.frontmatter.name
    }
}
