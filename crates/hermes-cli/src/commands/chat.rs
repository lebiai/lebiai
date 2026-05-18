//! `hermes chat` — multi-turn REPL with JSONL session persistence,
//! MCP tool support, memory injection, and per-turn skill matching.
//!
//! Context lifecycle:
//! - Session start: load all skills + all *active* memories from
//!   `FsSkillStore` / `FsMemoryStore`. Build a session-scoped `system`
//!   string with pinned-memory bodies + a one-line index of episodic
//!   memories + a name/description index of every skill.
//! - Per turn: re-stitch the system string with the bodies of skills whose
//!   triggers / name / description token-overlap the current user input.

use std::io::Write;
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{
    ContentBlock, LlmProvider, Role, Session, SessionEvent,
    SessionMeta, ToolHost,
};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;

use super::context::ContextSources;
use super::readline::{ChatLineEditor, LineOutcome};
use super::util::{build_active_provider, load_tool_host, session_path_for};

const MAX_TOOL_ROUNDS: usize = 10;

struct SessionStats {
    turn_count: usize,
    tool_calls: std::collections::HashMap<String, usize>,
    per_turn_usage: Vec<(u32, u32)>,
}

pub async fn run(
    system: Option<String>,
    model_override: Option<String>,
    resume_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let memory_store_arc: Arc<dyn MemoryStore> = Arc::new(
        FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("memory store: {e}"))?,
    );
    let host = load_tool_host(&workspace_root, Some(memory_store_arc.clone())).await?;
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?;

    // Prepend a hard-stated workspace clause to whatever system prompt
    // the user supplied. This is a soft constraint at the LLM level; the
    // hard constraint is the filesystem MCP's allowed-directory which
    // load_tool_host has already rewritten to match.
    let system = compose_system_prompt(system, &workspace_root);

    // ---- skill / memory snapshot for this session ----
    let skill_store = FsSkillStore::standard()
        .map_err(|e| anyhow::anyhow!("skill store: {e}"))?;

    let active_memories: Vec<LoadedMemory> = memory_store_arc
        .list_active()
        .map_err(|e| anyhow::anyhow!("listing memories: {e}"))?;
    let pinned_memories: Vec<LoadedMemory> = active_memories
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();

    // Load skill effectiveness data for deprioritizing low-use skills.
    let effectiveness: std::collections::HashMap<String, hermes_skills::SkillEffectiveness> =
        hermes_skills::load_effectiveness().unwrap_or_default();

    // Load memory effectiveness data for deprioritizing low-reference memories.
    let mem_effectiveness: std::collections::HashMap<String, hermes_memory::MemoryEffectiveness> =
        hermes_memory::load_effectiveness().unwrap_or_default();

    // --- Memory Palace: auto-install skill, build index, collect always-active ---
    auto_install_palace_skill(&skill_store);
    let all_skills: Vec<LoadedSkill> = skill_store
        .list()
        .map_err(|e| anyhow::anyhow!("listing skills: {e}"))?;
    let always_active_refs: Vec<&LoadedSkill> = all_skills
        .iter()
        .filter(|s| s.frontmatter.always_active)
        .collect();

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

    // ---- session: resume or fresh ----
    let (mut session, mut writer, session_path, resumed) = match resume_path {
        Some(path) => {
            let s = hermes_store::read_session(&path)
                .map_err(|e| anyhow::anyhow!("reading session {}: {e}", path.display()))?;
            let w = SessionWriter::open_append(&path)
                .map_err(|e| anyhow::anyhow!("opening session {} for append: {e}", path.display()))?;
            (s, w, path, true)
        }
        None => {
            let model_for_meta = model_override
                .clone()
                .unwrap_or_else(|| provider_cfg.model.clone());
            let meta = SessionMeta::new(&model_for_meta, provider.name());
            let path = session_path_for(&meta)?;
            let mut w = SessionWriter::create(&path)
                .with_context(|| format!("creating session file at {}", path.display()))?;
            w.append(&SessionEvent::Meta(meta.clone()))
                .context("writing session meta line")?;
            (Session::new(meta), w, path, false)
        }
    };

    let model = model_override.unwrap_or_else(|| session.meta.model.clone());

    hermes_core::banner::print_banner();
    eprintln!("workspace: {}", workspace_root.display());
    eprintln!(
        "session:  {} {}",
        session_path.display(),
        if resumed {
            format!("(resumed; {} prior turns)", session.messages.len())
        } else {
            "(new)".into()
        }
    );
    eprintln!("tools:    {} loaded", tools.len());
    {
        let zones = hermes_memory::group_by_zone(&active_memories);
        let zone_info: String = zones
            .iter()
            .map(|(z, m)| format!("{}:{}", z, m.len()))
            .collect::<Vec<_>>()
            .join(" ");
        if zone_info.is_empty() {
            eprintln!(
                "memory:   {} active ({} pinned)",
                active_memories.len(),
                pinned_memories.len()
            );
        } else {
            eprintln!(
                "palace:   {} memories across {} zones [{}]",
                active_memories.len(),
                zones.len(),
                zone_info
            );
        }
    }
    eprintln!("skills:   {} loaded", all_skills.len());
    eprintln!("commands: /exit /quit /clear /tokens /stats /tools /memory /skills /context /session /remember /forget /reflect /compile /palace /help");
    eprintln!();

    // Auto-compile profile on first session if memories exist but profile.md doesn't.
    if !active_memories.is_empty()
        && hermes_memory::load_profile().unwrap_or(None).is_none()
    {
        eprintln!("\x1b[90m(compiling memory profile for the first time...)\x1b[0m");
        match hermes_reflect::compile_profile(provider.as_ref(), &active_memories).await {
            Ok(profile) => match hermes_memory::save_profile(&profile) {
                Ok(p) => eprintln!("\x1b[32m✓ profile compiled ({})\x1b[0m", p.display()),
                Err(e) => eprintln!("\x1b[31m✗ profile save failed: {e}\x1b[0m"),
            },
            Err(e) => eprintln!("\x1b[31m✗ profile compile failed: {e}\x1b[0m"),
        }
    }

    let mut line_editor = ChatLineEditor::new()?;
    let mut turns_since_last_reflect: usize = 0;
    let mut stats = SessionStats {
        turn_count: 0,
        tool_calls: std::collections::HashMap::new(),
        per_turn_usage: Vec::new(),
    };

    loop {
        let input = match line_editor.readline("> ").await {
            Ok(LineOutcome::Line(l)) => l,
            Ok(LineOutcome::Interrupted) => {
                eprintln!("(^C — type /exit to quit)");
                continue;
            }
            Ok(LineOutcome::Eof) => {
                eprintln!();
                break;
            }
            Err(e) => return Err(e).context("reading prompt"),
        };

        if input.is_empty() {
            continue;
        }
        let trimmed = input.as_str();

        if let Some(cmd) = trimmed.strip_prefix('/') {
            if cmd.trim() == "reflect" {
                turns_since_last_reflect = 0;
            }
            if cmd.trim() == "stats" {
                eprintln!("--- session stats ---");
                eprintln!("turns:     {}", stats.turn_count);
                eprintln!(
                    "tokens:    input={} output={} (cumulative)",
                    session.total_input_tokens, session.total_output_tokens
                );
                if let Some(&(last_in, last_out)) = stats.per_turn_usage.last() {
                    eprintln!("last turn: input={last_in} output={last_out}");
                }
                if stats.turn_count > 0 {
                    let avg_in = session.total_input_tokens / stats.turn_count as u32;
                    let avg_out = session.total_output_tokens / stats.turn_count as u32;
                    eprintln!("avg/turn:  input={avg_in} output={avg_out}");
                }
                if !stats.tool_calls.is_empty() {
                    eprintln!("tools used:");
                    let mut sorted: Vec<_> = stats.tool_calls.iter().collect();
                    sorted.sort_by(|a, b| b.1.cmp(a.1));
                    for (name, count) in sorted {
                        eprintln!("  {name}: {count}x");
                    }
                }
                let cost = (session.total_input_tokens as f64 * 3.0
                    + session.total_output_tokens as f64 * 15.0)
                    / 1_000_000.0;
                eprintln!("cost est:  ~${cost:.4} (Sonnet reference rate)");
                continue;
            }
            if !handle_command(
                cmd,
                &mut session,
                &session_path,
                &tools,
                &all_skills,
                &active_memories,
                system.as_deref(),
                palace_index.as_deref(),
                &always_active_refs,
                &*memory_store_arc,
                &skill_store,
                provider.as_ref(),
            ).await {
                break;
            }
            continue;
        }

        // Build per-turn system prompt: base + palace/memories + skills index +
        // bodies of skills triggered by *this* user input.
        let compiled_profile = hermes_memory::load_profile().unwrap_or(None);
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
        };
        let turn_system = sources.build_turn_system(trimmed);

        // Track which skills were triggered for effectiveness stats.
        let matched_skill_names: Vec<String> = hermes_skills::match_for_query_with_effectiveness(
            &all_skills,
            trimmed,
            3,
            Some(&effectiveness),
        )
        .iter()
        .map(|s| s.frontmatter.name.clone())
        .collect();
        for name in &matched_skill_names {
            hermes_skills::record_skill_stat(hermes_skills::SkillStatEntry {
                at: chrono::Utc::now(),
                skill_name: name.clone(),
                event: hermes_skills::SkillEvent::Matched,
            });
        }

        // Track which memories were injected for effectiveness stats.
        // Skip when a compiled profile is active (no per-turn retrieval).
        let loaded_memory_ids: Vec<String> = if compiled_profile.is_none() {
            hermes_memory::search_memories_effective(
                &active_memories,
                trimmed,
                3 + pinned_memories.len(),
                Some(&mem_effectiveness),
            )
            .into_iter()
            .filter(|m| !m.frontmatter.pinned)
            .take(3)
            .map(|m| m.frontmatter.id.clone())
            .collect()
        } else {
            Vec::new()
        };
        for id in &loaded_memory_ids {
            hermes_memory::record_memory_stat(hermes_memory::MemoryStatEntry {
                at: chrono::Utc::now(),
                memory_id: id.clone(),
                event: hermes_memory::MemoryEvent::Loaded,
            });
        }

        let mut turn_msg_index = session.messages.len();
        let user_msg = session.push_user(trimmed).clone();
        if let Err(e) = writer.append(&SessionEvent::Message(user_msg)) {
            tracing::warn!(error = %e, "failed to persist user message");
        }

        // Context compaction check.
        let tools_approx = hermes_core::compaction::estimate_tokens(
            &serde_json::to_string(&tools).unwrap_or_default(),
        );
        if hermes_core::compaction::should_compact(
            &turn_system,
            &session,
            tools_approx,
            cfg.context.model_limit,
            cfg.context.headroom,
        ) {
            match hermes_core::compaction::compact_session(
                provider.as_ref(),
                &mut session,
                cfg.context.keep_recent_turns,
            )
            .await
            {
                Ok(n) => {
                    eprintln!(
                        "(context compacted: {n} messages → summary + {} recent)",
                        session.messages.len() - 1
                    );
                    turn_msg_index = session.messages.len().saturating_sub(1);
                }
                Err(e) => eprintln!("(compaction failed: {e})"),
            }
        }

        let pre_input = session.total_input_tokens;
        let pre_output = session.total_output_tokens;

        match run_one_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &model,
            &turn_system,
            provider_cfg.max_tokens,
            &workspace_root,
            &mut session,
            &mut writer,
            &cfg.permissions,
        )
        .await
        {
            Ok(()) => {
                let turn_in = session.total_input_tokens - pre_input;
                let turn_out = session.total_output_tokens - pre_output;
                stats.turn_count += 1;
                stats.per_turn_usage.push((turn_in, turn_out));
                for msg in &session.messages[turn_msg_index..] {
                    for block in &msg.content {
                        if let ContentBlock::ToolUse { name, .. } = block {
                            *stats.tool_calls.entry(name.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
            Err(e) => eprintln!("turn error: {e:#}"),
        }

        // Record SkillEvent::Used for matched skills whose body content
        // appears in the assistant's latest response.
        let assistant_text: String = session
            .messages
            .iter()
            .rev()
            .take_while(|m| matches!(m.role, Role::Assistant))
            .flat_map(|m| {
                m.content.iter().filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
            })
            .collect::<Vec<&str>>()
            .into_iter()
            .rev()
            .collect::<String>();
        if !matched_skill_names.is_empty() {
            for name in &matched_skill_names {
                if let Some(sk) = all_skills.iter().find(|s| &s.frontmatter.name == name) {
                    let body_fragments = sk.body.split_whitespace().collect::<Vec<_>>();
                    let fragment_len = body_fragments.len().min(5);
                    if fragment_len >= 3 {
                        let probe: String = body_fragments[..fragment_len].join(" ");
                        if assistant_text.contains(&probe) {
                            hermes_skills::record_skill_stat(
                                hermes_skills::SkillStatEntry {
                                    at: chrono::Utc::now(),
                                    skill_name: name.clone(),
                                    event: hermes_skills::SkillEvent::Used,
                                },
                            );
                        }
                    }
                }
            }
        }

        // Record MemoryEvent::Referenced for loaded memories whose body
        // fragment appears in the assistant's response.
        if !loaded_memory_ids.is_empty() {
            for id in &loaded_memory_ids {
                if let Some(mem) = active_memories.iter().find(|m| &m.frontmatter.id == id) {
                    let body_fragments = mem.body.split_whitespace().collect::<Vec<_>>();
                    let fragment_len = body_fragments.len().min(5);
                    if fragment_len >= 3 {
                        let probe: String = body_fragments[..fragment_len].join(" ");
                        if assistant_text.contains(&probe) {
                            hermes_memory::record_memory_stat(
                                hermes_memory::MemoryStatEntry {
                                    at: chrono::Utc::now(),
                                    memory_id: id.clone(),
                                    event: hermes_memory::MemoryEvent::Referenced,
                                },
                            );
                        }
                    }
                }
            }
        }
        // --- micro-reflection (background) ---
        let turn_messages: Vec<hermes_core::Message> =
            session.messages[turn_msg_index..].to_vec();
        if hermes_reflect::should_micro_reflect(&turn_messages, turns_since_last_reflect) {
            turns_since_last_reflect = 0;
            let prov = provider.clone();
            let ms = memory_store_arc.clone();
            let skills_snap = all_skills.clone();
            let mems_snap = active_memories.clone();
            let auto_accept = cfg.reflect.auto_accept_memories;
            let session_id = session.meta.id.clone();
            tokio::spawn(async move {
                match hermes_reflect::micro_reflect(
                    prov.as_ref(),
                    &turn_messages,
                    &skills_snap,
                    &mems_snap,
                )
                .await
                {
                    Ok(output) if !output.is_empty() => {
                        let conflict_ids: std::collections::HashSet<String> = output
                            .conflicts
                            .iter()
                            .map(|c| c.with.clone())
                            .collect();

                        let mut any_accepted = false;
                        for c in &output.memory_candidates {
                            let eligible = auto_accept
                                && matches!(c.confidence, hermes_memory::Confidence::Medium)
                                && c.supersedes.is_empty()
                                && !c.supersedes.iter().any(|id| conflict_ids.contains(id));

                            if eligible {
                                let fm = hermes_memory::MemoryFrontmatter::new(
                                    hermes_memory::Source::Reflection,
                                    c.confidence,
                                    c.tags.clone(),
                                    "general".to_string(),
                                );
                                match ms.put(c.scope, fm, &c.fact) {
                                    Ok(path) => {
                                        let preview: String = c.fact.chars().take(60).collect();
                                        eprintln!("  \x1b[32m💾 learned: {preview}\x1b[0m");
                                        tracing::info!(path=%path.display(), "auto-accepted memory");
                                        any_accepted = true;
                                    }
                                    Err(e) => {
                                        tracing::warn!(error=%e, "auto-accept memory failed");
                                    }
                                }
                                hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
                                    at: chrono::Utc::now(),
                                    session_id: session_id.clone(),
                                    kind: hermes_reflect::CandidateKind::Memory,
                                    action: hermes_reflect::ActionTaken::AutoAccept,
                                    label: c.fact.lines().next().unwrap_or("").to_string(),
                                });
                            } else {
                                hermes_reflect::deferred_save(
                                    hermes_reflect::DeferredCandidate::Memory(c.clone()),
                                );
                            }
                        }

                        for c in &output.skill_candidates {
                            hermes_reflect::deferred_save(
                                hermes_reflect::DeferredCandidate::Skill(c.clone()),
                            );
                        }

                        // Recompile profile and palace index when new memories were auto-accepted.
                        if any_accepted {
                            if let Ok(fresh_mems) = ms.list_active() {
                                match hermes_reflect::compile_profile(
                                    prov.as_ref(),
                                    &fresh_mems,
                                )
                                .await
                                {
                                    Ok(profile) => {
                                        if let Err(e) = hermes_memory::save_profile(&profile) {
                                            tracing::warn!(error=%e, "save compiled profile");
                                        } else {
                                            eprintln!("  \x1b[90m📋 profile recompiled\x1b[0m");
                                        }
                                    }
                                    Err(e) => {
                                        tracing::debug!(error=%e, "background profile compile failed");
                                    }
                                }
                                let idx = hermes_memory::build_palace_index_simple(&fresh_mems);
                                if let Err(e) = hermes_memory::save_palace_index(&idx) {
                                    tracing::warn!(error=%e, "save palace index");
                                } else {
                                    eprintln!("  \x1b[90m🏛 palace index updated\x1b[0m");
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(error=%e, "micro-reflection failed");
                    }
                }
            });
        } else {
            turns_since_last_reflect += 1;
        }

        println!();
    }

    eprintln!("session saved: {}", session_path.display());
    line_editor.save_history();

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_one_turn(
    provider: &dyn LlmProvider,
    host: &dyn ToolHost,
    tools: &[hermes_core::ToolSpec],
    model: &str,
    turn_system: &str,
    max_tokens: u32,
    _workspace: &std::path::Path,
    session: &mut Session,
    writer: &mut SessionWriter,
    permissions_cfg: &hermes_llm::PermissionsConfig,
) -> Result<()> {
    use hermes_turn::{ConfirmAction, PermissionChecker, TurnConfig, TurnEvent};
    use std::io::Write as _;

    let permissions = PermissionChecker::new(&permissions_cfg.allow, &permissions_cfg.deny);
    let config = TurnConfig {
        model: model.to_string(),
        system: if turn_system.is_empty() { None } else { Some(turn_system.to_string()) },
        max_tokens,
        max_tool_rounds: MAX_TOOL_ROUNDS,
        permissions,
    };

    let history = session.messages.clone();
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Confirmation channel: the turn loop sends ConfirmRequest, a spawned
    // task reads from confirm_rx and prompts the user on stdin.
    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);

    // Spawn a task that reads confirmation requests and prompts the user.
    let confirm_task = tokio::spawn(async move {
        let mut always_allow: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut first_prompt = true;
        while let Some(req) = confirm_rx.recv().await {
            if always_allow.contains(&req.tool_name) {
                let _ = req.reply.send(ConfirmAction::Allow);
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
                    "y" => ConfirmAction::Allow,
                    "a" => {
                        always_allow.insert(req.tool_name.clone());
                        ConfirmAction::AlwaysAllow
                    }
                    "" | "n" => ConfirmAction::Deny { reason: None },
                    other => ConfirmAction::Deny {
                        reason: Some(other.to_string()),
                    },
                }
            } else {
                ConfirmAction::Deny { reason: None }
            };
            let _ = req.reply.send(action);
        }
    });

    let text_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_started = std::sync::atomic::AtomicBool::new(false);
    let thinking_buf = std::sync::Mutex::new(String::new());
    use std::sync::atomic::Ordering::Relaxed;

    let on_event = |event: TurnEvent| {
        match event {
            TurnEvent::TextDelta(text) => {
                if thinking_started.load(Relaxed) {
                    eprint!("\r\x1b[K");
                    let mut buf = thinking_buf.lock().unwrap();
                    if !buf.is_empty() {
                        eprintln!("\x1b[90m  💭 ──────\x1b[0m");
                        for line in buf.lines() {
                            eprintln!("\x1b[90m  │ {line}\x1b[0m");
                        }
                    }
                    buf.clear();
                    thinking_started.store(false, Relaxed);
                }
                text_started.store(true, Relaxed);
                print!("{text}");
                std::io::stdout().flush().ok();
            }
            TurnEvent::ThinkingDelta(text) => {
                if !text_started.load(Relaxed) {
                    let mut buf = thinking_buf.lock().unwrap();
                    buf.push_str(&text);
                    let preview: String = buf.chars().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect();
                    let preview = preview.replace('\n', " ");
                    drop(buf);
                    eprint!("\r\x1b[K\x1b[90m  💭 {preview}\x1b[0m");
                    std::io::stderr().flush().ok();
                    thinking_started.store(true, Relaxed);
                }
            }
            TurnEvent::ToolUseStart { name, .. } => {
                if thinking_started.load(Relaxed) {
                    eprint!("\r\x1b[K");
                    let mut buf = thinking_buf.lock().unwrap();
                    if !buf.is_empty() {
                        eprintln!("\x1b[90m  💭 ──────\x1b[0m");
                        for line in buf.lines() {
                            eprintln!("\x1b[90m  │ {line}\x1b[0m");
                        }
                    }
                    buf.clear();
                    thinking_started.store(false, Relaxed);
                }
                eprint!("\x1b[33m  🔧 {name} …\x1b[0m");
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
                // The spawned confirm_task handles the actual stdin prompt.
                // This event is for frontends that render their own UI.
                tracing::debug!(tool_name, summary, "tool confirmation pending");
            }
            TurnEvent::Usage { .. } => {}
            TurnEvent::Error(msg) => {
                eprintln!("\x1b[31m  error: {msg}\x1b[0m");
            }
            TurnEvent::Done => {}
        }
    };

    let output = hermes_turn::run_turn(
        provider, host, tools, &history, &config,
        Some(confirm_tx), on_event, cancel_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Shut down the confirm task (drop the sender side already happened when
    // run_turn returned, but abort the task to be safe).
    confirm_task.abort();

    // Apply new messages to session + persist
    for msg in &output.new_messages {
        session.messages.push(msg.clone());
        if let Err(e) = writer.append(&SessionEvent::Message(msg.clone())) {
            tracing::warn!(error=%e, "persist message");
        }
    }
    session.record_usage(output.usage);
    if let Err(e) = writer.append(&SessionEvent::Usage(output.usage)) {
        tracing::warn!(error=%e, "persist usage");
    }

    Ok(())
}

#[allow(dead_code)]
fn summarise_input(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() <= 80 {
        s
    } else {
        let truncated: String = s.chars().take(80).collect();
        format!("{truncated}…")
    }
}

#[allow(dead_code)]
fn friendly_tool_desc(name: &str) -> String {
    match name {
        "read" => "📖 Reading file...".into(),
        "write" => "📝 Writing file...".into(),
        "edit" => "✏️  Editing file...".into(),
        "bash" => "💻 Running command...".into(),
        "glob" => "🔍 Searching files...".into(),
        "grep" => "🔎 Searching content...".into(),
        "web_fetch" => "🌐 Fetching web page...".into(),
        "web_search" => "🔍 Searching the web...".into(),
        "todo_add" => "📋 Adding task...".into(),
        "todo_update" => "✅ Updating task...".into(),
        "todo_list" => "📋 Listing tasks...".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}")
        }
    }
}

#[allow(dead_code)]
fn friendly_tool_result(name: &str, input: &serde_json::Value, workspace: &std::path::Path) -> String {
    let full_path = |rel: &str| -> String {
        if std::path::Path::new(rel).is_absolute() {
            rel.to_string()
        } else {
            workspace.join(rel).to_string_lossy().to_string()
        }
    };

    match name {
        "read" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let offset = input.get("offset").and_then(|o| o.as_u64());
            let limit = input.get("limit").and_then(|l| l.as_u64());
            let mut desc = format!("📖 read {}", full_path(path));
            if let Some(o) = offset {
                desc.push_str(&format!(" (from line {o}"));
                if let Some(l) = limit {
                    desc.push_str(&format!(", {} lines", l));
                }
                desc.push(')');
            }
            desc
        }
        "write" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let len = input
                .get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.len())
                .unwrap_or(0);
            format!("📝 write {} ({len} bytes)", full_path(path))
        }
        "edit" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let old: String = input
                .get("old_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            let new: String = input
                .get("new_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            format!("✏️  edit {}: \"{old}\" → \"{new}\"", full_path(path))
        }
        "bash" => {
            let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("?");
            let short: String = cmd.chars().take(120).collect();
            if cmd.chars().count() > 120 {
                format!("💻 $ {short}…")
            } else {
                format!("💻 $ {short}")
            }
        }
        "glob" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            format!("🔍 glob {}", full_path(pat))
        }
        "grep" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            let path = input.get("path").and_then(|p| p.as_str());
            match path {
                Some(p) => format!("🔎 grep /{pat}/ in {}", full_path(p)),
                None => format!("🔎 grep /{pat}/ in {}", workspace.display()),
            }
        }
        "web_search" => {
            let q = input.get("query").and_then(|q| q.as_str()).unwrap_or("?");
            format!("🌐 search \"{q}\"")
        }
        "web_fetch" => {
            let url = input.get("url").and_then(|u| u.as_str()).unwrap_or("?");
            format!("🌐 fetch {url}")
        }
        "think" => "💭 thinking…".into(),
        "todo_add" => {
            let title = input.get("title").and_then(|t| t.as_str()).unwrap_or("?");
            format!("📋 + \"{title}\"")
        }
        "todo_update" => {
            let id = input.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            let status = input.get("status").and_then(|s| s.as_str()).unwrap_or("?");
            format!("✅ todo #{id} → {status}")
        }
        "todo_list" => "📋 listing todos".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}({})", summarise_input(input))
        }
    }
}

