//! hermes-skills: skill domain (parse, store, match).

pub mod install;
pub mod relevance;
pub mod skill;
pub mod stats;
pub mod store;

pub use install::{
    delete_skill, install_from_source, parse_skill_doc, validate_relative_path, DeleteOutcome,
    InstallOutcome, BUNDLED_SKILLS, MAX_FILES, MAX_FILE_BYTES, MAX_PATH_DEPTH, MAX_TOTAL_BYTES,
};
pub use relevance::{match_for_query, match_for_query_hybrid, match_for_query_with_effectiveness};
pub use skill::{LoadedSkill, Scope, SkillFrontmatter};
pub use stats::{SkillEffectiveness, SkillEvent, SkillStatEntry, load_effectiveness, record as record_skill_stat};
pub use store::{
    standard_project_root, standard_user_root, validate_skill_name, FsSkillStore, SkillStore,
    SkillStoreError,
};
