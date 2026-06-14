//! Agentic loop: receive a goal, autonomously iterate until complete.
//!
//! `run_agent()` calls `run_turn()` in a loop, injecting progress-assessment
//! messages between turns. The LLM signals completion with `[GOAL_COMPLETE]`
//! or failure with `[GOAL_FAILED]`. A hard iteration cap prevents infinite
//! loops. Context compaction is applied between turns when needed.

use hermes_core::{
    compaction, LlmProvider, Message, Role, Session, SessionMeta, ToolHost, ToolSpec, Usage,
};
use tokio::sync::{mpsc, oneshot};

use crate::{ConfirmRequest, TurnConfig, TurnEvent, TurnOutput};

const AGENT_SYSTEM_SUFFIX: &str = "\
You are an autonomous agent working toward a goal.

## Workflow — PLAN then EXECUTE

PHASE 1 — PLAN (first turn, before writing any code or files):
- Use `think` to analyze the goal, identify requirements, and design your approach.
- Use `todo_write` to lay out a step-by-step plan of small, concrete tasks.
- Do NOT write files, run commands, or make changes in this phase.

PHASE 2 — EXECUTE (subsequent turns):
- Work through your todo list one step at a time.
- Use `todo_write` to update statuses: mark exactly one task `in_progress` before \
starting it, and `completed` the moment it's done.
- Use `todo_list` between steps to review progress.

PHASE 3 — VERIFY:
- Test or review your work before declaring completion.

## Important Rules

- Only call tools listed in your tool definitions — never invent tool names.
- For large files (>150 lines): write a minimal skeleton first, then use `edit` to \
fill in sections incrementally. Never generate an entire large file in one tool call \
— it risks output truncation and wastes work.
- Prefer `edit` over `write` when modifying existing files.
- When the goal is fully achieved: output [GOAL_COMPLETE] followed by a brief summary.
- On unrecoverable error: output [GOAL_FAILED] followed by an explanation.
- Do NOT output [GOAL_COMPLETE] until verified.";

const GOAL_COMPLETE_MARKER: &str = "[GOAL_COMPLETE]";
const GOAL_FAILED_MARKER: &str = "[GOAL_FAILED]";

/// Configuration for the agentic loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// The high-level goal the agent should achieve.
    pub goal: String,
    /// Maximum number of turn iterations before forcing stop.
    pub max_iterations: usize,
    /// Turn-level config reused for each `run_turn()` call.
    pub turn_config: TurnConfig,
    /// Model context window size (for compaction threshold).
    pub context_model_limit: usize,
    /// Fraction of context to reserve (for compaction threshold).
    pub context_headroom: f64,
    /// Number of recent turn pairs to keep verbatim during compaction.
    pub context_keep_recent_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            goal: String::new(),
            max_iterations: 50,
            turn_config: TurnConfig::default(),
            context_model_limit: 128_000,
            context_headroom: 0.18,
            context_keep_recent_turns: 4,
        }
    }
}

/// Events emitted by the agentic loop.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A new turn iteration is starting.
    TurnStart { iteration: usize, max: usize },
    /// An event from the inner `run_turn()` call.
    TurnEvent(TurnEvent),
    /// A turn iteration has completed.
    TurnEnd { iteration: usize },
    /// Context was compacted between turns.
    Compacted { removed: usize },
    /// The agent declared the goal complete.
    GoalComplete { summary: String },
    /// The agent declared the goal failed.
    GoalFailed { reason: String },
    /// The iteration limit was reached without completion.
    MaxIterationsReached,
}

/// Output of the agentic loop.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// All messages produced across all iterations.
    pub messages: Vec<Message>,
    /// Cumulative token usage across all iterations.
    pub total_usage: Usage,
    /// Number of iterations executed.
    pub iterations: usize,
    /// Whether the goal was completed successfully.
    pub completed: bool,
}

