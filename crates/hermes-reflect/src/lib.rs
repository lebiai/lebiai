//! hermes-reflect: turn finished sessions into skill / memory / conflict
//! candidates via an LLM.
//!
//! Two modes:
//! - **Full reflection** (`reflect`) — runs at session end on the entire
//!   transcript. Expensive but thorough.
//! - **Micro-reflection** (`micro_reflect`) — runs per-turn in the
//!   background. Cheap, fast, only looks at the latest turn.

pub mod compile;
pub mod deferred;
pub mod focused;
pub mod log;
pub mod micro;
pub mod output;
pub mod prompt;
pub mod runner;

pub use compile::{compile_palace_index, compile_profile, compile_zone_summary};
pub use deferred::{DeferredCandidate, save as deferred_save, load as deferred_load, clear as deferred_clear};
pub use focused::reflect_focused;
pub use log::{
    append as log_append, default_log_path as log_default_path, read_all as log_read_all,
    stats as log_stats, ActionTaken, CandidateKind, ReflectLogEntry, Stats,
};
pub use micro::{micro_reflect, should_micro_reflect};
pub use output::{ConflictCandidate, MemoryCandidate, ReflectionOutput, SkillCandidate};
pub use runner::{reflect, ReflectError};
