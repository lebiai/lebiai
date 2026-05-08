//! hermes-skills: skill domain (parse, store, match).

pub mod relevance;
pub mod skill;
pub mod store;

pub use relevance::match_for_query;
pub use skill::{LoadedSkill, Scope, SkillFrontmatter};
pub use store::{standard_project_root, standard_user_root, FsSkillStore, SkillStore, SkillStoreError};
