//! End-to-end micro-reflection for one completed turn.
//!
//! Surfaces (CLI / GUI / server) only decide **when** to call this and **how
//! to present** the result. They must not re-implement gate / LLM / apply.
//!
//! ```text
//! should_micro_reflect?
//!   → micro_reflect (LLM)
//!   → apply_micro_output (auto-accept / defer / dedup)
//!   → optional profile+palace refresh when auto-accepted
//! ```

use hermes_core::{LlmProvider, Message};
use hermes_memory::{
    build_palace_index_simple, save_palace_index, save_profile, LoadedMemory, MemoryStore,
};
use hermes_skills::LoadedSkill;

use crate::compile::compile_profile;
use crate::micro::{has_explicit_intent, micro_reflect, should_micro_reflect};
use crate::micro_apply::{apply_micro_output, MicroApplyConfig, MicroApplyResult};
use crate::runner::ReflectError;

/// Inputs for one micro-reflection attempt after a turn completes.
pub struct MicroRunRequest<'a> {
    pub provider: &'a dyn LlmProvider,
    pub store: &'a dyn MemoryStore,
    pub turn_messages: &'a [Message],
    pub skills: &'a [LoadedSkill],
    pub memories: &'a [LoadedMemory],
    /// Turns since last micro-reflect for this session (periodic gate).
    pub turns_since_last: usize,
    pub apply: MicroApplyConfig,
    /// When true and memories were auto-accepted, recompile profile + palace.
    pub recompile_on_auto_accept: bool,
}

/// Outcome of [`run_micro_after_turn`].
#[derive(Debug)]
pub enum MicroRunOutcome {
    /// Cooldown not met and no explicit teaching intent.
    Skipped,
    /// Micro ran but model returned empty candidates.
    Empty,
    /// Micro ran and apply finished (may have 0 pending if all auto/skipped).
    Applied(MicroApplyResult),
}

impl MicroRunOutcome {
    pub fn applied(&self) -> Option<&MicroApplyResult> {
        match self {
            Self::Applied(r) => Some(r),
            _ => None,
        }
    }

    /// True when the cooldown counter should reset (we attempted a micro pass).
    pub fn did_run(&self) -> bool {
        !matches!(self, Self::Skipped)
    }
}

/// Run the full micro pipeline if due. Sync store I/O; async only for LLM.
pub async fn run_micro_after_turn(
    req: MicroRunRequest<'_>,
) -> Result<MicroRunOutcome, ReflectError> {
    if !should_micro_reflect(req.turn_messages, req.turns_since_last) {
        return Ok(MicroRunOutcome::Skipped);
    }

    let mut apply_cfg = req.apply;
    // Always recompute explicit intent from this turn's messages.
    apply_cfg.explicit_intent = has_explicit_intent(req.turn_messages);

    let output = micro_reflect(req.provider, req.turn_messages, req.skills, req.memories).await?;

    if output.is_empty() {
        return Ok(MicroRunOutcome::Empty);
    }

    let applied = apply_micro_output(output, req.store, &apply_cfg);

    if req.recompile_on_auto_accept && applied.auto_accepted > 0 {
        if let Ok(fresh) = req.store.list_active() {
            match compile_profile(req.provider, &fresh).await {
                Ok(profile) => {
                    if let Err(e) = save_profile(&profile) {
                        tracing::warn!(error=%e, "micro_run: save profile");
                    }
                }
                Err(e) => {
                    tracing::debug!(error=%e, "micro_run: profile compile failed");
                }
            }
            let idx = build_palace_index_simple(&fresh);
            if let Err(e) = save_palace_index(&idx) {
                tracing::warn!(error=%e, "micro_run: save palace index");
            }
        }
    }

    Ok(MicroRunOutcome::Applied(applied))
}

/// Helper for surfaces that track a per-session cooldown counter.
///
/// - On [`MicroRunOutcome::Skipped`]: increments counter by 1, returns Skipped.
/// - On run (Empty or Applied): resets counter to 0.
pub fn update_cooldown_after(outcome: &MicroRunOutcome, turns_since: &mut usize) {
    if outcome.did_run() {
        *turns_since = 0;
    } else {
        *turns_since = turns_since.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_increments_on_skip() {
        let mut n = 2;
        update_cooldown_after(&MicroRunOutcome::Skipped, &mut n);
        assert_eq!(n, 3);
    }

    #[test]
    fn cooldown_resets_on_run() {
        let mut n = 5;
        update_cooldown_after(&MicroRunOutcome::Empty, &mut n);
        assert_eq!(n, 0);
        n = 5;
        update_cooldown_after(
            &MicroRunOutcome::Applied(MicroApplyResult::default()),
            &mut n,
        );
        assert_eq!(n, 0);
    }
}
