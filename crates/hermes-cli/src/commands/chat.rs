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
use futures::StreamExt;
use hermes_core::{
    CompletionRequest, ContentBlock, LlmProvider, Message, Role, Session, SessionEvent,
    SessionMeta, StopReason, StreamEvent, ToolHost,
};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;

use super::context::ContextSources;
use super::readline::{ChatLineEditor, LineOutcome};
use super::util::{build_active_provider, load_tool_host, session_path_for};

const MAX_TOOL_ROUNDS: usize = 10;

pub async fn run(
    system: Option<String>,
    model_override: Option<String>,
    no_reflect: bool,
    resume_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let host = load_tool_host(&workspace_root).await?;
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
    let memory_store = FsMemoryStore::standard()
        .map_err(|e| anyhow::anyhow!("memory store: {e}"))?;

    let all_skills: Vec<LoadedSkill> = skill_store
        .list()
        .map_err(|e| anyhow::anyhow!("listing skills: {e}"))?;
    let active_memories: Vec<LoadedMemory> = memory_store
        .list_active()
        .map_err(|e| anyhow::anyhow!("listing memories: {e}"))?;
    let pinned_memories: Vec<LoadedMemory> = active_memories
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();

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
    eprintln!(
        "memory:   {} active ({} pinned)",
        active_memories.len(),
        pinned_memories.len()
    );
    eprintln!("skills:   {} loaded", all_skills.len());
    eprintln!("commands: /exit /quit /clear /tokens /tools /memory /skills /context /session /help");
    if let Some(warn) = super::reflect_stats::low_acceptance_warning(50) {
        eprintln!("{warn}");
    }
    eprintln!();

    let mut line_editor = ChatLineEditor::new()?;
    let mut pending_candidates: Vec<hermes_reflect::ReflectionOutput> = Vec::new();
    let mut turns_since_last_reflect: usize = 100; // start high so first turn can trigger
    let mut micro_reflected_this_session = false;

    loop {
        // Before prompting: show any pending micro-reflection candidates.
        for output in pending_candidates.drain(..) {
            if output.is_empty() {
                continue;
            }
            eprintln!();
            eprintln!("\x1b[90m💡 micro-reflection found candidates:\x1b[0m");
            present_micro_candidates(&output, &memory_store, &skill_store, &session);
        }

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
            if !handle_command(
                cmd,
                &mut session,
                &session_path,
                &tools,
                &all_skills,
                &active_memories,
                system.as_deref(),
            ) {
                break;
            }
            continue;
        }

        // Build per-turn system prompt: base + memories + skills index +
        // bodies of skills triggered by *this* user input.
        let sources = ContextSources {
            base: system.as_deref(),
            pinned: &pinned_memories,
            active: &active_memories,
            all_skills: &all_skills,
        };
        let turn_system = sources.build_turn_system(trimmed);

        let turn_msg_index = session.messages.len(); // before pushing user msg
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
                Ok(n) => eprintln!(
                    "(context compacted: {n} messages → summary + {} recent)",
                    session.messages.len() - 1
                ),
                Err(e) => eprintln!("(compaction failed: {e})"),
            }
        }

        if let Err(e) = run_one_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &model,
            &turn_system,
            provider_cfg.max_tokens,
            &workspace_root,
            &mut session,
            &mut writer,
        )
        .await
        {
            eprintln!("turn error: {e:#}");
        }
        println!();

        // --- Per-turn micro-reflection (background) ---
        turns_since_last_reflect += 1;
        let turn_msgs = &session.messages[turn_msg_index..];
        if !no_reflect
            && hermes_reflect::should_micro_reflect(turn_msgs, turns_since_last_reflect)
        {
            turns_since_last_reflect = 0;
            micro_reflected_this_session = true;
            let turn_msgs_owned: Vec<hermes_core::Message> = turn_msgs.to_vec();
            let skills_snap = all_skills.clone();
            let mems_snap = active_memories.clone();
            let prov = provider.clone();
            // Run synchronously (it's fast ~1s) to keep candidates ready
            // before the next prompt. If we want true async, use spawn +
            // channel, but for CLI the blocking is acceptable.
            match hermes_reflect::micro_reflect(
                prov.as_ref(),
                &turn_msgs_owned,
                &skills_snap,
                &mems_snap,
            )
            .await
            {
                Ok(output) => {
                    if !output.is_empty() {
                        pending_candidates.push(output);
                    }
                }
                Err(e) => {
                    tracing::debug!(error=%e, "micro-reflection failed");
                }
            }
        }
    }

    eprintln!("session saved: {}", session_path.display());
    line_editor.save_history();

    // Fallback: if micro-reflection never triggered this session AND there
    // are enough turns, run one final full reflection. Otherwise skip —
    // the user already saw candidates inline.
    if !no_reflect && !micro_reflected_this_session {
        let user_turns = session
            .messages
            .iter()
            .filter(|m| matches!(m.role, hermes_core::Role::User)
                && m.content.iter().any(|b| matches!(b, ContentBlock::Text { .. })))
            .count();
        if user_turns >= cfg.reflect.min_turns {
            eprintln!("(running end-of-session reflection...)");
            if let Err(e) = super::reflect::run_with_min_turns(
                provider.as_ref(),
                &session,
                0, // already checked min_turns above
            )
            .await
            {
                eprintln!("(reflection failed: {e:#})");
            }
        }
    }
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
    workspace: &std::path::Path,
    session: &mut Session,
    writer: &mut SessionWriter,
) -> Result<()> {
    let system = if turn_system.is_empty() {
        None
    } else {
        Some(turn_system.to_string())
    };

    for round in 0..MAX_TOOL_ROUNDS {
        let req = CompletionRequest {
            model: model.to_string(),
            system: system.clone(),
            messages: session.messages.clone(),
            tools: tools.to_vec(),
            max_tokens,
            temperature: None,
            enable_caching: provider.capabilities().prompt_caching,
        };

        let mut stream = provider
            .stream(req)
            .await
            .map_err(|e| anyhow::anyhow!("provider.stream: {e}"))?;

        // We render text deltas live; thinking deltas are summarised as a
        // status line so users see the model is working without flooding
        // their terminal with chain-of-thought.
        let mut text_started = false;
        let mut thinking_started = false;
        let mut thinking_preview = String::new();
        let mut final_resp: Option<hermes_core::CompletionResponse> = None;
        let mut stream_err: Option<anyhow::Error> = None;

        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(e) => {
                    stream_err = Some(anyhow::anyhow!("stream: {e}"));
                    break;
                }
            };
            match event {
                StreamEvent::TextDelta { text, .. } => {
                    if thinking_started {
                        // Clear the thinking line
                        eprint!("\r\x1b[K");
                        thinking_started = false;
                    }
                    if !text_started {
                        text_started = true;
                    }
                    print!("{text}");
                    std::io::stdout().flush().ok();
                }
                StreamEvent::ThinkingDelta { text, .. } => {
                    if !text_started {
                        thinking_preview.push_str(&text);
                        // Show a rolling preview of thinking (last 60 chars)
                        let preview: String = thinking_preview
                            .chars()
                            .rev()
                            .take(60)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        let preview = preview.replace('\n', " ");
                        eprint!("\r\x1b[K\x1b[90m  💭 {preview}\x1b[0m");
                        std::io::stderr().flush().ok();
                        thinking_started = true;
                    }
                }
                StreamEvent::ToolUseStart { name, .. } => {
                    if thinking_started {
                        eprint!("\r\x1b[K");
                        thinking_started = false;
                    }
                    // Don't print here — we'll show the full info at
                    // execution time when we have the complete input args.
                    let _ = name;
                }
                StreamEvent::Final(resp) => {
                    final_resp = Some(resp);
                }
                _ => {}
            }
        }
        if text_started {
            println!();
        } else if thinking_started {
            eprint!("\r\x1b[K");
        }

        if let Some(e) = stream_err {
            return Err(e);
        }
        let resp = final_resp
            .ok_or_else(|| anyhow::anyhow!("provider stream ended without Final event"))?;

        let assistant_msg = session.push_assistant(resp.content.clone()).clone();
        if let Err(e) = writer.append(&SessionEvent::Message(assistant_msg)) {
            tracing::warn!(error = %e, "persist assistant");
        }
        if let Err(e) = writer.append(&SessionEvent::Usage(resp.usage)) {
            tracing::warn!(error = %e, "persist usage");
        }
        session.record_usage(resp.usage);

        if resp.stop_reason != StopReason::ToolUse {
            return Ok(());
        }

        let tool_uses: Vec<(String, String, serde_json::Value)> = resp
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();
        if tool_uses.is_empty() {
            tracing::warn!("stop_reason=tool_use but no tool_use blocks; ending turn");
            return Ok(());
        }

        let mut tool_results = Vec::with_capacity(tool_uses.len());
        for (id, name, input) in tool_uses {
            eprintln!("\x1b[33m  {}\x1b[0m", friendly_tool_result(&name, &input, workspace));
            let outcome = match host.call(&name, input.clone()).await {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("\x1b[31m  ✗ {name} failed: {e}\x1b[0m");
                    hermes_core::ToolCallOutcome {
                        content: format!("tool call failed: {e}"),
                        is_error: true,
                    }
                }
            };
            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: outcome.content,
                is_error: outcome.is_error,
            });
        }

        let result_msg = Message {
            role: Role::User,
            content: tool_results,
        };
        session.messages.push(result_msg.clone());
        if let Err(e) = writer.append(&SessionEvent::Message(result_msg)) {
            tracing::warn!(error = %e, "persist tool_result");
        }

        tracing::debug!(round, "completed tool round");
    }

    eprintln!("(max tool rounds reached — stopping)");
    Ok(())
}