/// Build the session system prompt: workspace clause first, then any
/// user-supplied system prompt.
pub(crate) fn compose_system_prompt(
    user_system: Option<String>,
    workspace_root: &std::path::Path,
) -> Option<String> {
    let now = chrono::Local::now();
    let clause = format!(
        "## Role\n\
         You are an expert software engineer assistant. You think step-by-step, \
         verify before claiming success, and prefer the smallest correct change. \
         When uncertain, say so. When a task is risky, warn the user.\n\n\
         ## Context\n\
         Current time: {}\n\n\
         ## Workspace\n\
         Your working directory is `{}`. All file reads, writes, and \
         commands MUST stay inside this directory. If a task seems to \
         require touching anything outside, stop and ask the user before \
         proceeding.\n\n\
         ## Tool Strategy\n\
         Explore before acting: grep/glob to locate, read to understand, \
         then edit/write to change. Verify with bash (tests, build).\n\
         - Use grep to find symbols, glob to find files by pattern.\n\
         - Use read to inspect specific lines before editing.\n\
         - Use edit for surgical changes; write only for new files.\n\
         - Use bash for builds, tests, and shell commands.\n\
         - Use git for read-only repo inspection (status, diff, log, blame).\n\
         - Use think to reason through complex multi-step plans.\n\
         Minimize tool calls: batch related reads, avoid re-reading unchanged files.\n\n\
         ## Code Analysis Workflow\n\
         1. grep/glob → locate relevant files and symbols\n\
         2. read → understand context around the target code\n\
         3. Trace callers/callees if the change has side effects\n\
         4. edit → make the minimal change\n\
         5. bash → run tests or build to verify\n\n\
         ## Memory\n\
         You have a Memory Palace with zone-organized memories. The palace \
         index (zone map) is in your system prompt when available.\n\
         - Use palace_zones to list zones\n\
         - Use palace_read_zone to load a zone's content\n\
         - Use palace_recall to search by topic\n\
         - Use memory_save (with zone parameter) to persist new learnings\n\
         - Use memory_delete to remove outdated memories\n\
         Don't guess about preferences or conventions — load the relevant \
         zone first.\n\n\
         ## Output Style\n\
         Be concise. Show file paths with line numbers when referencing code. \
         End task responses with: Changed / Verified / Not verified / Risks.",
        now.format("%Y-%m-%d %H:%M (%A)"),
        workspace_root.display()
    );
    Some(match user_system {
        Some(extra) if !extra.is_empty() => format!("{clause}\n\n{extra}"),
        _ => clause,
    })
}

