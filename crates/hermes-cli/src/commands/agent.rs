//! `hermes run` — autonomous agent: receive a goal, iterate until complete.
//!
//! Mirrors the `hermes chat` subsystem wiring: loads skills, memories,
//! memory-backed tool host, session persistence, workspace system prompt,
//! context assembly, and end-of-session reflection.

use std::io::Write;
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{Message, Session, SessionEvent, SessionMeta};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;
use hermes_turn::{AgentConfig, AgentEvent, TurnConfig, TurnEvent};

use super::context::ContextSources;
use super::util::{build_active_provider, load_tool_host, session_path_for};
use hermes_tools::SubagentContext;

pub async fn run(goal: String, system: Option<String>, max_iterations: Option<usize>) -> Result<()> {
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;
    let max_iterations = max_iterations.unwrap_or(cfg.limits.agent_max_iterations);

    let workspace_root = cfg.workspace.root.clone();

    // --- memory store (enables memory_search tool) ---
    let memory_store_arc: Arc<dyn MemoryStore> = Arc::new(
        FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("memory store: {e}"))?,
    );
    let skill_store_arc: Arc<FsSkillStore> = Arc::new(
        FsSkillStore::standard().map_err(|e| anyhow::anyhow!("skill store: {e}"))?,
    );
    super::chat::auto_install_palace_skill(skill_store_arc.as_ref());
    super::chat::auto_install_skill_creator_skill(skill_store_arc.as_ref());
    super::chat::auto_install_find_skills_skill(skill_store_arc.as_ref());

    // Wire up `subagent` so an autonomous goal can spawn child contexts (skill
    // evaluation, blind comparison, grader subagents — see skill-creator).
    let subagent_ctx = Arc::new(SubagentContext::new(
        provider.clone(),
        provider_cfg.model.clone(),
        provider_cfg.max_tokens,
        cfg.limits.max_tool_rounds,
        hermes_turn::PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
        workspace_root.clone(),
        Some(memory_store_arc.clone()),
        Some(skill_store_arc.clone() as Arc<dyn SkillStore>),
    ));

    let host = load_tool_host(
        &workspace_root,
        Some(memory_store_arc.clone()),
        Some(skill_store_arc.clone() as Arc<dyn SkillStore>),
        None,
        Some(subagent_ctx),
    )
    .await?;
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?;

    // --- workspace clause in system prompt ---
    let system = super::chat::compose_system_prompt(system, &workspace_root);

    // --- skills & memories snapshot ---
    let all_skills: Vec<LoadedSkill> = skill_store_arc
        .list()
        .map_err(|e| anyhow::anyhow!("listing skills: {e}"))?;
    let always_active_refs: Vec<&LoadedSkill> = all_skills
        .iter()
        .filter(|s| s.frontmatter.always_active)
        .collect();

    let active_memories: Vec<LoadedMemory> = memory_store_arc
        .list_active()
        .map_err(|e| anyhow::anyhow!("listing memories: {e}"))?;
    let pinned_memories: Vec<LoadedMemory> = active_memories
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();
    let effectiveness: std::collections::HashMap<String, hermes_skills::SkillEffectiveness> =
        hermes_skills::load_effectiveness().unwrap_or_default();

    let mem_effectiveness: std::collections::HashMap<String, hermes_memory::MemoryEffectiveness> =
        hermes_memory::load_effectiveness().unwrap_or_default();

    let compiled_profile = hermes_memory::load_profile().unwrap_or(None);

    let palace_index: Option<String> = if active_memories.is_empty() {
        None
    } else {
        match hermes_memory::load_palace_index() {
            Ok(Some(idx)) => Some(idx),
            _ => {
                let idx = hermes_memory::build_palace_index_simple(&active_memories);
                if let Err(e) = hermes_memory::save_palace_index(&idx) {
                    tracing::warn!(error=%e, "save palace index");
                }
                Some(idx)
            }
        }
    };

    // --- build per-goal system prompt with context sources ---
    let sources = ContextSources {
        base: system.as_deref(),
        palace_index: palace_index.as_deref(),
        compiled_profile: compiled_profile.as_deref(),
        always_active_skills: &always_active_refs,
        pinned: &pinned_memories,
        active: &active_memories,
        all_skills: &all_skills,
        effectiveness: Some(&effectiveness),
        memory_effectiveness: Some(&mem_effectiveness),
        limits: cfg.limits,
    };
    let turn_system = sources.build_turn_system(&goal);

    // --- session persistence ---
    let meta = SessionMeta::new(&provider_cfg.model, provider.name());
    let session_path = session_path_for(&meta)?;
    let mut writer = SessionWriter::create(&session_path)
        .with_context(|| format!("creating session file at {}", session_path.display()))?;
    writer
        .append(&SessionEvent::Meta(meta.clone()))
        .context("writing session meta line")?;
    let mut session = Session::new(meta);

    // --- turn config (from config.toml, not hardcoded) ---
    let turn_config = TurnConfig {
        model: provider_cfg.model.clone(),
        system: if turn_system.is_empty() {
            None
        } else {
            Some(turn_system)
        },
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: hermes_turn::PermissionChecker::new(
            &cfg.permissions.allow,
            &cfg.permissions.deny,
        ),
    };

    let agent_config = AgentConfig {
        goal: goal.clone(),
        max_iterations,
        turn_config,
        context_model_limit: cfg.context.model_limit,
        context_headroom: cfg.context.headroom,
        context_keep_recent_turns: cfg.context.keep_recent_turns,
    };

    // --- banner ---
    eprintln!("workspace: {}", workspace_root.display());
    eprintln!("session:   {}", session_path.display());
    eprintln!("tools:     {} loaded", tools.len());
    eprintln!(
        "memory:    {} active ({} pinned)",
        active_memories.len(),
        pinned_memories.len()
    );
    eprintln!("skills:    {} loaded", all_skills.len());
    eprintln!("goal:      {goal}");
    eprintln!();

    let history: Vec<Message> = Vec::new();
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let (confirm_tx, mut confirm_rx) =
        tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);

    // --- confirmation task: prompt user for dangerous tools ---
    let confirm_task = tokio::spawn(async move {
        let mut always_allow: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut first_prompt = true;
        while let Some(req) = confirm_rx.recv().await {
            if always_allow.contains(&req.tool_name) {
                let _ = req.reply.send(hermes_turn::ConfirmAction::Allow);
                continue;
            }
            if first_prompt {
                eprintln!(
                    "\x1b[2m  (y = yes, a = always allow this tool, N = deny, or type a reason to deny with feedback)\x1b[0m"
                );
                first_prompt = false;
            }
            eprint!(
                "\x1b[1m\x1b[33m  ⚠ confirm\x1b[0m {}: {}  \x1b[1m[y/a/N/...]\x1b[0m ",
                req.tool_name, req.summary,
            );
            std::io::stderr().flush().ok();
            let mut input = String::new();
            let action = if std::io::stdin().read_line(&mut input).is_ok() {
                match input.trim().to_ascii_lowercase().as_str() {
                    "y" => hermes_turn::ConfirmAction::Allow,
                    "a" => {
                        always_allow.insert(req.tool_name.clone());
                        hermes_turn::ConfirmAction::AlwaysAllow
                    }
                    "" | "n" => hermes_turn::ConfirmAction::Deny { reason: None },
                    other => hermes_turn::ConfirmAction::Deny {
                        reason: Some(other.to_string()),
                    },
                }
            } else {
                hermes_turn::ConfirmAction::Deny { reason: None }
            };
            let _ = req.reply.send(action);
        }
    });

    let thinking_buf = std::sync::Mutex::new(String::new());

    let flush_thinking = |buf: &std::sync::Mutex<String>| {
        let mut b = buf.lock().unwrap();
        if !b.is_empty() {
            eprint!("\r\x1b[K");
            eprintln!("\x1b[90m  💭 ──────\x1b[0m");
            for line in b.lines() {
                eprintln!("\x1b[90m  │ {line}\x1b[0m");
            }
            b.clear();
        }
    };

    let on_event = |event: AgentEvent| {
        match event {
            AgentEvent::TurnStart { iteration, max } => {
                eprintln!("\x1b[1m--- Iteration {iteration}/{max} ---\x1b[0m");
            }
            AgentEvent::TurnEvent(te) => match te {
                TurnEvent::TextDelta(text) => {
                    flush_thinking(&thinking_buf);
                    print!("{text}");
                    std::io::stdout().flush().ok();
                }
                TurnEvent::ThinkingDelta(text) => {
                    let mut buf = thinking_buf.lock().unwrap();
                    buf.push_str(&text);
                    let preview: String = buf.chars().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect();
                    let preview = preview.replace('\n', " ");
                    drop(buf);
                    eprint!("\r\x1b[K\x1b[90m  💭 {preview}\x1b[0m");
                    std::io::stderr().flush().ok();
                }
                TurnEvent::ToolUseStart { name, .. } => {
                    flush_thinking(&thinking_buf);
                    eprint!("\r\x1b[K\x1b[33m  🔧 {name} …\x1b[0m");
                    std::io::stderr().flush().ok();
                }
                TurnEvent::ToolExecStart { summary, .. } => {
                    eprint!("\r\x1b[K");
                    eprintln!("\x1b[33m  🔧 {summary}\x1b[0m");
                }
                TurnEvent::ToolUseResult { content, is_error, .. } => {
                    if is_error {
                        eprintln!("\x1b[31m  ✗ {}\x1b[0m", content.lines().next().unwrap_or(""));
                    }
                }
                TurnEvent::ToolConfirmPending { tool_name, summary, .. } => {
                    tracing::debug!(tool_name, summary, "tool confirmation pending");
                }
                TurnEvent::Usage { .. } | TurnEvent::Done => {}
                TurnEvent::Error(msg) => {
                    eprintln!("\x1b[31m  error: {msg}\x1b[0m");
                }
            },
            AgentEvent::TurnEnd { iteration } => {
                eprintln!("\n\x1b[2m  iteration {iteration} done\x1b[0m");
            }
            AgentEvent::Compacted { removed } => {
                eprintln!("\x1b[2m  🗜 compacted: removed {removed} messages\x1b[0m");
            }
            AgentEvent::GoalComplete { summary } => {
                eprintln!("\n\x1b[32m✓ Goal complete:\x1b[0m {summary}");
            }
            AgentEvent::GoalFailed { reason } => {
                eprintln!("\n\x1b[31m✗ Goal failed:\x1b[0m {reason}");
            }
            AgentEvent::MaxIterationsReached => {
                eprintln!("\n\x1b[33m⚠ Max iterations ({max_iterations}) reached without completion.\x1b[0m");
            }
        }
    };

    let output = hermes_turn::run_agent(
        provider.as_ref(),
        host.as_ref(),
        &tools,
        &history,
        &agent_config,
        Some(confirm_tx),
        on_event,
        cancel_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    confirm_task.abort();

    // --- persist all messages to session JSONL ---
    for msg in &output.messages {
        session.messages.push(msg.clone());
        if let Err(e) = writer.append(&SessionEvent::Message(msg.clone())) {
            tracing::warn!(error=%e, "persist message");
        }
    }
    session.record_usage(output.total_usage);
    if let Err(e) = writer.append(&SessionEvent::Usage(output.total_usage)) {
        tracing::warn!(error=%e, "persist usage");
    }

    eprintln!();
    eprintln!("session saved: {}", session_path.display());

    tracing::info!(
        iterations = output.iterations,
        completed = output.completed,
        input_tokens = output.total_usage.input_tokens,
        output_tokens = output.total_usage.output_tokens,
        "agent loop done"
    );
    Ok(())
}
