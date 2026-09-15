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
use hermes_channel::{ServeCtx, IM_TOOL_WHITELIST};
use hermes_core::{ContentBlock, Role, Session, SessionEvent, SessionMeta, ToolSpec};
use hermes_llm::Config;
use hermes_memory::{views_for, FsMemoryStore, LoadedMemory, MemoryEffectiveness, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillEffectiveness, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::subagent::MemoryView;
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
    persona: Option<&'static hermes_core::persona::Persona>,
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

    // ---- session: resume or fresh（身份先定，早于任何装配与网络连接）----
    // 这个会话是谁，只由 `session.meta.persona` 说了算：提示词的人物块、记忆归属、
    // micro 打标三处都读它。续会话时它以**文件里**的值为准，命令行的 `--persona`
    // 与文件不符就报错（见 `resumed_persona`）——静默换身份会让三处里外不一。
    let (mut session, mut writer, session_path, resumed, persona) = match resume_path {
        Some(path) => {
            let s = hermes_store::read_session(&path)
                .map_err(|e| anyhow::anyhow!("reading session {}: {e}", path.display()))?;
            let resumed_id = resumed_persona(
                &path,
                s.meta.persona.as_deref(),
                persona.map(|p| p.id.as_str()),
            )?;
            // 身份转换只走 `session_persona` 一处：它的返回类型里没有 `Err`，
            // 「文件里的 id 本版本不认识」在类型上就写不出 bail（见该函数）。
            let persona = session_persona(resumed_identity(resumed_id), &path);
            let w = SessionWriter::open_append(&path).map_err(|e| {
                anyhow::anyhow!("opening session {} for append: {e}", path.display())
            })?;
            (s, w, path, true, persona)
        }
        None => {
            let model_for_meta = model_override
                .clone()
                .unwrap_or_else(|| provider_cfg.model.clone());
            let meta = meta_for_new_session(&model_for_meta, provider.name(), persona);
            let path = session_path_for(&meta)?;
            let mut w = SessionWriter::create(&path)
                .with_context(|| format!("creating session file at {}", path.display()))?;
            w.append(&SessionEvent::Meta(meta.clone()))
                .context("writing session meta line")?;
            (Session::new(meta), w, path, false, persona)
        }
    };

    // 会话 → 记忆归属：转换只走 `persona::memory_owner_for` 一处（自带角色 → 全局）。
    let owner: Option<String> =
        hermes_core::persona::memory_owner_for(session.meta.persona.as_deref());

    // Wire up `subagent`: lets the agent spawn fresh child contexts (used by
    // skill-creator's eval flow — each test prompt runs in a clean context so
    // the parent's reasoning doesn't leak into the grade).
    // **建在 `owner` 之后**：child 是同一个会话伸出去的一只手，读面与写面都 = 父的
    // 那一件外套（`subagent_ctx_for` 恒给 `Scoped(owner)`）。
    let subagent_ctx = Arc::new(subagent_ctx_for(
        provider.clone(),
        provider_cfg.model.clone(),
        provider_cfg.max_tokens,
        cfg.limits.max_tool_rounds,
        PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
        workspace_root.clone(),
        memory_store_arc.clone(),
        Some(skill_store_arc.clone() as Arc<dyn SkillStore>),
        owner.clone(),
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
    // 工具面收窄：记忆工具只认这个会话的人物——读面只看「全局 + 本人物」，写面带上
    // 归属。`store` 必须与建 host 时用的是同一个 `Arc`（见 `PersonaToolHost::new`）。
    let host: Arc<dyn hermes_core::ToolHost> =
        Arc::new(hermes_tools::persona_scope::PersonaToolHost::new(
            host,
            Some(memory_store_arc.clone()),
            owner.clone(),
        ));
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?;

    // Prepend a hard-stated workspace clause to whatever system prompt
    // the user supplied. This is a soft constraint at the LLM level; the
    // hard constraint is the filesystem MCP's allowed-directory which
    // load_tool_host has already rewritten to match.
    let system = compose_system_prompt(
        system,
        &workspace_root,
        hermes_channel::PromptKind::Dialogue,
    );

    // ---- skill / memory snapshot for this session ----
    let active_memories: Vec<LoadedMemory> = memory_store_arc
        .list_active()
        .map_err(|e| anyhow::anyhow!("listing memories: {e}"))?;
    // 注入隔离（规格 §5.2）：人物会话只给「全局 + 本人物」，别人的一条都不给。
    let (active_view, pinned_view) = views_for(&active_memories, owner.as_deref());

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

    let topic_cards: Option<String> = if active_memories.is_empty() {
        None
    } else {
        hermes_memory::topics::render_from_disk(&active_view, owner.as_deref())
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
    if let Some(p) = persona {
        eprintln!("persona:  {} · {} ({})", p.name, p.role, p.id);
    }
    eprintln!("tools:    {} loaded", tools.len());
    {
        // 开场的这一行也是「他记得什么」的面：吃本会话可见的那一份，和
        // `/memory` / `/palace` / 真正注入模型的**同一份切片**（`active_view`）。
        // 用全量会让开了人物的会话报出别的人物才看得见的区与条数。
        let zones = hermes_memory::group_by_zone(&active_view);
        let zone_info: String = zones
            .iter()
            .map(|(z, m)| format!("{}:{}", z, m.len()))
            .collect::<Vec<_>>()
            .join(" ");
        if zone_info.is_empty() {
            eprintln!(
                "memory:   {} active ({} pinned)",
                active_view.len(),
                pinned_view.len()
            );
        } else {
            eprintln!(
                "palace:   {} memories across {} zones [{}]",
                active_view.len(),
                zones.len(),
                zone_info
            );
        }
    }
    eprintln!("skills:   {} loaded", all_skills.len());
    eprintln!("commands: /exit /quit /clear /tokens /stats /tools /memory /skills /context /session /remember /forget /reflect /compile /palace /help");
    eprintln!();

    // Auto-compile profile on first session if memories exist but profile.md doesn't.
    // `profile.md` 是**单个全局文件**、被每个视图无条件注入系统提示词，所以它只能装
    // 全局可见的口径（`profile_input`）。空集合守卫也用同一份——否则「全是人物私有」
    // 时会编出一个空 profile。
    let profile_memories = hermes_reflect::profile_input(&active_memories);
    if !profile_memories.is_empty() && hermes_memory::load_profile().unwrap_or(None).is_none() {
        eprintln!(
            "{}",
            style::dim("(compiling memory profile for the first time...)")
        );
        match hermes_reflect::compile_profile(provider.as_ref(), &profile_memories).await {
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
                persona,
                // 记忆分两份喂：`active_memories` 是全量（只有 `/compile` 该吃，
                // 编译前再取全局可见的一份给 `profile.md`）；`active_view` /
                // `pinned_view` 是本会话可见的那一份——与真正注入模型的**同一份切片**，
                // 显示面才不是假话。
                &active_memories,
                &active_view,
                &pinned_view,
                system.as_deref(),
                topic_cards.as_deref(),
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

        // Build per-turn system prompt: base + topic cards + skills index +
        // bodies of skills triggered by *this* user input.
        let compiled_profile = hermes_memory::load_profile().unwrap_or(None);
        let roster = hermes_core::persona::open();
        let sources = ContextSources {
            base: system.as_deref(),
            persona,
            roster: &roster,
            topic_cards: topic_cards.as_deref(),
            compiled_profile: compiled_profile.as_deref(),
            always_active_skills: &always_active_refs,
            pinned: &pinned_view,
            active: &active_view,
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
                &active_view,
                trimmed,
                3 + pinned_view.len(),
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
            let memory_owner =
                hermes_core::persona::memory_owner_for(session.meta.persona.as_deref());
            tokio::spawn(async move {
                let apply = hermes_reflect::MicroApplyConfig::new(
                    session_id,
                    auto_accept,
                    min_confidence,
                    false,
                )
                .with_memory_owner(memory_owner);
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

/// 新会话的身份写在 `meta.persona` 上——它是这次会话唯一的身份来源，
/// 提示词的人物块、记忆归属、micro 打标都读它。
///
/// 自带角色（李现 / 小文）也照样记 id：「记不记」与「归谁」是两件事——
/// 归属由 `persona::memory_owner_for` 判（自带角色 → 全局）。
fn meta_for_new_session(
    model: &str,
    provider: &str,
    persona: Option<&hermes_core::persona::Persona>,
) -> SessionMeta {
    let mut meta = SessionMeta::new(model, provider);
    meta.persona = persona.map(|p| p.id.clone());
    meta
}

/// 一次会话的**注入切片**：给 `list_active()` 的全量加一层可见性，返回
/// 这次会话派出去的 child 穿什么：**与父同一件** `PersonaToolHost`。
/// `hermes chat` 的父**恒**被收窄（无人物时收窄成「只看全局」），所以这里恒给
/// `Scoped(owner)`——**不是** `Unscoped`（那是父未被收窄的 `hermes agent` / IM）。
/// 读面与写面必须同轴：child 只读得到本人物，它写下的话也必须归本人物，否则就是
/// 「父私有、子广播」（Task 1.10f 的 B-1）。
/// 抽成纯函数是为了让这条接线本身可测（B-2）：把它改回 `Unscoped` 必须变红——
/// 以前它零覆盖，改坏了全量测试照样全绿。
#[allow(clippy::too_many_arguments)]
fn subagent_ctx_for(
    provider: Arc<dyn hermes_core::LlmProvider>,
    model: String,
    max_tokens: u32,
    max_tool_rounds: usize,
    permissions: PermissionChecker,
    workspace: std::path::PathBuf,
    memory_store: Arc<dyn MemoryStore>,
    skill_store: Option<Arc<dyn SkillStore>>,
    owner: Option<String>,
) -> SubagentContext {
    SubagentContext::new(
        provider,
        model,
        max_tokens,
        max_tool_rounds,
        permissions,
        workspace,
        Some(memory_store),
        MemoryView::Scoped(owner),
        skill_store,
    )
}

/// 续会话时文件里那个人物 id 的解析结果（`resumed_persona` 之后的第二步）。
/// 为什么单列一个类型而不是 `Option`：三种处境里有一种是**异常但必须放行**——
/// 文件里的 id 本版本不认识（授权名单变了 / 人物下线了）。压成 `Option` 会让
/// 「本来就没人物」和「不认识这个人物」在调用点长得一模一样，那条 warn 就容易被
/// 顺手删掉，用户只会看到自己的会话静默换了归属。
enum ResumedIdentity<'a> {
    /// 文件里没有人物：无人物会话，照常开工（零影响）。
    None,
    /// 本版本认识：用它。
    Persona(&'static hermes_core::persona::Persona),
    /// 本版本不认识：**不阻断**，按无人物开工；归属落全局（见调用点）；会话文件里
    /// 那个 id 原样不动（不改写用户文件）。`session_persona` 在这里留一条 warn 后放行。
    Unknown(&'a str),
}

fn resumed_identity(id: Option<&str>) -> ResumedIdentity<'_> {
    match id {
        None => ResumedIdentity::None,
        Some(id) => match hermes_core::persona::get(id) {
            Some(p) => ResumedIdentity::Persona(p),
            None => ResumedIdentity::Unknown(id),
        },
    }
}

/// 「这次会话算谁」：`ResumedIdentity` → 本次会话的人物。**返回类型里没有 `Err`**，
/// 所以「文件里的 id 本版本不认识」在类型上就写不出 bail —— 放行是编译期事实，
/// 不靠一句注释守着。
///
/// 为什么必须放行：bail 会把用户锁在自己的会话外用不了（授权名单变了 / 人物下线了）。
/// 放行的代价写在这里——会话文件里那个 id **原样不动**（不改写用户文件）；归属也
/// **不**随它走：`persona::memory_owner_for` 对不认识的 id 返回 `None`
/// （persona.rs:113-118），这个会话的记忆归**全局**。唯一的交代是那条 warn：用户得
/// 能查到自己的会话为什么"没了人物"。
fn session_persona(
    identity: ResumedIdentity<'_>,
    session: &std::path::Path,
) -> Option<&'static hermes_core::persona::Persona> {
    match identity {
        ResumedIdentity::None => None,
        ResumedIdentity::Persona(p) => Some(p),
        ResumedIdentity::Unknown(id) => {
            tracing::warn!(
                persona = %id,
                session = %session.display(),
                "会话文件里的人物 id 本版本不认识：不阻断，按无人物开工（归属落全局）"
            );
            None
        }
    }
}

/// 续会话时这次会话是谁：以**文件里**的 `meta.persona` 为准。
///
/// 命令行给的 id 与文件不符就算错——**包括**"文件里没有人物、命令行给了"。
/// 静默改身份会让这次会话的两套归属打架：micro 打标读 `session.meta.persona`，
/// 而工具写面读命令行那个人物，同一条记忆会被记到两个人头上。
fn resumed_persona<'a>(
    path: &std::path::Path,
    file_persona: Option<&'a str>,
    cli_id: Option<&str>,
) -> Result<Option<&'a str>> {
    match (file_persona, cli_id) {
        (Some(file), Some(cli)) if file != cli => anyhow::bail!(
            "会话 {} 属于人物 `{file}`，不接受 `--persona {cli}`。\n  \
             → 想以 `{cli}` 开工，请开新会话（去掉 --resume）",
            path.display()
        ),
        (None, Some(cli)) => anyhow::bail!(
            "会话 {} 没有人物，不接受 `--persona {cli}`。\n  \
             → 想以 `{cli}` 开工，请开新会话（去掉 --resume）",
            path.display()
        ),
        (file, _) => Ok(file),
    }
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
        // IM 渠道还没接人物（阶段 5）：`Unscoped` = 父未被收窄，child 也不收窄，
        // 行为与今天一致。
        MemoryView::Unscoped,
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
        .filter(|t| IM_TOOL_WHITELIST.contains(&t.name.as_str()))
        .collect();
    eprintln!("✓ tools ready: {} IM-whitelisted", tools.len());

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

    let topic_cards: Option<String> = if active_memories.is_empty() {
        None
    } else {
        // 工位：IM 渠道级固定人物归**阶段 5**（规格 §7）——今天这条路径恒无人物。
        hermes_memory::topics::render_from_disk(&active_memories, None)
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

    let base_system = compose_system_prompt(None, &workspace_root, hermes_channel::PromptKind::Im);
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
        topic_cards,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `subagent_ctx_for` 只做装配、不碰 provider；真被调到就是测试写错了
    /// （与 `subagent.rs` 的测试夹具同一手法）。
    struct NeverCalledProvider;

    #[async_trait::async_trait]
    impl hermes_core::LlmProvider for NeverCalledProvider {
        async fn complete(
            &self,
            _req: hermes_core::CompletionRequest,
        ) -> hermes_core::Result<hermes_core::CompletionResponse> {
            Err(hermes_core::Error::Provider(
                "test provider: complete() must never be called".into(),
            ))
        }

        fn capabilities(&self) -> hermes_core::Capabilities {
            hermes_core::Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: false,
            }
        }

        fn name(&self) -> &str {
            "never-called"
        }
    }

    fn write_session_meta(path: &std::path::Path, persona: Option<&str>) {
        let meta = meta_for_new_session("m", "p", persona.and_then(hermes_core::persona::get));
        let mut w = SessionWriter::create(path).unwrap();
        w.append(&SessionEvent::Meta(meta)).unwrap();
    }

    fn mem(owner: Option<&str>, body: &str, pinned: bool) -> LoadedMemory {
        let mut fm = hermes_memory::MemoryFrontmatter::new(
            hermes_memory::Source::User,
            hermes_memory::Confidence::High,
            vec![],
            "general".into(),
        )
        .owned(owner.map(str::to_string));
        fm.pinned = pinned;
        LoadedMemory {
            frontmatter: fm,
            body: body.into(),
            source_path: std::path::PathBuf::from("/nowhere"),
            scope: hermes_memory::Scope::User,
        }
    }

    /// 注入切片是用户可见价值的那条线（提示词 / `/memory` / `/palace` / 子代理都吃
    /// 它）——没有断言的接线等于没接。
    #[test]
    fn views_for_keeps_globals_and_own_memories_only() {
        let active = vec![
            mem(None, "全局：交付一律给 Word 放桌面", true),
            mem(Some("xiao-xie"), "自己的：林碳报告只引 IEA", false),
            mem(Some("wang-hai-yan"), "别人的：标题不夸张", true),
        ];

        let (view, pinned) = views_for(&active, Some("xiao-xie"));
        assert_eq!(
            view.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            vec!["全局：交付一律给 Word 放桌面", "自己的：林碳报告只引 IEA"],
            "人物会话只拿「全局 + 本人物」"
        );
        assert!(
            !view
                .iter()
                .any(|m| m.frontmatter.owner.as_deref() == Some("wang-hai-yan")),
            "别的人物的那条一条都不许进来"
        );
        // pinned 是 view 的**子集**：别人的 pinned 不许借「置顶」溜进提示词。
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].frontmatter.owner, None);
        assert!(pinned.iter().all(|p| {
            p.frontmatter.pinned && view.iter().any(|m| m.frontmatter.id == p.frontmatter.id)
        }));
    }

    #[test]
    fn views_for_without_a_persona_is_the_globals_only() {
        let active = vec![
            mem(None, "全局：交付一律给 Word 放桌面", true),
            mem(Some("xiao-xie"), "自己的：林碳报告只引 IEA", false),
            mem(Some("wang-hai-yan"), "别人的：标题不夸张", true),
        ];

        let (view, pinned) = views_for(&active, None);
        assert_eq!(view.len(), 1, "无人物 = 只看得到全局");
        assert_eq!(view[0].frontmatter.owner, None);
        assert_eq!(pinned.len(), 1);

        // 全是全局时 = 全量（不是「一律只剩 pinned」也不是空手）
        let globals = vec![
            mem(None, "全局一", true),
            mem(None, "全局二", false),
            mem(None, "全局三", false),
        ];
        let (view, pinned) = views_for(&globals, None);
        assert_eq!(view.len(), 3);
        assert_eq!(pinned.len(), 1);
    }

    #[test]
    fn a_new_session_records_the_persona_it_was_started_with() {
        let meta = meta_for_new_session("m", "p", hermes_core::persona::get("xiao-xie"));
        assert_eq!(meta.persona.as_deref(), Some("xiao-xie"));
        assert_eq!(
            meta_for_new_session("m", "p", None).persona,
            None,
            "没人物就别写出这个键"
        );

        // 自带角色照样记 id；「归全局」是归属判定的事，不是"不记"
        let le = meta_for_new_session("m", "p", hermes_core::persona::get("li-xian"));
        assert_eq!(le.persona.as_deref(), Some("li-xian"));
        assert_eq!(
            hermes_core::persona::memory_owner_for(le.persona.as_deref()),
            None,
            "自带角色会话产生的记忆算全局"
        );
    }

    #[test]
    fn resuming_keeps_the_identity_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        write_session_meta(&path, Some("xiao-xie"));
        let s = hermes_store::read_session(&path).unwrap();

        // 命令行没给 / 给的与文件一致 → 用文件里的那个人
        assert_eq!(
            resumed_persona(&path, s.meta.persona.as_deref(), None).unwrap(),
            Some("xiao-xie")
        );
        assert_eq!(
            resumed_persona(&path, s.meta.persona.as_deref(), Some("xiao-xie")).unwrap(),
            Some("xiao-xie")
        );
    }

    #[test]
    fn resuming_never_switches_identity_silently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        write_session_meta(&path, Some("xiao-xie"));
        let s = hermes_store::read_session(&path).unwrap();

        let err = resumed_persona(&path, s.meta.persona.as_deref(), Some("wang-hai-yan"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("xiao-xie") && err.contains("wang-hai-yan"),
            "报错要说清是谁的会话、想换成谁：{err}"
        );
        assert!(err.contains("新会话"), "报错要给出出路：{err}");

        // 旧会话（没有人物）也不许被"补"一个人物进去：那会让这段对话的记忆归属
        // 从全局变成某个人物的私有。
        let plain = dir.path().join("plain.jsonl");
        write_session_meta(&plain, None);
        let s = hermes_store::read_session(&plain).unwrap();
        assert_eq!(s.meta.persona, None);
        let err = resumed_persona(&plain, s.meta.persona.as_deref(), Some("xiao-xie"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("没有人物"), "{err}");
    }

    /// child 的视野与父**同一件外套**（Task 1.10f 的 B-2）：`hermes chat` 的父恒被
    /// `PersonaToolHost` 收窄，所以 child 恒是 `Scoped(owner)`。这一行以前零覆盖——
    /// 改成 `Unscoped`（= 把 1.10d 整条撤销）全量测试也全绿，这条就是钉子。
    #[test]
    fn subagent_ctx_follows_the_parent_persona() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn MemoryStore> =
            Arc::new(FsMemoryStore::new(dir.path().to_path_buf(), None));

        let xie = subagent_ctx_for(
            Arc::new(NeverCalledProvider),
            "m".into(),
            1024,
            4,
            PermissionChecker::new(&[], &[]),
            dir.path().to_path_buf(),
            store.clone(),
            None,
            Some("xiao-xie".into()),
        );
        assert!(
            matches!(&xie.memory_view, MemoryView::Scoped(Some(id)) if id == "xiao-xie"),
            "人物会话的 child 必须继承本人物: {:?}",
            xie.memory_view
        );

        // 无人物会话也必须**恒**收窄成「只看全局」：`Unscoped`（父没被收窄）是
        // `hermes agent` / IM 才有的处境，`hermes chat` 不许出现。
        let nobody = subagent_ctx_for(
            Arc::new(NeverCalledProvider),
            "m".into(),
            1024,
            4,
            PermissionChecker::new(&[], &[]),
            dir.path().to_path_buf(),
            store,
            None,
            None,
        );
        assert!(
            matches!(nobody.memory_view, MemoryView::Scoped(None)),
            "无人物会话的 child 也必须收窄成只看全局（不许 Unscoped）: {:?}",
            nobody.memory_view
        );
    }

    /// 续会话时文件里的人物 id 本版本不认识：**不阻断**，按无人物开工——Task 1.10f
    /// 的 B-5。bail 会把用户锁在自己的会话外用不了（授权名单变了 / 人物下线了）。
    /// 这条钉的是**决定本身**（`Unknown` → `None`），不是「`Unknown` 变体存在」：
    /// 老版本只调了纯分类函数，把调用点改成 bail 也全绿（Task 1.12 必修 A）。
    #[test]
    fn an_unknown_resumed_persona_id_does_not_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");

        assert_eq!(
            session_persona(resumed_identity(None), &path).map(|p| p.id.as_str()),
            None,
            "文件里没人物 → 无人物开工"
        );
        assert_eq!(
            session_persona(resumed_identity(Some("xiao-xie")), &path).map(|p| p.id.as_str()),
            Some("xiao-xie"),
            "认识的 id → 用文件里那个人物"
        );

        // 不认识的 id：**不 panic、不报错**，按无人物开工（会话文件里那个 id 原样不动）。
        assert_eq!(
            session_persona(resumed_identity(Some("nobody-here")), &path).map(|p| p.id.as_str()),
            None,
            "不认识的 id 必须放行：整条链走一遍也不许阻断"
        );
        assert_eq!(
            session_persona(ResumedIdentity::Unknown("nobody"), &path).map(|p| p.id.as_str()),
            None,
            "直接给它 Unknown 也放行"
        );

        // 分类仍要报出是哪个 id：那条 warn 靠它，静默当没人物会让用户查不出原因。
        match resumed_identity(Some("nobody-here")) {
            ResumedIdentity::Unknown(id) => assert_eq!(id, "nobody-here"),
            _ => panic!("不认识的 id 必须走 Unknown，不许静默当没人物"),
        }
    }
}
