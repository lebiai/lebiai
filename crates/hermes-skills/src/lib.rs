//! hermes-skills: skill domain (parse, store, match).

pub mod relevance;
pub mod skill;
pub mod stats;
pub mod store;

pub use relevance::{match_for_query, match_for_query_hybrid, match_for_query_with_effectiveness};
pub use skill::{LoadedSkill, Scope, SkillFrontmatter};
pub use stats::{SkillEffectiveness, SkillEvent, SkillStatEntry, load_effectiveness, record as record_skill_stat};
pub use store::{standard_project_root, standard_user_root, FsSkillStore, SkillStore, SkillStoreError};
