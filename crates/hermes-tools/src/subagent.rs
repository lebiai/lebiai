//! `subagent` tool: spawn a fresh child turn with isolated context.
//!
//! This is the runtime primitive the bundled `skill-creator` meta-skill relies
//! on to do real evaluations: the parent agent calls `subagent` once per test
//! case with a clean `system` + `prompt` and the child runs the task in a
//! fresh context (no parent reasoning leakage). The parent then grades the
//! returned outputs — also via `subagent` calls if it wants a grader subagent
//! with a different system prompt.
//!
//! Why fresh context: an agent that runs both the test AND the grade in the
//! same conversation has read the answer key; blind grading needs separation.
//!
//! Safety:
//! - Recursion guard: an [`AtomicUsize`] depth counter shared via Arc; tool
//!   refuses if `depth >= max_depth` (default 1 — parent spawns subagents,
//!   subagents don't spawn subagents → no fork bomb).
//! - Tool whitelist: the subagent only sees tools the caller named in
//!   `allow_tools`. The `subagent` tool itself is always excluded.
//! - Fresh `BuiltinToolHost`: built per-call from the same workspace and
//!   memory/skill stores, with NO `propose_ctx` and NO subagent context.
//!   That structural choice — not a check — is what prevents recursion.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use hermes_core::{
    ContentBlock, LlmProvider, Message, Result, Role, ToolCallOutcome, ToolHost, ToolSpec,
};
use hermes_memory::MemoryStore;
use hermes_skills::SkillStore;
use hermes_turn::{PermissionChecker, TurnConfig, TurnEvent, run_turn};
use serde::Deserialize;

/// Wiring needed by the `subagent` tool. Construct one at startup and inject
/// into [`BuiltinToolHost::with_subagent_ctx`].
pub struct SubagentContext {
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
    pub max_tokens: u32,
    pub max_tool_rounds: usize,
    pub permissions: PermissionChecker,
    pub workspace: PathBuf,
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    pub skill_store: Option<Arc<dyn SkillStore>>,
    /// Recursion depth tracker; shared via Arc so increments from parallel
    /// subagent invocations are coherent. Increment on entry, decrement on
    /// drop. Refuse to enter when depth >= max_depth.
    pub depth: Arc<AtomicUsize>,
    pub max_depth: usize,
}

impl SubagentContext {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        model: String,
        max_tokens: u32,
        max_tool_rounds: usize,
        permissions: PermissionChecker,
        workspace: PathBuf,
        memory_store: Option<Arc<dyn MemoryStore>>,
        skill_store: Option<Arc<dyn SkillStore>>,
    ) -> Self {
        Self {
            provider,
            model,
            max_tokens,
            max_tool_rounds,
            permissions,
            workspace,
            memory_store,
            skill_store,
            depth: Arc::new(AtomicUsize::new(0)),
            max_depth: 1,
        }
    }
}

#[derive(Deserialize)]
struct SubagentArgs {
    /// System prompt installed at the top of the subagent's context. This is
    /// the role/contract the subagent operates under (e.g. "You are a grader.
    /// Read the transcript at <path> and the output at <path>, then write
    /// grading.json matching the schema in references/schemas.md.")
    system: String,
    /// Initial user message. The subagent treats this as its single user
    /// turn and runs tool loops until it produces a final text response.
    prompt: String,
    /// Whitelist of tool names the subagent may use. Default: empty (text-only
    /// reasoning, no tool access). Common picks:
    /// - executor subagent: `["read", "write", "edit", "bash", "glob", "grep",
    ///   "skill_read", "skill_read_file"]`
    /// - grader subagent: `["read", "glob", "write"]` (read transcript +
    ///   outputs, write grading.json)
    /// - comparator subagent: `["read", "glob", "write"]` (read both outputs,
    ///   write comparison.json)
    /// The `subagent` tool itself is always excluded from the list (no nesting).
    #[serde(default)]
    allow_tools: Vec<String>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "subagent".into(),
        description: "Spawn a child agent in a fresh context to run a sub-task. \
            Use for: (a) executing each test-case prompt during skill evaluation \
            (clean context per run, no leakage from parent reasoning), \
            (b) grading transcripts with a dedicated grader prompt, \
            (c) blind A/B comparison between two outputs, \
            (d) description-optimization loop. \
            The child has its own context: it sees only the `system` and `prompt` \
            you pass — none of your conversation history. Returns the child's \
            final text reply plus a summary of tool calls it made. \
            Multiple `subagent` calls in the same response run in parallel (the model \
            batches them). Subagents cannot themselves call `subagent` (depth=1 hard cap)."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "system": {
                    "type": "string",
                    "description": "System prompt for the child. Sets its role and contract. For evaluator runs, include the skill body (read via skill_read first) or the path to the skill workspace."
                },
                "prompt": {
                    "type": "string",
                    "description": "The single user message the child receives. Be self-contained — no parent context leaks through."
                },
                "allow_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tool names the child may call. Default empty (text-only). Pick the minimum set: e.g. ['read','write','bash','glob','grep'] for an executor; ['read','glob','write'] for a grader.",
                    "default": []
                }
            },
            "required": ["system", "prompt"]
        }),
        requires_confirmation: true,
    }
}

