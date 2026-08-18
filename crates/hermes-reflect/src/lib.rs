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
pub mod episode;
pub mod focused;
pub mod inbox;
pub mod ledger;
pub mod log;
pub mod micro;
pub mod micro_apply;
pub mod micro_run;
pub mod output;
pub mod prompt;
pub mod runner;

pub use compile::{compile_palace_index, compile_profile};
pub use deferred::{
    clear as deferred_clear, load as deferred_load, save as deferred_save, DeferredCandidate,
};
pub use episode::{
    episode_is_self_contained, finalize_reflection_output, finalize_reflection_output_with,
    is_internal_noise_text, is_work_episode, normalize_candidate, seed_episode_from_summary,
};
pub use focused::reflect_focused;
pub use inbox::{
    clear as inbox_clear, count as inbox_count, enqueue_from_reflection,
    enqueue_from_reflection_marked, get as inbox_get, list as inbox_list, memory_passes_gate,
    remove as inbox_remove, skill_passes_gate, EnqueueMark, InboxItem, InboxPayload, InboxSource,
};
pub use ledger::{
    needs_distill, new_distill_id, record_success, DistillCursor, DistillSessionGuard,
};
pub use log::{
    append as log_append, default_log_path as log_default_path, read_all as log_read_all,
    stats as log_stats, ActionTaken, CandidateKind, ReflectLogEntry, Stats,
};
pub use micro::{has_explicit_intent, micro_reflect, should_micro_reflect};
pub use micro_apply::{apply_micro_output, MicroApplyConfig, MicroApplyResult};
pub use micro_run::{
    run_micro_after_turn, update_cooldown_after, MicroRunOutcome, MicroRunRequest,
};
pub use output::{ConflictCandidate, MemoryCandidate, ReflectionOutput, SkillCandidate};
pub use runner::{reflect, reflect_quick, ReflectError};
