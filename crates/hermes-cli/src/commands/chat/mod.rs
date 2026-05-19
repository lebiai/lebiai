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

use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{
    ContentBlock, Role, Session, SessionEvent, SessionMeta,
};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;

use super::context::ContextSources;
use super::readline::{ChatLineEditor, LineOutcome};
use super::util::{build_active_provider, load_tool_host, session_path_for};

pub(crate) use system_prompt::compose_system_prompt;

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
                &skill_store,
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
