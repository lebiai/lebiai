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
mod system_prompt;
mod turn;

use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result};
use hermes_core::{
    ContentBlock, Role, Session, SessionEvent, SessionMeta,
};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::{ProposeContext, SubagentContext};

use super::context::ContextSources;
use super::readline::{ChatLineEditor, LineOutcome};
use super::util::{build_active_provider, load_tool_host, session_path_for};

pub(crate) use system_prompt::{compose_system_prompt, inject_time_header};

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
    let skill_store_arc: Arc<FsSkillStore> = Arc::new(
        FsSkillStore::standard().map_err(|e| anyhow::anyhow!("skill store: {e}"))?,
    );
    auto_install_palace_skill(skill_store_arc.as_ref());
    auto_install_skill_creator_skill(skill_store_arc.as_ref());
    auto_install_find_skills_skill(skill_store_arc.as_ref());

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
            if let Err(e) = super::reflect::review_proposed_skill(
                c,
                &session.meta.id,
                skill_store_arc.as_ref(),
            )
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
        let turn_was_substantive =
            turn_had_tool_use || assistant_text.trim().chars().count() >= 40;

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

/// Auto-install the bundled `skill-creator` meta-skill — a guide that
/// teaches the agent how to author new skills (single-file or multi-file
/// with `scripts/` / `references/` / `assets/` / `agents/`), how to write
/// test cases, how to spawn `subagent` evals, and how to iterate.
///
/// The bundle ships **six files**: SKILL.md plus three agents/ prompts
/// (grader / comparator / analyzer), references/schemas.md, and the
/// upstream Apache 2.0 LICENSE.txt. The skill body links to each via
/// `skill_read_file` (Progressive Disclosure level 3) — they stay on disk
/// until the agent actually needs them, so context cost is just the
/// SKILL.md body during activation.
///
/// Runs at every chat / agent / wechat startup. **Upgrade-aware**: an
/// older install that only has SKILL.md (no `agents/grader.md`) is
/// considered stale and re-installed in full. This makes the upgrade
/// from the abridged single-file version automatic — users don't need
/// to `rm -rf` anything.
pub(crate) fn auto_install_skill_creator_skill(skill_store: &FsSkillStore) {
    use hermes_skills::{Scope as SkScope, SkillStore as _};

    // Upgrade detection: skip only when both SKILL.md is registered
    // AND the multi-file bundle marker (`agents/grader.md`) is on disk.
    // If either is missing, re-install the full bundle.
    let already_full = matches!(skill_store.get("skill-creator"), Ok(Some(_)))
        && matches!(
            skill_store.skill_dir(SkScope::User, "skill-creator"),
            Ok(d) if d.join("agents").join("grader.md").is_file()
        );
    if already_full {
        return;
    }

    let raw = include_str!("../../skills/skill-creator/SKILL.md");
    install_bundled_skill(skill_store, "skill-creator", raw);

    // SKILL.md is in place — now lay down the bundled subfiles next to
    // it. Paths are compile-time constants (`include_str!`), so no path
    // validation is needed here; the trust boundary is the source tree.
    let extra_files: &[(&str, &str)] = &[
        (
            "agents/grader.md",
            include_str!("../../skills/skill-creator/agents/grader.md"),
        ),
        (
            "agents/comparator.md",
            include_str!("../../skills/skill-creator/agents/comparator.md"),
        ),
        (
            "agents/analyzer.md",
            include_str!("../../skills/skill-creator/agents/analyzer.md"),
        ),
        (
            "references/schemas.md",
            include_str!("../../skills/skill-creator/references/schemas.md"),
        ),
        (
            "LICENSE.txt",
            include_str!("../../skills/skill-creator/LICENSE.txt"),
        ),
    ];
    write_bundled_subfiles(skill_store, "skill-creator", extra_files);
}

