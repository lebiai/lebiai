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

mod commands;
mod turn;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{anyhow, Context, Result};
use hermes_channel::{ServeCtx, CHAT_TOOL_WHITELIST};
use hermes_core::{ContentBlock, Role, Session, SessionEvent, SessionMeta, ToolSpec};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryEffectiveness, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillEffectiveness, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::{ProposeContext, SubagentContext};
use hermes_turn::{PermissionChecker, TurnConfig};

use super::context::ContextSources;
use super::readline::{ChatLineEditor, LineOutcome};
use super::style;
use super::util::{build_active_provider, build_web_ctx, load_tool_host, session_path_for};

pub(crate) use hermes_channel::system_prompt::{compose_system_prompt, inject_time_header};

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
    let cfg = super::util::load_config_or_hint()?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let memory_store_arc: Arc<dyn MemoryStore> =
        Arc::new(FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("memory store: {e}"))?);
    let skill_store_arc: Arc<FsSkillStore> =
        Arc::new(FsSkillStore::standard().map_err(|e| anyhow::anyhow!("skill store: {e}"))?);
    hermes_skills::bundled::auto_install_bundled(skill_store_arc.as_ref());

    // Wire up propose_skill: a shared message snapshot (kept in sync by the
    // chat loop) + a queue the tool pushes candidates onto. The chat loop
    // drains the queue after each turn and runs the interactive approval.
    let propose_messages: Arc<RwLock<Vec<hermes_core::Message>>> =
        Arc::new(RwLock::new(Vec::new()));
    let propose_queue: Arc<Mutex<Vec<hermes_reflect::SkillCandidate>>> =
        Arc::new(Mutex::new(Vec::new()));
    let propose_ctx = Arc::new(ProposeContext {
        provider: provider.clone(),
        messages: propose_messages.clone(),
        queue: propose_queue.clone(),
    });

    // Wire up `subagent`: lets the agent spawn fresh child contexts (used by
    // skill-creator's eval flow — each test prompt runs in a clean context so
    // the parent's reasoning doesn't leak into the grade).
    let subagent_ctx = Arc::new(SubagentContext::new(
        provider.clone(),
        provider_cfg.model.clone(),
        provider_cfg.max_tokens,
        cfg.limits.max_tool_rounds,
        hermes_turn::PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
        workspace_root.clone(),
        Some(memory_store_arc.clone()),
        Some(skill_store_arc.clone() as Arc<dyn hermes_skills::SkillStore>),
    ));

    let host = load_tool_host(
        &workspace_root,
        Some(memory_store_arc.clone()),
        Some(skill_store_arc.clone() as Arc<dyn hermes_skills::SkillStore>),
        Some(propose_ctx),
        Some(subagent_ctx),
        Some(build_web_ctx(&cfg, provider.clone())),
    )
    .await?;
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

    let all_skills: Vec<LoadedSkill> = skill_store_arc
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
            let w = SessionWriter::open_append(&path).map_err(|e| {
                anyhow::anyhow!("opening session {} for append: {e}", path.display())
            })?;
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
    if !active_memories.is_empty() && hermes_memory::load_profile().unwrap_or(None).is_none() {
        eprintln!(
            "{}",
            style::dim("(compiling memory profile for the first time...)")
        );
        match hermes_reflect::compile_profile(provider.as_ref(), &active_memories).await {
            Ok(profile) => match hermes_memory::save_profile(&profile) {
                Ok(p) => eprintln!(
                    "{}",
                    style::green(&format!("✓ profile compiled ({})", p.display()))
                ),
                Err(e) => eprintln!("{}", style::red(&format!("✗ profile save failed: {e}"))),
            },
            Err(e) => eprintln!("{}", style::red(&format!("✗ profile compile failed: {e}"))),
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
                print_stats(&session, &stats);
                continue;
            }
            if !commands::handle_command(
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
                skill_store_arc.as_ref(),
                provider.as_ref(),
                cfg.limits,
            )
            .await
            {
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
            limits: cfg.limits,
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

        // Sync the snapshot the propose_skill tool reads from. Must happen
        // before the turn runs because the tool may fire mid-turn.
        if let Ok(mut guard) = propose_messages.write() {
            *guard = session.messages.clone();
        }

        match turn::run_one_turn(
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
            cfg.limits.max_tool_rounds,
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

        // Drain any skill candidates the `propose_skill` tool queued during
        // the turn, then run the interactive approval. Reuses the same UI
        // path as `/reflect` so the approval gate stays uniform.
        let proposed: Vec<hermes_reflect::SkillCandidate> = {
            match propose_queue.lock() {
                Ok(mut q) => q.drain(..).collect(),
                Err(_) => Vec::new(),
            }
        };
        for c in &proposed {
            if let Err(e) =
                super::reflect::review_proposed_skill(c, &session.meta.id, skill_store_arc.as_ref())
                    .await
            {
                tracing::warn!(error=%e, "review proposed skill failed");
            }
        }

        // Record SkillEvent::Used / MemoryEvent::Referenced when the turn
        // produced non-trivial output (any tool call, or assistant text long
        // enough to count as a substantive reply). Prior versions tried to
        // detect verbatim echoes of skill/memory bodies — that produced false
        // negatives whenever the LLM paraphrased, which is most of the time.
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
        let turn_had_tool_use = session.messages[turn_msg_index..]
            .iter()
            .flat_map(|m| m.content.iter())
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
        let turn_was_substantive = turn_had_tool_use || assistant_text.trim().chars().count() >= 40;

        if turn_was_substantive && !matched_skill_names.is_empty() {
            for name in &matched_skill_names {
                hermes_skills::record_skill_stat(hermes_skills::SkillStatEntry {
                    at: chrono::Utc::now(),
                    skill_name: name.clone(),
                    event: hermes_skills::SkillEvent::Used,
                });
            }
        }

        if turn_was_substantive && !loaded_memory_ids.is_empty() {
            for id in &loaded_memory_ids {
                hermes_memory::record_memory_stat(hermes_memory::MemoryStatEntry {
                    at: chrono::Utc::now(),
                    memory_id: id.clone(),
                    event: hermes_memory::MemoryEvent::Referenced,
                });
            }
        }
        // --- micro-reflection (background; shared pipeline) ---
        let turn_messages: Vec<hermes_core::Message> = session.messages[turn_msg_index..].to_vec();
        {
            let prov = provider.clone();
            let ms = memory_store_arc.clone();
            let skills_snap = all_skills.clone();
            let mems_snap = active_memories.clone();
            let auto_accept = cfg.reflect.auto_accept_memories;
            let min_confidence: hermes_memory::Confidence = cfg
                .reflect
                .auto_accept_min_confidence
                .parse()
                .unwrap_or(hermes_memory::Confidence::Medium);
            let session_id = session.meta.id.clone();
            let turns_since = turns_since_last_reflect;
            tokio::spawn(async move {
                let apply = hermes_reflect::MicroApplyConfig::new(
                    session_id,
                    auto_accept,
                    min_confidence,
                    false,
                );
                let outcome =
                    hermes_reflect::run_micro_after_turn(hermes_reflect::MicroRunRequest {
                        provider: prov.as_ref(),
                        store: ms.as_ref(),
                        turn_messages: &turn_messages,
                        skills: &skills_snap,
                        memories: &mems_snap,
                        turns_since_last: turns_since,
                        apply,
                        recompile_on_auto_accept: true,
                    })
                    .await;
                match outcome {
                    Ok(hermes_reflect::MicroRunOutcome::Applied(applied)) => {
                        for _ in 0..applied.skipped_near_duplicates {
                            eprintln!("{}", style::dim("  ↺ skipped (near-duplicate memory)"));
                        }
                        if applied.auto_accepted > 0 {
                            if let Ok(fresh) = ms.list_active() {
                                if let Some(last) = fresh.last() {
                                    let preview: String = last.body.chars().take(60).collect();
                                    eprintln!(
                                        "{}",
                                        style::green(&format!("  💾 learned: {preview}"))
                                    );
                                }
                            }
                            eprintln!("{}", style::dim("  📋 profile / palace refreshed"));
                        }
                        if applied.has_pending() {
                            eprintln!(
                                "{}",
                                style::dim(&format!(
                                    "  🪞 micro-reflect: {} memory / {} skill pending review",
                                    applied.pending_memory_count(),
                                    applied.pending_skill_count()
                                ))
                            );
                        }
                    }
                    Ok(hermes_reflect::MicroRunOutcome::Empty) => {}
                    Ok(hermes_reflect::MicroRunOutcome::Skipped) => {}
                    Err(e) => {
                        tracing::debug!(error=%e, "micro-reflection failed");
                    }
                }
            });
            // Cooldown is updated on the main loop using the same gate
            // (should_micro_reflect) so we stay in sync with the spawn decision.
            if hermes_reflect::should_micro_reflect(
                &session.messages[turn_msg_index..],
                turns_since_last_reflect,
            ) {
                turns_since_last_reflect = 0;
            } else {
                turns_since_last_reflect += 1;
            }
        }

        println!();
    }

    // Quit-driven full reflection (P0 第一条): run full reflection when the
    // session ends. Skipped silently below `reflect.min_turns`; the explicit
    // `/reflect` command always runs regardless. Deferred candidates from
    // micro-reflection surface through the same approval gate. A failure here
    // must never block session save / exit.
    if let Err(e) =
        super::reflect::run_with_min_turns(provider.as_ref(), &session, cfg.reflect.min_turns).await
    {
        tracing::warn!(error=%e, "end-of-session reflection failed");
    }

    eprintln!("session saved: {}", session_path.display());
    line_editor.save_history();

    Ok(())
}

fn print_stats(session: &Session, stats: &SessionStats) {
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
}

/// Build the shared channel serve context from the default config (CLI
/// wiring: provider, subagent-capable tool host, stores, whitelisted
/// tools). Used by `wechat run` / `feishu run` / `telegram run`.
pub(crate) async fn build_channel_ctx() -> Result<Arc<ServeCtx>> {
    let cfg = Config::load_default().context("loading config from ~/.lebi-ai/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;
    let provider_name = provider.name().to_string();
    let model = provider_cfg.model.clone();
    let workspace_root = cfg.workspace.root.clone();

    let memory_store_arc: Arc<dyn MemoryStore> =
        Arc::new(FsMemoryStore::standard().map_err(|e| anyhow!("memory store: {e}"))?);
    let skill_store_arc: Arc<FsSkillStore> =
        Arc::new(FsSkillStore::standard().map_err(|e| anyhow!("skill store: {e}"))?);
    hermes_skills::bundled::auto_install_bundled(skill_store_arc.as_ref());

    let subagent_ctx = Arc::new(SubagentContext::new(
        provider.clone(),
        provider_cfg.model.clone(),
        provider_cfg.max_tokens,
        cfg.limits.max_tool_rounds,
        PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
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
        Some(build_web_ctx(&cfg, provider.clone())),
    )
    .await?;
    let all_tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow!("listing tools: {e}"))?;
    let tools: Vec<ToolSpec> = all_tools
        .into_iter()
        .filter(|t| CHAT_TOOL_WHITELIST.contains(&t.name.as_str()))
        .collect();
    eprintln!("✓ tools ready: {} whitelisted", tools.len());

    let active_memories: Vec<LoadedMemory> = memory_store_arc
        .list_active()
        .map_err(|e| anyhow!("listing memories: {e}"))?;
    let pinned_memories: Vec<LoadedMemory> = active_memories
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();
    let all_skills: Vec<LoadedSkill> = skill_store_arc
        .list()
        .map_err(|e| anyhow!("listing skills: {e}"))?;
    let always_active_skills: Vec<LoadedSkill> = all_skills
        .iter()
        .filter(|s| s.frontmatter.always_active)
        .cloned()
        .collect();
    let skill_effectiveness: HashMap<String, SkillEffectiveness> =
        hermes_skills::load_effectiveness().unwrap_or_default();
    let memory_effectiveness: HashMap<String, MemoryEffectiveness> =
        hermes_memory::load_effectiveness().unwrap_or_default();

    let palace_index: Option<String> = if active_memories.is_empty() {
        None
    } else {
        match hermes_memory::load_palace_index() {
            Ok(Some(idx)) => Some(idx),
            _ => Some(hermes_memory::build_palace_index_simple(&active_memories)),
        }
    };
    let compiled_profile: Option<String> = hermes_memory::load_profile().unwrap_or(None);

    eprintln!(
        "memory:   {} active ({} pinned) · profile {}",
        active_memories.len(),
        pinned_memories.len(),
        if compiled_profile.is_some() {
            "✓"
        } else {
            "—"
        },
    );
    eprintln!("skills:   {} loaded", all_skills.len());

    let base_system = compose_system_prompt(None, &workspace_root);
    let base_turn_cfg = TurnConfig {
        model: model.clone(),
        system: None,
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
    };

    Ok(Arc::new(ServeCtx {
        provider,
        host,
        tools,
        base_turn_cfg,
        model,
        provider_name,
        base_system,
        palace_index,
        compiled_profile,
        always_active_skills,
        pinned_memories,
        active_memories,
        all_skills,
        skill_effectiveness,
        memory_effectiveness,
        limits: cfg.limits,
    }))
}
