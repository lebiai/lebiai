//! `propose_skill` tool: agent-initiated request to distill recent turns
//! into a reusable skill candidate.
//!
//! Flow:
//! - Marked as `requires_confirmation: true`, so the user sees a confirm
//!   modal before this tool runs at all.
//! - On Allow, the handler invokes a focused reflection over the last
//!   `focus_turns` messages (default 10), producing zero or one
//!   `SkillCandidate`.
//! - The candidate is pushed onto a shared `SkillProposeQueue` for the
//!   frontend (CLI or GUI) to drain after the turn — the user then approves
//!   or rejects via the normal reflection UI.
//! - The tool itself returns a short message to the LLM describing what was
//!   drafted; the actual skill file is only written after frontend approval.
//!
//! This preserves the project principle: skills are only born from the
//! reflection pipeline, never via direct agent write.

use std::sync::{Arc, Mutex, RwLock};

use hermes_core::{LlmProvider, Message, Result, ToolCallOutcome, ToolSpec};
use hermes_reflect::{reflect_focused, SkillCandidate};
use serde::Deserialize;

/// Shared queue of skill candidates awaiting frontend approval.
///
/// `BuiltinToolHost` holds one of these; CLI/GUI drain it after each turn
/// completes and route candidates through the existing reflection-approval
/// UI. Memory only — not persisted across sessions.
pub type SkillProposeQueue = Arc<Mutex<Vec<SkillCandidate>>>;

/// Snapshot of recent session messages, kept in sync by the chat loop.
pub type SessionMessages = Arc<RwLock<Vec<Message>>>;

/// Wiring needed by the `propose_skill` tool.
pub struct ProposeContext {
    pub provider: Arc<dyn LlmProvider>,
    pub messages: SessionMessages,
    pub queue: SkillProposeQueue,
}

#[derive(Deserialize)]
struct ProposeArgs {
    /// Free-form hint from the agent about what workflow to distill.
    /// Typically derived from the user's own request ("save this as a skill",
    /// "remember this procedure for next time").
    #[serde(default)]
    hint: Option<String>,
    /// How many of the most recent messages (including user / assistant /
    /// tool turns) to feed to the focused reflection. Default 20.
    #[serde(default = "default_focus_turns")]
    focus_turns: usize,
}

fn default_focus_turns() -> usize {
    20
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "propose_skill".into(),
        description: "Propose a reusable skill from the recent conversation. \
            Use this when the user explicitly asks to save a workflow as a skill \
            (e.g. \"save this as a skill\", \"remember this procedure\"), or when \
            you observe a successful multi-step procedure worth distilling \
            (especially after trial-and-error: the working path is the skill, \
            the failed attempts are notes to avoid). \
            The drafted skill is NOT saved immediately — it goes through the \
            user's reflection-approval UI. Pass `hint` to describe what \
            workflow you want distilled."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "hint": {
                    "type": "string",
                    "description": "One-line description of the workflow to distill, e.g. \"the steps I used to integrate the third-party news API after the first two attempts failed\". Helps the reflection LLM focus."
                },
                "focus_turns": {
                    "type": "integer",
                    "description": "How many of the most recent messages to consider. Default 20.",
                    "default": 20
                }
            },
            "required": []
        }),
        requires_confirmation: true,
    }
}

pub async fn run(
    ctx: &ProposeContext,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: ProposeArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("propose_skill: bad args: {e}")))?;

    let focus = a.focus_turns.clamp(1, 100);

    // Snapshot the recent N messages.
    let recent: Vec<Message> = {
        let guard = ctx
            .messages
            .read()
            .map_err(|e| hermes_core::Error::ToolHost(format!("propose_skill: messages lock: {e}")))?;
        let total = guard.len();
        let start = total.saturating_sub(focus);
        guard[start..].to_vec()
    };

    if recent.is_empty() {
        return Ok(ToolCallOutcome {
            content: "propose_skill: no conversation history yet to distill.".into(),
            is_error: true,
        });
    }

    let candidate = reflect_focused(
        ctx.provider.as_ref(),
        &recent,
        a.hint.as_deref(),
    )
    .await
    .map_err(|e| hermes_core::Error::ToolHost(format!("propose_skill: reflection failed: {e}")))?;

    match candidate {
        Some(c) => {
            let summary = format!(
                "Drafted skill candidate \"{}\" — {}. Awaiting your approval in the reflection panel before it's saved.",
                c.name, c.description
            );
            // Push onto the queue for the frontend to drain.
            if let Ok(mut q) = ctx.queue.lock() {
                q.push(c);
            }
            Ok(ToolCallOutcome {
                content: summary,
                is_error: false,
            })
        }
        None => Ok(ToolCallOutcome {
            content: "Reflection found nothing reusable enough to propose as a skill. Try giving a more specific `hint`, or describe more of the procedure first."
                .into(),
            is_error: false,
        }),
    }
}