/// Auto-install the bundled `find-skills` meta-skill — teaches the agent
/// how to search skills.sh, vet candidates by install count / source
/// reputation, present the SKILL.md for review, and call `skill_install`
/// on confirmation.
pub(crate) fn auto_install_find_skills_skill(skill_store: &FsSkillStore) {
    use hermes_skills::SkillStore as _;
    if let Ok(Some(_)) = skill_store.get("find-skills") {
        return;
    }
    let raw = include_str!("../../skills/find-skills/SKILL.md");
    install_bundled_skill(skill_store, "find-skills", raw);
}

/// Shared inner: parse a bundled SKILL.md and write it through the store.
/// Bundled skills are user-scoped; failure is logged and swallowed so a
/// malformed bundle never blocks the CLI from starting.
fn install_bundled_skill(skill_store: &FsSkillStore, name: &str, raw: &str) {
    use hermes_skills::{Scope as SkScope, SkillStore as _};
    let (fm, body) = match hermes_skills::parse_skill_doc(raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(skill=%name, error=%e, "bundled SKILL.md failed to parse");
            return;
        }
    };
    match skill_store.put(SkScope::User, fm, &body) {
        Ok(p) => tracing::info!(skill=%name, path=%p.display(), "auto-installed bundled skill"),
        Err(e) => tracing::warn!(skill=%name, error=%e, "failed to auto-install bundled skill"),
    }
}

/// Write extra files alongside a bundled skill's SKILL.md (the level-3
/// Progressive Disclosure payload — `scripts/` / `references/` /
/// `assets/` / `agents/`). Pass `subfiles` as `(rel_path, content)`
/// pairs. Creates intermediate directories as needed.
///
/// Trust model: `rel_path` is a compile-time constant rooted at the
/// bundle source tree (via `include_str!`), so no path validation here.
/// Failures are logged and swallowed for the same reason as
/// [`install_bundled_skill`] — a broken bundle should not block startup.
fn write_bundled_subfiles(
    skill_store: &FsSkillStore,
    name: &str,
    subfiles: &[(&str, &str)],
) {
    use hermes_skills::{Scope as SkScope, SkillStore as _};
    let base = match skill_store.skill_dir(SkScope::User, name) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(skill=%name, error=%e, "resolving bundled skill dir failed");
            return;
        }
    };
    for (rel, content) in subfiles {
        let target = base.join(rel);
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(skill=%name, rel=%rel, error=%e, "create subfile dir failed");
                continue;
            }
        }
        if let Err(e) = std::fs::write(&target, content) {
            tracing::warn!(skill=%name, rel=%rel, error=%e, "write bundled subfile failed");
        }
    }
}

#[cfg(test)]
mod bundled_skill_tests {
    //! Compile-time guards for the bundled meta-skills. If their SKILL.md
    //! frontmatter regresses (missing field, bad YAML, etc.) the test fails
    //! at `cargo test`, long before users see a broken cold start.

    #[test]
    fn skill_creator_bundle_parses_and_is_not_always_active() {
        let raw = include_str!("../../skills/skill-creator/SKILL.md");
        let (fm, body) = hermes_skills::parse_skill_doc(raw)
            .expect("bundled skill-creator SKILL.md must parse");
        assert_eq!(fm.name, "skill-creator");
        assert!(!fm.description.is_empty());
        assert!(!fm.always_active, "skill-creator should not be always_active");
        assert!(body.contains("Skill Creator"));
        assert!(
            body.contains("agents/grader.md"),
            "skill-creator body must link to its bundled grader prompt"
        );
    }

    #[test]
    fn find_skills_bundle_parses_and_is_not_always_active() {
        let raw = include_str!("../../skills/find-skills/SKILL.md");
        let (fm, body) = hermes_skills::parse_skill_doc(raw)
            .expect("bundled find-skills SKILL.md must parse");
        assert_eq!(fm.name, "find-skills");
        assert!(!fm.description.is_empty());
        assert!(!fm.always_active, "find-skills should not be always_active");
        assert!(body.contains("Finding and Installing Skills"));
    }