fn summarise_input(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() <= 80 {
        s
    } else {
        let truncated: String = s.chars().take(80).collect();
        format!("{truncated}…")
    }
}

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
fn compose_system_prompt(
    user_system: Option<String>,
    workspace_root: &std::path::Path,
) -> Option<String> {
    let clause = format!(
        "## Workspace\n\
         Your working directory is `{}`. All file reads, writes, and \
         commands MUST stay inside this directory. If a task seems to \
         require touching anything outside, stop and ask the user before \
         proceeding. Never modify the user's source code or system files \
         outside this workspace.",
        workspace_root.display()
    );
    Some(match user_system {
        Some(extra) if !extra.is_empty() => format!("{clause}\n\n{extra}"),
        _ => clause,
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_command(
    cmd: &str,
    session: &mut Session,
    path: &std::path::Path,
    tools: &[hermes_core::ToolSpec],
    skills: &[LoadedSkill],
    active_memories: &[LoadedMemory],
    base_system: Option<&str>,
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
            let sources = ContextSources {
                base: base_system,
                pinned: &pinned,
                active: active_memories,
                all_skills: skills,
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
        "help" => {
            eprintln!("commands:");
            eprintln!("  /exit, /quit   — leave the chat");
            eprintln!("  /clear         — drop in-memory history (transcript on disk kept)");
            eprintln!("  /tokens        — cumulative input/output token counts");
            eprintln!("  /tools         — list tools loaded from MCP servers");
            eprintln!("  /memory        — list active memories with ids");
            eprintln!("  /skills        — list available skills");
            eprintln!("  /context       — show the assembled session-level system prompt");
            eprintln!("  /session       — show transcript path and turn count");
            eprintln!("  /help          — this list");
        }
        other => eprintln!("unknown command: /{other}  (try /help)"),
    }
    true
}

/// Present micro-reflection candidates inline (non-blocking, quick a/r/s).
fn present_micro_candidates(
    output: &hermes_reflect::ReflectionOutput,
    memory_store: &FsMemoryStore,
    skill_store: &FsSkillStore,
    session: &Session,
) {
    use hermes_memory::{MemoryFrontmatter, Scope as MemScope, Source as MemSource};
    use hermes_skills::{Scope as SkScope, SkillFrontmatter};

    for c in &output.skill_candidates {
        eprintln!(
            "  \x1b[33m[skill]\x1b[0m {} — {}",
            c.name, c.description
        );
        eprint!("  accept? [a/r/s] ▸ ");
        std::io::stderr().flush().ok();
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            match input.trim() {
                "a" | "y" => {
                    let mut extra = serde_yaml::Mapping::new();
                    extra.insert(
                        serde_yaml::Value::String("source".into()),
                        serde_yaml::Value::String("micro_reflection".into()),
                    );
                    let fm = SkillFrontmatter {
                        name: c.name.clone(),
                        description: c.description.clone(),
                        triggers: c.triggers.clone(),
                        version: Some("0.1.0".into()),
                        license: None,
                        extra,
                    };
                    match skill_store.put(SkScope::User, fm, &c.body) {
                        Ok(p) => eprintln!("  \x1b[32m✓\x1b[0m {}", p.display()),
                        Err(e) => eprintln!("  \x1b[31m✗\x1b[0m {e}"),
                    }
                    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
                        at: chrono::Utc::now(),
                        session_id: session.meta.id.clone(),
                        kind: hermes_reflect::CandidateKind::Skill,
                        action: hermes_reflect::ActionTaken::Accept,
                        label: c.name.clone(),
                    });
                }
                "r" | "n" => {
                    eprintln!("  (rejected)");
                    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
                        at: chrono::Utc::now(),
                        session_id: session.meta.id.clone(),
                        kind: hermes_reflect::CandidateKind::Skill,
                        action: hermes_reflect::ActionTaken::Reject,
                        label: c.name.clone(),
                    });
                }
                _ => eprintln!("  (skipped)"),
            }
        }
    }

    for c in &output.memory_candidates {
        eprintln!(
            "  \x1b[33m[memory]\x1b[0m {}",
            c.fact.lines().next().unwrap_or("")
        );
        eprint!("  accept? [a/r/s] ▸ ");
        std::io::stderr().flush().ok();
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            match input.trim() {
                "a" | "y" => {
                    let mut fm =
                        MemoryFrontmatter::new(MemSource::Reflection, c.confidence, c.tags.clone());
                    fm.supersedes = c.supersedes.clone();
                    let scope = match c.scope {
                        MemScope::User => MemScope::User,
                        MemScope::Project => MemScope::Project,
                    };
                    match memory_store.put(scope, fm.clone(), &c.fact) {
                        Ok(p) => eprintln!("  \x1b[32m✓\x1b[0m {}", p.display()),
                        Err(e) => {
                            // Fallback to user scope
                            if matches!(scope, MemScope::Project) {
                                match memory_store.put(MemScope::User, fm, &c.fact) {
                                    Ok(p) => eprintln!("  \x1b[32m✓\x1b[0m {}", p.display()),
                                    Err(e2) => eprintln!("  \x1b[31m✗\x1b[0m {e2}"),
                                }
                            } else {
                                eprintln!("  \x1b[31m✗\x1b[0m {e}");
                            }
                        }
                    }
                    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
                        at: chrono::Utc::now(),
                        session_id: session.meta.id.clone(),
                        kind: hermes_reflect::CandidateKind::Memory,
                        action: hermes_reflect::ActionTaken::Accept,
                        label: c.fact.lines().next().unwrap_or("").to_string(),
                    });
                }
                "r" | "n" => {
                    eprintln!("  (rejected)");
                    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
                        at: chrono::Utc::now(),
                        session_id: session.meta.id.clone(),
                        kind: hermes_reflect::CandidateKind::Memory,
                        action: hermes_reflect::ActionTaken::Reject,
                        label: c.fact.lines().next().unwrap_or("").to_string(),
                    });
                }
                _ => eprintln!("  (skipped)"),
            }
        }
    }
}