/// Guard that decrements the depth counter on drop — keeps the counter
/// consistent even if `run_turn` panics or the host fails mid-call.
struct DepthGuard {
    depth: Arc<AtomicUsize>,
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        self.depth.fetch_sub(1, Ordering::SeqCst);
    }
}

pub async fn run(ctx: &SubagentContext, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: SubagentArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("subagent: bad args: {e}")))?;

    // Recursion guard — atomic CAS-style increment.
    let prev = ctx.depth.fetch_add(1, Ordering::SeqCst);
    if prev >= ctx.max_depth {
        // We over-incremented; roll back and refuse.
        ctx.depth.fetch_sub(1, Ordering::SeqCst);
        return Ok(ToolCallOutcome {
            content: format!(
                "subagent: refused — recursion depth {prev} already at max {} (subagents cannot themselves spawn subagents).",
                ctx.max_depth
            ),
            is_error: true,
        });
    }
    let _guard = DepthGuard { depth: ctx.depth.clone() };

    // Build a fresh BuiltinToolHost for the child. No propose_ctx and no
    // subagent_ctx → child literally cannot call `propose_skill` or `subagent`,
    // regardless of what allow_tools says.
    let mut child = crate::BuiltinToolHost::new(ctx.workspace.clone());
    if let Some(m) = &ctx.memory_store {
        child = child.with_memory_store(m.clone());
    }
    if let Some(s) = &ctx.skill_store {
        child = child.with_skill_store(s.clone());
    }

    let all_specs = child.list_tools().await?;
    let allowed: std::collections::HashSet<&str> =
        a.allow_tools.iter().map(|s| s.as_str()).collect();
    // Always exclude `subagent` from the child's tool list (depth guard would
    // catch a sneaky call anyway, but filter at the API surface so the model
    // doesn't see it advertised).
    let filtered: Vec<ToolSpec> = all_specs
        .into_iter()
        .filter(|t| t.name != "subagent" && allowed.contains(t.name.as_str()))
        .collect();

    // Report which tools the child can use — gives the parent a way to debug
    // when allow_tools contained a typo that silently dropped a tool.
    let advertised: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();

    let history = vec![Message::user_text(a.prompt.clone())];

    let turn_cfg = TurnConfig {
        model: ctx.model.clone(),
        system: Some(a.system),
        max_tokens: ctx.max_tokens,
        max_tool_rounds: ctx.max_tool_rounds,
        permissions: ctx.permissions.clone(),
    };

    // Collect tool-call summaries as events stream by — parent sees a digest
    // of what the child did instead of having to re-spawn the same conversation
    // to find out.
    let tool_calls: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let on_event = |ev: TurnEvent| {
        if let TurnEvent::ToolExecStart { summary, .. } = ev {
            if let Ok(mut v) = tool_calls.lock() {
                v.push(summary);
            }
        }
    };

    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let started = Instant::now();
    let output = run_turn(
        ctx.provider.as_ref(),
        &child,
        &filtered,
        &history,
        &turn_cfg,
        None, // confirm_tx: None → all `requires_confirmation` tools auto-approved
        on_event,
        cancel_rx,
    )
    .await?;
    let duration_ms = started.elapsed().as_millis() as u64;

    // Extract the child's final assistant text.
    let final_text: String = output
        .new_messages
        .iter()
        .filter(|m| matches!(m.role, Role::Assistant))
        .flat_map(|m| {
            m.content.iter().filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    let calls = tool_calls.into_inner().unwrap_or_default();
    let input_tokens = output.usage.input_tokens;
    let output_tokens = output.usage.output_tokens;

    let mut content = String::new();
    content.push_str("--- subagent reply ---\n");
    if final_text.is_empty() {
        content.push_str("(no text reply)\n");
    } else {
        content.push_str(&final_text);
        content.push('\n');
    }
    content.push_str("--- subagent telemetry ---\n");
    content.push_str(&format!(
        "tools_advertised: [{}]\n",
        advertised.join(", ")
    ));
    content.push_str(&format!("tool_calls: {}\n", calls.len()));
    for c in &calls {
        content.push_str(&format!("  - {c}\n"));
    }
    content.push_str(&format!(
        "duration_ms: {duration_ms}\ninput_tokens: {input_tokens}\noutput_tokens: {output_tokens}\n"
    ));

    Ok(ToolCallOutcome {
        content,
        is_error: false,
    })
}