    #[test]
    fn auto_install_writes_both_bundled_skills_into_a_fresh_store() {
        use hermes_skills::{FsSkillStore, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        // Bypass FsSkillStore::standard()-based functions; reach into the
        // shared installer directly so the test doesn't touch $HOME.
        let raw_creator = include_str!("../../skills/skill-creator/SKILL.md");
        let raw_finder = include_str!("../../skills/find-skills/SKILL.md");

        let (fm_c, body_c) = hermes_skills::parse_skill_doc(raw_creator).unwrap();
        store
            .put(hermes_skills::Scope::User, fm_c, &body_c)
            .unwrap();

        let (fm_f, body_f) = hermes_skills::parse_skill_doc(raw_finder).unwrap();
        store
            .put(hermes_skills::Scope::User, fm_f, &body_f)
            .unwrap();

        assert!(store.get("skill-creator").unwrap().is_some());
        assert!(store.get("find-skills").unwrap().is_some());
    }

    #[test]
    fn auto_install_skill_creator_writes_full_multi_file_bundle() {
        use hermes_skills::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        super::auto_install_skill_creator_skill(&store);

        // SKILL.md registered through the store.
        assert!(store.get("skill-creator").unwrap().is_some());

        // All five bundled subfiles landed on disk next to SKILL.md.
        let dir = store
            .skill_dir(Scope::User, "skill-creator")
            .expect("skill_dir for skill-creator");
        for rel in [
            "agents/grader.md",
            "agents/comparator.md",
            "agents/analyzer.md",
            "references/schemas.md",
            "LICENSE.txt",
        ] {
            let p = dir.join(rel);
            assert!(p.is_file(), "expected bundled subfile {} to exist", p.display());
        }
    }

    #[test]
    fn auto_install_skill_creator_is_upgrade_aware() {
        // Simulate an older install: SKILL.md present, but no
        // agents/grader.md. The upgrade-detection branch should re-run
        // the full installer and lay down all five subfiles.
        use hermes_skills::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        // Plant a stale single-file install by writing only SKILL.md.
        let raw = include_str!("../../skills/skill-creator/SKILL.md");
        let (fm, body) = hermes_skills::parse_skill_doc(raw).unwrap();
        store.put(Scope::User, fm, &body).unwrap();
        let dir = store.skill_dir(Scope::User, "skill-creator").unwrap();
        assert!(!dir.join("agents").join("grader.md").exists());

        super::auto_install_skill_creator_skill(&store);

        assert!(dir.join("agents").join("grader.md").is_file());
        assert!(dir.join("references").join("schemas.md").is_file());
        assert!(dir.join("LICENSE.txt").is_file());
    }

    #[test]
    fn auto_install_skill_creator_is_noop_when_bundle_already_complete() {
        // When the multi-file marker is already on disk, the function
        // must not overwrite (preserves any local edits a user made).
        use hermes_skills::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        super::auto_install_skill_creator_skill(&store);

        // Overwrite SKILL.md with a sentinel and the marker file with a
        // sentinel; if the second call no-ops, both survive.
        let dir = store.skill_dir(Scope::User, "skill-creator").unwrap();
        let skill_md = dir.join("SKILL.md");
        let marker = dir.join("agents").join("grader.md");
        std::fs::write(&skill_md, "---\nname: skill-creator\ndescription: sentinel\nalways_active: false\n---\nsentinel-body\n").unwrap();
        std::fs::write(&marker, "sentinel-grader").unwrap();

        super::auto_install_skill_creator_skill(&store);

        assert_eq!(std::fs::read_to_string(&skill_md).unwrap().trim_end(), "---\nname: skill-creator\ndescription: sentinel\nalways_active: false\n---\nsentinel-body");
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "sentinel-grader");
    }
}