#[allow(clippy::too_many_arguments)]
async fn handle_command(
    cmd: &str,
    session: &mut Session,
    path: &std::path::Path,
    tools: &[hermes_core::ToolSpec],
    skills: &[LoadedSkill],
    active_memories: &[LoadedMemory],
    base_system: Option<&str>,
    palace_index: Option<&str>,
    always_active_skills: &[&LoadedSkill],
    memory_store: &dyn MemoryStore,
    skill_store: &FsSkillStore,
    provider: &dyn LlmProvider,
) -> bool {
    match cmd.trim() {
        "exit" | "quit" => return false,
        "clear" => {
            let dropped = session.messages.len();
            session.messages.clear();
            eprintln!("(cleared {dropped} in-memory messages — JSONL transcript untouched)");
        }
        "tokens" => {
            eprintln!(
                "tokens: input={} output={} (cumulative)",
                session.total_input_tokens, session.total_output_tokens
            );
        }
        "tools" => {
            if tools.is_empty() {
                eprintln!("(no MCP tools loaded; check ~/.small-rust-hermes/mcp.json)");
            } else {
                for t in tools {
                    eprintln!(
                        "  {} — {}",
                        t.name,
                        t.description.lines().next().unwrap_or("")
                    );
                }
            }
        }
        "memory" | "memories" => {
            if active_memories.is_empty() {
                eprintln!("(no active memories)");
            } else {
                for m in active_memories {
                    let pin = if m.frontmatter.pinned { "★ " } else { "  " };
                    let line = m.body.lines().next().unwrap_or("").trim();
                    eprintln!("{pin}{} [{:?}]: {}", m.frontmatter.id, m.scope, line);
                }
            }
        }
        "skills" => {
            if skills.is_empty() {
                eprintln!("(no skills loaded)");
            } else {
                for s in skills {
                    eprintln!(
                        "  {}: {}",
                        s.frontmatter.name, s.frontmatter.description
                    );
                }
            }
        }
        "context" => {
            // Show the *current* assembled session-level system prompt so
            // the user can inspect what gets sent to the LLM.
            let pinned: Vec<_> = active_memories
                .iter()
                .filter(|m| m.frontmatter.pinned)
                .cloned()
                .collect();
            let ctx_profile = hermes_memory::load_profile().unwrap_or(None);
            let sources = ContextSources {
                base: base_system,
                palace_index,
                compiled_profile: ctx_profile.as_deref(),
                always_active_skills,
                pinned: &pinned,
                active: active_memories,
                all_skills: skills,
                effectiveness: None,
                memory_effectiveness: None,
            };
            let s = sources.build_session_system();
            if s.is_empty() {
                eprintln!("(empty system prompt — no base, no memory, no skills)");
            } else {
                eprintln!("--- session-level system prompt ---");
                eprintln!("{s}");
                eprintln!("--- end (skills triggered per turn are appended dynamically) ---");
            }
        }
        "session" => {
            eprintln!("path:     {}", path.display());
            eprintln!("messages: {}", session.messages.len());
        }
        s if s.starts_with("remember ") => {
            let text = s.strip_prefix("remember ").unwrap().trim();
            if text.is_empty() {
                eprintln!("usage: /remember <text>");
            } else {
                use hermes_memory::{Confidence, MemoryFrontmatter, Scope as MemScope, Source as MemSource};
                let mut fm = MemoryFrontmatter::new(MemSource::User, Confidence::High, vec![], "core".to_string());
                fm.pinned = true;
                match memory_store.put(MemScope::User, fm, text) {
                    Ok(p) => eprintln!("\x1b[32m✓\x1b[0m remembered: {} ({})", text.chars().take(60).collect::<String>(), p.display()),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
                }
            }
        }
        s if s.starts_with("forget ") => {
            let id_prefix = s.strip_prefix("forget ").unwrap().trim();
            if id_prefix.is_empty() {
                eprintln!("usage: /forget <id-prefix>");
            } else {
                let matches: Vec<_> = active_memories
                    .iter()
                    .filter(|m| m.frontmatter.id.starts_with(id_prefix))
                    .collect();
                match matches.len() {
                    0 => eprintln!("no memory matching \"{id_prefix}\""),
                    1 => {
                        let m = matches[0];
                        eprint!("delete \"{}\"? [y/N] ▸ ", m.body.lines().next().unwrap_or(""));
                        std::io::stderr().flush().ok();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_ok() && input.trim() == "y" {
                            match memory_store.delete(m.scope, &m.frontmatter.id) {
                                Ok(true) => eprintln!("\x1b[32m✓\x1b[0m forgotten"),
                                Ok(false) => eprintln!("\x1b[31m✗\x1b[0m not found on disk"),
                                Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
                            }
                        } else {
                            eprintln!("(cancelled)");
                        }
                    }
                    n => {
                        eprintln!("{n} memories match \"{id_prefix}\":");
                        for m in &matches {
                            eprintln!("  {} — {}", m.frontmatter.id, m.body.lines().next().unwrap_or(""));
                        }
                        eprintln!("use a longer prefix to disambiguate");
                    }
                }
            }
        }
        "reflect" => {
            if session.messages.is_empty() {
                eprintln!("(nothing to reflect on yet — send a message first)");
            } else {
                eprintln!("\x1b[90m(reflecting on session so far...)\x1b[0m");
                if let Err(e) = super::reflect::run_with_min_turns(provider, session, 0).await {
                    eprintln!("\x1b[31m(reflection failed: {e:#})\x1b[0m");
                }
            }
        }
        "compile" => {
            if active_memories.is_empty() {
                eprintln!("(no memories to compile)");
            } else {
                eprint!("\x1b[90m(compiling memory profile...)\x1b[0m");
                std::io::stderr().flush().ok();
                match hermes_reflect::compile_profile(provider, active_memories).await {
                    Ok(profile) => match hermes_memory::save_profile(&profile) {
                        Ok(p) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[32m✓ profile updated ({})\x1b[0m", p.display());
                        }
                        Err(e) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[31m✗ save failed: {e}\x1b[0m");
                        }
                    },
                    Err(e) => {
                        eprint!("\r\x1b[K");
                        eprintln!("\x1b[31m✗ compile failed: {e}\x1b[0m");
                    }
                }
            }
        }
        s if s.starts_with("skill add") => {
            let rest = s.strip_prefix("skill add").unwrap().trim();
            handle_skill_add(rest, skill_store);
        }
        s if s.starts_with("skill edit ") => {
            let name = s.strip_prefix("skill edit ").unwrap().trim();
            handle_skill_edit(name, skill_store);
        }
        s if s.starts_with("skill show ") => {
            let name = s.strip_prefix("skill show ").unwrap().trim();
            handle_skill_show(name, skill_store);
        }
        "skill" => {
            // /skill alone is same as /skills
            if skills.is_empty() {
                eprintln!("(no skills loaded)");
            } else {
                for s in skills {
                    eprintln!("  {}: {}", s.frontmatter.name, s.frontmatter.description);
                }
            }
        }
        "palace" => {
            let zones = hermes_memory::group_by_zone(active_memories);
            eprintln!("Memory Palace: {} memories across {} zones", active_memories.len(), zones.len());
            for (zone, mems) in &zones {
                eprintln!("  {zone}: {} memories", mems.len());
            }
            if palace_index.is_some() {
                eprintln!("  index: loaded (in system prompt)");
            } else {
                eprintln!("  index: not loaded");
            }
        }
        "palace compile" => {
            if active_memories.is_empty() {
                eprintln!("(no memories to compile)");
            } else {
                eprint!("\x1b[90m(compiling palace index via LLM...)\x1b[0m");
                std::io::stderr().flush().ok();
                match hermes_reflect::compile_palace_index(provider, active_memories).await {
                    Ok(index) => match hermes_memory::save_palace_index(&index) {
                        Ok(p) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[32m✓ palace index compiled ({})\x1b[0m", p.display());
                            eprintln!("(restart chat to use the new index)");
                        }
                        Err(e) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[31m✗ save failed: {e}\x1b[0m");
                        }
                    },
                    Err(e) => {
                        eprint!("\r\x1b[K");
                        eprintln!("\x1b[31m✗ compile failed: {e}\x1b[0m");
                    }
                }
            }
        }
        "help" => {
            eprintln!("commands:");
            eprintln!("  /exit, /quit   — leave the chat");
            eprintln!("  /clear         — drop in-memory history (transcript on disk kept)");
            eprintln!("  /tokens        — cumulative input/output token counts");
            eprintln!("  /stats         — detailed session stats: turns, tools, cost estimate");
            eprintln!("  /tools         — list tools loaded from MCP servers");
            eprintln!("  /memory        — list active memories with ids");
            eprintln!("  /skills        — list available skills");
            eprintln!("  /skill add     — interactively add a new skill");
            eprintln!("  /skill edit <name> — edit an existing skill in $EDITOR");
            eprintln!("  /skill show <name> — display full skill body");
            eprintln!("  /context       — show the assembled session-level system prompt");
            eprintln!("  /session       — show transcript path and turn count");
            eprintln!("  /remember <text> — save a memory (pinned, high confidence)");
            eprintln!("  /forget <id>   — delete a memory by id prefix (with confirmation)");
            eprintln!("  /reflect       — trigger on-demand reflection");
            eprintln!("  /compile       — recompile memory profile");
            eprintln!("  /palace        — show Memory Palace zone counts");
            eprintln!("  /palace compile — LLM-compile the palace index");
            eprintln!("  /help          — this list");
        }
        other => eprintln!("unknown command: /{other}  (try /help)"),
    }
    true
}

fn handle_skill_add(rest: &str, skill_store: &FsSkillStore) {
    use hermes_skills::{Scope as SkScope, SkillFrontmatter, SkillStore as _};

    // Parse: /skill add <name> <description> or just /skill add for interactive
    let (name, description) = if rest.is_empty() {
        // Interactive mode
        eprint!("skill name: ");
        std::io::stderr().flush().ok();
        let mut name = String::new();
        if std::io::stdin().read_line(&mut name).is_err() || name.trim().is_empty() {
            eprintln!("(cancelled)");
            return;
        }
        let name = name.trim().to_string();

        eprint!("description: ");
        std::io::stderr().flush().ok();
        let mut desc = String::new();
        if std::io::stdin().read_line(&mut desc).is_err() {
            eprintln!("(cancelled)");
            return;
        }
        (name, desc.trim().to_string())
    } else {
        // Inline: /skill add <name> <description>
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 || parts[1].trim().is_empty() {
            eprintln!("usage: /skill add <name> <description>");
            return;
        }
        (parts[0].trim().to_string(), parts[1].trim().to_string())
    };

    // Validate name
    if name.contains('/') || name.contains("..") || name.contains('\\') {
        eprintln!("\x1b[31m✗\x1b[0m invalid skill name (no path separators or '..')");
        return;
    }

    eprint!("triggers (comma-separated): ");
    std::io::stderr().flush().ok();
    let mut triggers_input = String::new();
    if std::io::stdin().read_line(&mut triggers_input).is_err() {
        eprintln!("(cancelled)");
        return;
    }
    let triggers: Vec<String> = triggers_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Open $EDITOR for body
    let body = match edit_in_editor(&format!(
        "# {}\n\n{}\n",
        name,
        "Describe the skill procedure here in markdown."
    )) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m editor failed: {e}");
            return;
        }
    };
    if body.trim().is_empty() {
        eprintln!("(cancelled — empty body)");
        return;
    }

    let fm = SkillFrontmatter {
        name: name.clone(),
        description,
        triggers,
        version: Some("0.1.0".into()),
        license: None,
        always_active: false,
        extra: serde_yaml::Mapping::new(),
    };
    match skill_store.put(SkScope::User, fm, &body) {
        Ok(p) => eprintln!("\x1b[32m✓\x1b[0m skill \"{name}\" saved: {}", p.display()),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

fn handle_skill_edit(name: &str, skill_store: &FsSkillStore) {
    use hermes_skills::SkillStore as _;

    if name.is_empty() {
        eprintln!("usage: /skill edit <name>");
        return;
    }
    let existing = match skill_store.get(name) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("skill \"{name}\" not found");
            return;
        }
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m {e}");
            return;
        }
    };

    let body = match edit_in_editor(&existing.body) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m editor failed: {e}");
            return;
        }
    };
    if body.trim().is_empty() {
        eprintln!("(cancelled — empty body)");
        return;
    }

    let scope = existing.scope;
    let fm = existing.frontmatter;
    match skill_store.put(scope, fm, &body) {
        Ok(p) => eprintln!("\x1b[32m✓\x1b[0m skill \"{name}\" updated: {}", p.display()),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

fn handle_skill_show(name: &str, skill_store: &FsSkillStore) {
    use hermes_skills::SkillStore as _;

    if name.is_empty() {
        eprintln!("usage: /skill show <name>");
        return;
    }
    match skill_store.get(name) {
        Ok(Some(s)) => {
            eprintln!("name:        {}", s.frontmatter.name);
            eprintln!("description: {}", s.frontmatter.description);
            eprintln!("triggers:    {}", s.frontmatter.triggers.join(", "));
            eprintln!("scope:       {:?}", s.scope);
            eprintln!();
            eprintln!("{}", s.body.trim());
        }
        Ok(None) => eprintln!("skill \"{name}\" not found"),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

pub(crate) fn auto_install_palace_skill(skill_store: &FsSkillStore) {
    use hermes_skills::{Scope as SkScope, SkillFrontmatter, SkillStore as _};

    if let Ok(Some(_)) = skill_store.get("memory-palace") {
        return;
    }
    let fm = SkillFrontmatter {
        name: "memory-palace".to_string(),
        description: "Protocol for navigating the Memory Palace".to_string(),
        triggers: vec![],
        version: Some("0.1.0".into()),
        license: None,
        always_active: true,
        extra: serde_yaml::Mapping::new(),
    };
    let body = r#"# Memory Palace Protocol

Your memories are organized into zones. The palace index (zone map) is in your system prompt.

## Zones
- core — stable user identity, preferences, principles
- work — current focus, recent activity
- project:<name> — per-project conventions
- episode — session summaries
- general — uncategorized (default)

## Navigation
1. Check the palace index to see what zones exist
2. Use palace_read_zone to load a specific zone's content
3. Use palace_recall to search by topic (optionally scoped to a zone)
4. Don't guess — load the zone before answering questions about preferences or conventions

## Saving
When using memory_save, set the zone parameter:
- User preferences, identity → core
- Current tasks, recent decisions → work
- Project-specific → project:<name>
- Everything else → general
"#;
    match skill_store.put(SkScope::User, fm, body) {
        Ok(p) => tracing::info!(path=%p.display(), "auto-installed memory-palace skill"),
        Err(e) => tracing::warn!(error=%e, "failed to auto-install memory-palace skill"),
    }
}

/// Open $EDITOR (or vi) with initial content, return the edited content.
fn edit_in_editor(initial_content: &str) -> Result<String> {
    use std::io::Read as _;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let mut tmp = tempfile::Builder::new()
        .prefix("hermes-skill-")
        .suffix(".md")
        .tempfile()
        .context("creating temp file for editor")?;
    write!(tmp, "{initial_content}")?;
    let tmp_path = tmp.path().to_owned();

    let status = std::process::Command::new(&editor)
        .arg(&tmp_path)
        .status()
        .context(format!("running editor '{editor}'"))?;

    if !status.success() {
        anyhow::bail!("editor exited with status {status}");
    }

    let mut content = String::new();
    let mut f = std::fs::File::open(&tmp_path).context("re-opening temp file")?;
    f.read_to_string(&mut content)?;
    Ok(content)
}