/// Run the agentic loop: iterate `run_turn()` until the goal is complete or
/// the iteration limit is hit.
///
/// Cancellation is checked between iterations. Within a turn, the per-turn
/// cancel oneshot is never fired (the outer cancel is consumed here). The
/// primary safety mechanism is `max_iterations`.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent<F>(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[ToolSpec],
    history: &[Message],
    agent_config: &AgentConfig,
    confirm_tx: Option<mpsc::Sender<ConfirmRequest>>,
    on_event: F,
    mut cancel: oneshot::Receiver<()>,
) -> hermes_core::Result<AgentOutput>
where
    F: Fn(AgentEvent) + Send + Sync + Clone,
{
    let agent_system = match &agent_config.turn_config.system {
        Some(base) => format!("{base}\n\n{AGENT_SYSTEM_SUFFIX}"),
        None => AGENT_SYSTEM_SUFFIX.to_string(),
    };

    let mut turn_config = agent_config.turn_config.clone();
    turn_config.system = Some(agent_system.clone());

    let mut messages: Vec<Message> = history.to_vec();
    messages.push(Message::user_text(&agent_config.goal));

    let mut total_usage = Usage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
    };

    let max = agent_config.max_iterations;
    let mut iterations = 0;

    for i in 0..max {
        // Check cancellation between iterations.
        match cancel.try_recv() {
            Ok(()) | Err(oneshot::error::TryRecvError::Closed) => break,
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        on_event(AgentEvent::TurnStart {
            iteration: i + 1,
            max,
        });

        let evt = on_event.clone();
        let turn_on_event = move |event: TurnEvent| {
            evt(AgentEvent::TurnEvent(event));
        };

        // Per-iteration cancel: never fires (we check the outer cancel above).
        // This is acceptable — max_iterations is the hard safety limit.
        let (_no_cancel_tx, no_cancel_rx) = oneshot::channel::<()>();

        let output: TurnOutput = crate::run_turn(
            provider,
            host,
            tools,
            &messages,
            &turn_config,
            confirm_tx.clone(),
            turn_on_event,
            no_cancel_rx,
        )
        .await?;

        messages.extend_from_slice(&output.new_messages);
        total_usage.input_tokens += output.usage.input_tokens;
        total_usage.output_tokens += output.usage.output_tokens;
        total_usage.cache_read_tokens += output.usage.cache_read_tokens;
        total_usage.cache_creation_tokens += output.usage.cache_creation_tokens;
        iterations = i + 1;

        on_event(AgentEvent::TurnEnd { iteration: iterations });

        // Scan assistant messages for completion/failure markers.
        let last_assistant = messages.iter().rev().find(|m| m.role == Role::Assistant);
        if let Some(msg) = last_assistant {
            let text = msg
                .content
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("");

            if let Some(pos) = text.find(GOAL_COMPLETE_MARKER) {
                let summary = text[pos + GOAL_COMPLETE_MARKER.len()..].trim().to_string();
                on_event(AgentEvent::GoalComplete {
                    summary: summary.clone(),
                });
                return Ok(AgentOutput {
                    messages,
                    total_usage,
                    iterations,
                    completed: true,
                });
            }

            if let Some(pos) = text.find(GOAL_FAILED_MARKER) {
                let reason = text[pos + GOAL_FAILED_MARKER.len()..].trim().to_string();
                on_event(AgentEvent::GoalFailed {
                    reason: reason.clone(),
                });
                return Ok(AgentOutput {
                    messages,
                    total_usage,
                    iterations,
                    completed: false,
                });
            }
        }

        // Context compaction between turns.
        let mut session = Session {
            meta: SessionMeta::new(&turn_config.model, "agent"),
            messages: messages.clone(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        let tools_approx = compaction::estimate_tokens(
            &serde_json::to_string(&tools).unwrap_or_default(),
        );
        if compaction::should_compact(
            &agent_system,
            &session,
            tools_approx,
            agent_config.context_model_limit,
            agent_config.context_headroom,
        ) {
            match compaction::compact_session(
                provider,
                &mut session,
                agent_config.context_keep_recent_turns,
            )
            .await
            {
                Ok(n) => {
                    on_event(AgentEvent::Compacted { removed: n });
                    messages = session.messages;
                }
                Err(e) => {
                    tracing::warn!(error=%e, "context compaction failed in agent loop");
                }
            }
        }

        // Inject progress-assessment message for the next turn.
        let progress_msg = format!(
            "[Agent Progress Check]\n\
             Goal: {}\n\
             Iterations completed: {}/{}\n\
             Review your todo list with `todo_list`. \
             If all tasks are done and verified, respond with [GOAL_COMPLETE]. \
             Otherwise, continue with the next pending task.",
            agent_config.goal, iterations, max,
        );
        messages.push(Message::user_text(progress_msg));
    }

    on_event(AgentEvent::MaxIterationsReached);
    Ok(AgentOutput {
        messages,
        total_usage,
        iterations,
        completed: false,
    })
}
