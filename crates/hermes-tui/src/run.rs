//! Event loop and orchestration.

use std::sync::Arc;

use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use hermes_core::{
    CompletionRequest, ContentBlock, LlmProvider, Message, Role, Session, SessionEvent,
    SessionMeta, StopReason, StreamEvent, ToolHost,
};
use hermes_llm::Config;
use hermes_memory::{
    FsMemoryStore, MemoryFrontmatter, MemoryStore, Scope as MemoryScope,
    Source as MemorySource,
};
use hermes_reflect::{reflect, ConflictCandidate, MemoryCandidate, SkillCandidate};
use hermes_skills::{FsSkillStore, Scope as SkillScope, SkillFrontmatter, SkillStore};
use hermes_store::SessionWriter;
use tokio::sync::mpsc;

use crate::app::{App, Candidate, Mode, Status, Streaming};
use crate::context::ContextSources;
use crate::ui::render;
use crate::util::{build_active_provider, load_tool_host, session_path_for};

/// Events produced by the streaming turn task, delivered to the UI loop.
enum TurnMsg {
    Text(String),
    Thinking,
    ToolStart(String),
    /// A completed assistant message with its content blocks + usage.
    /// Sent once per LLM round; the UI persists it and updates counters.
    AssistantFinal(hermes_core::CompletionResponse),
    /// A synthesised user message carrying tool_result blocks. Sent after
    /// each tool round so the UI loop can persist it to the JSONL.
    ToolResults(Message),
    Error(String),
    Done,
}

/// Events from the reflection task.
enum ReflectMsg {
    Summary(String),
    Ready(Vec<Candidate>),
    Error(String),
}

const MAX_TOOL_ROUNDS: usize = 10;

pub async fn main_loop() -> Result<()> {
    // Load config + stores before entering raw mode so errors surface
    // with a usable terminal.
    let cfg = Config::load_default()
        .context("loading ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let workspace_root = cfg.workspace.root.clone();
    let host = load_tool_host(&workspace_root).await?;
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("listing tools: {e}"))?;

    let skill_store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let memory_store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let skills = skill_store.list().map_err(|e| anyhow::anyhow!("{e}"))?;
    let active_memories = memory_store
        .list_active()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let pinned_memories: Vec<_> = active_memories
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();

    let model = provider_cfg.model.clone();
    let meta = SessionMeta::new(&model, "anthropic");
    let session_path = session_path_for(&meta)?;
    let writer = SessionWriter::create(&session_path)
        .with_context(|| format!("creating session at {}", session_path.display()))?;

    let status = Status {
        model: model.clone(),
        tools: tools.len(),
        memories: active_memories.len(),
        pinned: pinned_memories.len(),
        skills: skills.len(),
        input_tokens: 0,
        output_tokens: 0,
        session_path: session_path.display().to_string(),
    };

    let mut app = App {
        entries: Vec::new(),
        input: String::new(),
        cursor: 0,
        scroll: 0,
        streaming: Streaming::Idle,
        status,
        should_quit: false,
        skills,
        active_memories,
        pinned_memories,
        mode: Mode::Normal,
        candidates: std::collections::VecDeque::new(),
        quit_after_reflect: false,
        reflect_pending: false,
        reflect_force: false,
    };
    for line in hermes_core::banner::LOGO_PLAIN.lines() {
        app.push_info(line.to_string());
    }
    app.push_info(format!("session: {}", session_path.display()));
    app.push_info(format!(
        "model {} · {} tools · {} mem ({} pinned) · {} skills · /help",
        app.status.model,
        app.status.tools,
        app.status.memories,
        app.status.pinned,
        app.status.skills,
    ));

    // Install panic hook before entering raw mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::try_restore();
        default_hook(info);
    }));

    let mut terminal = ratatui::try_init().context("initialising terminal")?;

    let session = Session::new(meta);
    let writer = std::sync::Mutex::new(writer);

    let result = run_event_loop(
        &mut terminal,
        &mut app,
        provider.clone(),
        host.clone(),
        tools,
        model,
        provider_cfg.max_tokens,
        cfg.reflect.min_turns,
        session,
        writer,
    )
    .await;

    let _ = ratatui::try_restore();
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    provider: Arc<dyn LlmProvider>,
    host: Arc<dyn ToolHost>,
    tools: Vec<hermes_core::ToolSpec>,
    model: String,
    max_tokens: u32,
    reflect_min_turns: usize,
    mut session: Session,
    writer: std::sync::Mutex<SessionWriter>,
) -> Result<()> {
    // Persist the meta line once at start.
    if let Ok(mut w) = writer.lock() {
        let _ = w.append(&SessionEvent::Meta(session.meta.clone()));
    }

    let mut events = EventStream::new();
    // Current in-flight turn handle — holds the receiver and cancel guard.
    let mut turn_rx: Option<mpsc::UnboundedReceiver<TurnMsg>> = None;
    let mut turn_cancel: Option<tokio::sync::oneshot::Sender<()>> = None;
    // Reflection task channel.
    let mut reflect_rx: Option<mpsc::UnboundedReceiver<ReflectMsg>> = None;

    loop {
        // Drain any pending reflection request first; this lets /reflect
        // and quit-driven reflection share one entry point. Quit-driven
        // requests respect the configured min_turns; explicit /reflect
        // bypasses the threshold via app.reflect_force.
        if app.reflect_pending && reflect_rx.is_none() && app.mode == Mode::Normal {
            let user_turns = session
                .messages
                .iter()
                .filter(|m| {
                    matches!(m.role, hermes_core::Role::User)
                        && m.content.iter().any(|b| matches!(b, hermes_core::ContentBlock::Text { .. }))
                })
                .count();
            let allowed = app.reflect_force || user_turns >= reflect_min_turns;
            app.reflect_pending = false;
            app.reflect_force = false;
            if !allowed {
                app.push_info(format!(
                    "(reflection skipped — only {user_turns} user turn(s); threshold is {reflect_min_turns})"
                ));
                if app.quit_after_reflect {
                    app.should_quit = true;
                }
            } else {
                let (tx, rx) = mpsc::unbounded_channel();
                reflect_rx = Some(rx);
                spawn_reflect(
                    provider.clone(),
                    session.clone(),
                    app.skills.clone(),
                    app.active_memories.clone(),
                    tx,
                );
                app.mode = Mode::ReflectWaiting;
                app.push_info("(running reflection…)");
            }
        }

        terminal.draw(|f| render(f, app))?;
        if app.should_quit {
            break;
        }

        tokio::select! {
            // Keyboard / resize.
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(k))) => {
                        if k.kind == KeyEventKind::Press {
                            match app.mode {
                                Mode::Normal => {
                                    handle_key(app, k, &mut turn_cancel).await;
                                    if let Some(cmd) = take_pending_submit(app) {
                                        if turn_rx.is_none() {
                                            let (tx, rx) = mpsc::unbounded_channel();
                                            let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
                                            turn_rx = Some(rx);
                                            turn_cancel = Some(cancel_tx);
                                            spawn_turn(
                                                provider.clone(),
                                                host.clone(),
                                                tools.clone(),
                                                model.clone(),
                                                max_tokens,
                                                app,
                                                &mut session,
                                                &writer,
                                                cmd,
                                                tx,
                                                cancel_rx,
                                            );
                                        }
                                    }
                                }
                                Mode::Reflect => {
                                    handle_reflect_key(app, k);
                                }
                                Mode::ReflectWaiting => {
                                    // Eat keys; reflection is in flight.
                                    let _ = k;
                                }
                            }
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        app.push_info(format!("terminal error: {e}"));
                    }
                    None => break,
                }
            }
            // Turn events.
            Some(msg) = async {
                match turn_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match msg {
                    TurnMsg::Text(t) => app.extend_assistant(&t),
                    TurnMsg::Thinking => app.begin_thinking(),
                    TurnMsg::ToolStart(name) => app.push_tool(format!("calling {name}…")),
                    TurnMsg::AssistantFinal(resp) => {
                        session.record_usage(resp.usage);
                        app.status.input_tokens = session.total_input_tokens;
                        app.status.output_tokens = session.total_output_tokens;
                        let msg = Message {
                            role: Role::Assistant,
                            content: resp.content.clone(),
                        };
                        session.messages.push(msg.clone());
                        if let Ok(mut w) = writer.lock() {
                            let _ = w.append(&SessionEvent::Message(msg));
                            let _ = w.append(&SessionEvent::Usage(resp.usage));
                        }
                    }
                    TurnMsg::ToolResults(msg) => {
                        session.messages.push(msg.clone());
                        if let Ok(mut w) = writer.lock() {
                            let _ = w.append(&SessionEvent::Message(msg));
                        }
                    }
                    TurnMsg::Error(e) => {
                        app.end_streaming();
                        app.push_info(format!("error: {e}"));
                    }
                    TurnMsg::Done => {
                        app.end_streaming();
                        turn_rx = None;
                        turn_cancel = None;
                    }
                }
            }
            // Reflection events.
            Some(msg) = async {
                match reflect_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match msg {
                    ReflectMsg::Summary(s) => {
                        app.push_info(format!("reflection: {s}"));
                    }
                    ReflectMsg::Ready(cands) => {
                        let n = cands.len();
                        for c in cands {
                            app.candidates.push_back(c);
                        }
                        reflect_rx = None;
                        if n == 0 {
                            app.push_info("(no candidates)");
                            app.mode = Mode::Normal;
                            if app.quit_after_reflect {
                                app.should_quit = true;
                            }
                        } else {
                            app.push_info(format!(
                                "{n} candidate(s) — a/r/d skill+memory · n/o/m/s/k conflict"
                            ));
                            present_next_candidate(app);
                            app.mode = Mode::Reflect;
                        }
                    }
                    ReflectMsg::Error(e) => {
                        app.push_info(format!("reflection error: {e}"));
                        reflect_rx = None;
                        app.mode = Mode::Normal;
                        if app.quit_after_reflect {
                            app.should_quit = true;
                        }
                    }
                }
            }
        }
    }

    if let Ok(w) = writer.lock() {
        drop(w);
    }
    Ok(())
}

fn take_pending_submit(app: &mut App) -> Option<String> {
    // submit is signalled by the Entry::User we just pushed + streaming ==
    // Idle; actual extraction happens here. We don't use a boolean because
    // handle_key always leaves the submitted text as the last User entry.
    if matches!(app.streaming, Streaming::Idle) {
        if let Some(Entry::User(s)) = app
            .entries
            .iter()
            .rev()
            .find(|e| !matches!(e, Entry::Info(_)))
        {
            if app
                .entries
                .last()
                .map(|e| matches!(e, Entry::User(_)))
                .unwrap_or(false)
            {
                // Mark as "started" by flipping streaming to Thinking so a
                // repeated render doesn't resubmit.
                app.streaming = Streaming::Thinking;
                return Some(s.clone());
            }
        }
    }
    None
}

use crate::app::Entry;

async fn handle_key(
    app: &mut App,
    key: KeyEvent,
    cancel: &mut Option<tokio::sync::oneshot::Sender<()>>,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Char('c'), true) => {
            if let Some(c) = cancel.take() {
                let _ = c.send(());
                app.push_info("(cancelled)");
            } else if app.input.is_empty() {
                app.push_info("(^C — use ^D or /exit to quit)");
            } else {
                app.input_clear();
            }
        }
        (KeyCode::Char('d'), true) => {
            request_quit(app);
        }
        (KeyCode::Char('a'), true) => app.cursor_home(),
        (KeyCode::Char('e'), true) => app.cursor_end(),
        (KeyCode::Char('u'), true) => app.input_clear(),
        (KeyCode::Home, _) => app.cursor_home(),
        (KeyCode::End, _) => app.cursor_end(),
        (KeyCode::Left, _) => app.cursor_left(),
        (KeyCode::Right, _) => app.cursor_right(),
        (KeyCode::PageUp, _) => app.scroll_up(5),
        (KeyCode::PageDown, _) => app.scroll_down(5),
        (KeyCode::Backspace, _) => app.input_backspace(),
        (KeyCode::Enter, _) => {
            let text = app.take_input();
            if text.is_empty() {
                return;
            }
            if let Some(cmd) = text.strip_prefix('/') {
                handle_slash(app, cmd);
                return;
            }
            app.push_user(text);
            app.scroll_bottom();
        }
        (KeyCode::Char(c), _) => {
            app.input_insert_char(c);
        }
        _ => {}
    }
}

/// Request graceful exit: if there are messages to reflect on and a
/// reflection has not already been run/refused this session, schedule
/// one. Otherwise quit immediately.
fn request_quit(app: &mut App) {
    let has_messages = app
        .entries
        .iter()
        .any(|e| matches!(e, Entry::User(_) | Entry::Assistant(_)));
    if has_messages && app.mode == Mode::Normal {
        app.quit_after_reflect = true;
        app.reflect_pending = true;
        app.push_info("(quit — running reflection first; ^D again to skip)");
    } else {
        app.should_quit = true;
    }
}

fn handle_slash(app: &mut App, cmd: &str) {
    match cmd.trim() {
        "exit" | "quit" => request_quit(app),
        "reflect" => {
            if app.entries.iter().any(|e| matches!(e, Entry::User(_) | Entry::Assistant(_))) {
                app.reflect_pending = true;
                app.reflect_force = true;
            } else {
                app.push_info("(nothing to reflect on yet)");
            }
        }
        "clear" => {
            app.entries.clear();
            app.push_info("(scrollback cleared)");
        }
        "memory" | "memories" => {
            if app.active_memories.is_empty() {
                app.push_info("(no active memories)");
            } else {
                let lines: Vec<String> = app
                    .active_memories
                    .iter()
                    .map(|m| {
                        let pin = if m.frontmatter.pinned { "★ " } else { "  " };
                        let line = m.body.lines().next().unwrap_or("").trim();
                        format!("{pin}{} {}", m.frontmatter.id, line)
                    })
                    .collect();
                for l in lines {
                    app.push_info(l);
                }
            }
        }
        "skills" => {
            if app.skills.is_empty() {
                app.push_info("(no skills)");
            } else {
                let lines: Vec<String> = app
                    .skills
                    .iter()
                    .map(|s| format!("  {}: {}", s.frontmatter.name, s.frontmatter.description))
                    .collect();
                for l in lines {
                    app.push_info(l);
                }
            }
        }
        "tools" => {
            app.push_info(format!("(tools loaded: {})", app.status.tools));
        }
        "tokens" => app.push_info(format!(
            "tokens in/out: {}/{}",
            app.status.input_tokens, app.status.output_tokens
        )),
        "context" => {
            let sources = ContextSources {
                base: None,
                pinned: &app.pinned_memories,
                active: &app.active_memories,
                all_skills: &app.skills,
            };
            let s = sources.build_session_system();
            if s.is_empty() {
                app.push_info("(empty system prompt)");
            } else {
                for line in s.lines() {
                    app.push_info(line.to_string());
                }
            }
        }
        "help" => {
            for l in [
                "/exit /quit      — leave (runs reflection if there are messages)",
                "/reflect         — reflect on this session now",
                "/clear           — clear scrollback",
                "/memory          — list active memories",
                "/skills          — list skills",
                "/tools           — tool count",
                "/tokens          — cumulative tokens",
                "/context         — show session-level system prompt",
                "^C               — cancel current stream / clear input",
                "^D               — quit (graceful)",
                "↑↓ PgUp PgDn     — scroll transcript",
                "in reflection: a/r/d on skills+memories · n/o/m/s/k on conflicts",
            ] {
                app.push_info(l);
            }
        }
        other => app.push_info(format!("unknown command: /{other}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_turn(
    provider: Arc<dyn LlmProvider>,
    host: Arc<dyn ToolHost>,
    tools: Vec<hermes_core::ToolSpec>,
    model: String,
    max_tokens: u32,
    app: &mut App,
    session: &mut Session,
    writer: &std::sync::Mutex<SessionWriter>,
    user_input: String,
    tx: mpsc::UnboundedSender<TurnMsg>,
    cancel: tokio::sync::oneshot::Receiver<()>,
) {
    let sources = ContextSources {
        base: None,
        pinned: &app.pinned_memories,
        active: &app.active_memories,
        all_skills: &app.skills,
    };
    let turn_system = sources.build_turn_system(&user_input);

    let user_msg = session.push_user(&user_input).clone();
    if let Ok(mut w) = writer.lock() {
        let _ = w.append(&SessionEvent::Message(user_msg));
    }

    let messages = session.messages.clone();
    let skills = app.skills.clone();
    let memories = app.active_memories.clone();

    tokio::spawn(async move {
        let config = hermes_turn::TurnConfig {
            model,
            system: if turn_system.is_empty() { None } else { Some(turn_system) },
            max_tokens,
            max_tool_rounds: MAX_TOOL_ROUNDS,
            enable_micro_reflect: false,
            turns_since_last_reflect: 0,
        };

        let tx2 = tx.clone();
        let on_event = move |event: hermes_turn::TurnEvent| {
            match event {
                hermes_turn::TurnEvent::TextDelta(text) => {
                    let _ = tx2.send(TurnMsg::Text(text));
                }
                hermes_turn::TurnEvent::ThinkingDelta(_) => {
                    let _ = tx2.send(TurnMsg::Thinking);
                }
                hermes_turn::TurnEvent::ToolUseStart { name, .. } => {
                    let _ = tx2.send(TurnMsg::ToolStart(name));
                }
                hermes_turn::TurnEvent::ToolUseResult { .. } => {}
                hermes_turn::TurnEvent::Usage { .. } => {}
                hermes_turn::TurnEvent::MicroReflection(_) => {}
                hermes_turn::TurnEvent::Error(msg) => {
                    let _ = tx2.send(TurnMsg::Error(msg));
                }
                hermes_turn::TurnEvent::Done => {}
            }
        };

        let result = hermes_turn::run_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &messages,
            &config,
            &skills,
            &memories,
            on_event,
            cancel,
        )
        .await;

        match result {
            Ok(output) => {
                for msg in &output.new_messages {
                    if msg.role == Role::Assistant {
                        let resp = hermes_core::CompletionResponse {
                            content: msg.content.clone(),
                            stop_reason: StopReason::EndTurn,
                            usage: output.usage,
                        };
                        let _ = tx.send(TurnMsg::AssistantFinal(resp));
                    } else {
                        let _ = tx.send(TurnMsg::ToolResults(msg.clone()));
                    }
                }
            }
            Err(_) => {}
        }
        let _ = tx.send(TurnMsg::Done);
    });
}

// ---------------- reflection ----------------

fn spawn_reflect(
    provider: Arc<dyn LlmProvider>,
    session: Session,
    skills: Vec<hermes_skills::LoadedSkill>,
    memories: Vec<hermes_memory::LoadedMemory>,
    tx: mpsc::UnboundedSender<ReflectMsg>,
) {
    tokio::spawn(async move {
        let output = match reflect(provider.as_ref(), &session, &skills, &memories).await {
            Ok(o) => o,
            Err(e) => {
                let _ = tx.send(ReflectMsg::Error(e.to_string()));
                return;
            }
        };

        if !output.summary.is_empty() {
            let _ = tx.send(ReflectMsg::Summary(output.summary.clone()));
        }

        // Index conflicts by the id they reference, so each memory candidate
        // can pick up at most one matching conflict.
        let mut conflicts_by_old: std::collections::HashMap<String, Vec<ConflictCandidate>> =
            std::collections::HashMap::new();
        for c in &output.conflicts {
            conflicts_by_old
                .entry(c.with.clone())
                .or_default()
                .push(c.clone());
        }

        let mut cands: Vec<Candidate> = Vec::new();
        for s in output.skill_candidates {
            cands.push(Candidate::Skill(s));
        }
        let mut consumed_conflict_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for m in output.memory_candidates {
            let conflict = m
                .supersedes
                .iter()
                .find_map(|old_id| {
                    conflicts_by_old
                        .get(old_id)
                        .and_then(|v| v.first())
                        .map(|cc| (old_id.clone(), cc.clone()))
                });
            if let Some((old_id, _)) = &conflict {
                consumed_conflict_ids.insert(old_id.clone());
            }
            cands.push(Candidate::Memory { cand: m, conflict });
        }
        // Surface conflicts not already attached.
        for c in output.conflicts {
            if !consumed_conflict_ids.contains(&c.with) {
                cands.push(Candidate::OrphanConflict(c));
            }
        }
        let _ = tx.send(ReflectMsg::Ready(cands));
    });
}

fn present_next_candidate(app: &mut App) {
    let Some(c) = app.candidates.front().cloned() else {
        return;
    };
    let total_left = app.candidates.len();
    match c {
        Candidate::Skill(s) => {
            app.push_info(format!(
                "[Skill {}/?]  name: {}",
                pos_left(app, total_left),
                s.name
            ));
            app.push_info(format!("  description: {}", s.description));
            if !s.triggers.is_empty() {
                app.push_info(format!("  triggers:    {}", s.triggers.join(", ")));
            }
            app.push_info(format!("  rationale:   {}", s.rationale));
            app.push_info(format!("  confidence:  {:?}", s.confidence));
            for line in s.body.lines().take(3) {
                app.push_info(format!("  | {line}"));
            }
            let extra = s.body.lines().count().saturating_sub(3);
            if extra > 0 {
                app.push_info(format!("  | ... ({extra} more)"));
            }
            app.push_info("  Action: [a]ccept / [r]eject / [d]efer");
        }
        Candidate::Memory { cand, conflict } => match conflict {
            None => {
                app.push_info(format!(
                    "[Memory]  fact: {}",
                    cand.fact.lines().next().unwrap_or("")
                ));
                app.push_info(format!(
                    "  scope: {:?}  tags: {}  confidence: {:?}",
                    cand.scope,
                    cand.tags.join(", "),
                    cand.confidence
                ));
                app.push_info(format!("  rationale: {}", cand.rationale));
                if !cand.supersedes.is_empty() {
                    app.push_info(format!("  supersedes: {}", cand.supersedes.join(", ")));
                }
                app.push_info("  Action: [a]ccept / [r]eject / [d]efer");
            }
            Some((old_id, conflict)) => {
                app.push_info(format!("[Memory]  ⚠ CONFLICT with {old_id}"));
                app.push_info(format!("  kind: {} — {}", conflict.kind, conflict.explain));
                if let Some(old) = app
                    .active_memories
                    .iter()
                    .find(|m| m.frontmatter.id == old_id)
                {
                    let line = old.body.lines().next().unwrap_or("").trim();
                    app.push_info(format!("  OLD: {line}"));
                } else {
                    app.push_info("  OLD: (memory not found on disk)");
                }
                app.push_info(format!("  NEW: {}", cand.fact.lines().next().unwrap_or("")));
                app.push_info(
                    "  Resolve: [n]ew-supersedes-old / [o]ld-keep / [m]erge / [s]plit / s[k]ip",
                );
            }
        },
        Candidate::OrphanConflict(c) => {
            app.push_info(format!("[Conflict]  with {}", c.with));
            app.push_info(format!("  kind: {} — {}", c.kind, c.explain));
            app.push_info("  (informational only; press any reflect-action key to acknowledge)");
        }
    }
}

fn pos_left(app: &App, _: usize) -> usize {
    // Display position 1-based.
    app.entries
        .iter()
        .filter(|e| matches!(e, Entry::Info(s) if s.starts_with('[')))
        .count()
        + 1
}

fn handle_reflect_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d')) {
        // Bail out of reflection.
        app.candidates.clear();
        app.mode = Mode::Normal;
        if app.quit_after_reflect {
            app.should_quit = true;
        } else {
            app.push_info("(reflection cancelled)");
        }
        return;
    }
    let KeyCode::Char(c) = key.code else { return };
    let cur = match app.candidates.pop_front() {
        Some(c) => c,
        None => return,
    };
    apply_reflect_action(app, cur, c);

    if app.candidates.is_empty() {
        app.mode = Mode::Normal;
        app.push_info("(reflection done)");
        if app.quit_after_reflect {
            app.should_quit = true;
        }
    } else {
        present_next_candidate(app);
    }
}

fn apply_reflect_action(app: &mut App, cand: Candidate, c: char) {
    match cand {
        Candidate::Skill(s) => match c.to_ascii_lowercase() {
            'a' => match persist_skill(&s) {
                Ok(p) => app.push_info(format!("  ✓ wrote {}", p.display())),
                Err(e) => app.push_info(format!("  ✗ {e}")),
            },
            'r' => app.push_info("  (rejected)"),
            'd' => app.push_info("  (deferred)"),
            other => app.push_info(format!("  unknown action {other:?} (a/r/d)")),
        },
        Candidate::Memory { cand: m, conflict } => match (conflict, c.to_ascii_lowercase()) {
            (None, 'a') => match persist_memory(&m) {
                Ok(p) => {
                    app.push_info(format!("  ✓ wrote {}", p.display()));
                    refresh_memories(app);
                }
                Err(e) => app.push_info(format!("  ✗ {e}")),
            },
            (None, 'r') => app.push_info("  (rejected)"),
            (None, 'd') => app.push_info("  (deferred)"),
            (Some((old_id, _)), 'n') => {
                let mut nm = m.clone();
                if !nm.supersedes.iter().any(|i| i == &old_id) {
                    nm.supersedes.push(old_id);
                }
                match persist_memory(&nm) {
                    Ok(p) => {
                        app.push_info(format!("  ✓ wrote {} (new supersedes old)", p.display()));
                        refresh_memories(app);
                    }
                    Err(e) => app.push_info(format!("  ✗ {e}")),
                }
            }
            (Some(_), 'o') => app.push_info("  (kept old; new discarded)"),
            (Some((old_id, _)), 'm') => {
                // Merge: in TUI we do not have $EDITOR open. v1: write the
                // candidate verbatim with supersedes link, mark a TODO line.
                let mut nm = m.clone();
                if !nm.supersedes.iter().any(|i| i == &old_id) {
                    nm.supersedes.push(old_id);
                }
                nm.fact = format!("{}  (TODO: merge manually)", nm.fact);
                match persist_memory(&nm) {
                    Ok(p) => {
                        app.push_info(format!("  ✓ wrote {} — edit on disk to refine", p.display()));
                        refresh_memories(app);
                    }
                    Err(e) => app.push_info(format!("  ✗ {e}")),
                }
            }
            (Some((old_id, _)), 's') => {
                let mut nm = m.clone();
                nm.scope = match nm.scope {
                    MemoryScope::User => MemoryScope::Project,
                    MemoryScope::Project => MemoryScope::User,
                };
                nm.supersedes.retain(|i| i != &old_id);
                match persist_memory(&nm) {
                    Ok(p) => {
                        app.push_info(format!("  ✓ wrote {} (scope split)", p.display()));
                        refresh_memories(app);
                    }
                    Err(e) => app.push_info(format!("  ✗ {e}")),
                }
            }
            (Some(_), 'k') => app.push_info("  (skipped)"),
            (_, 'd') => app.push_info("  (deferred)"),
            (_, other) => app.push_info(format!("  unknown action {other:?}")),
        },
        Candidate::OrphanConflict(_) => {
            app.push_info(format!("  (acknowledged: {c})"));
        }
    }
}

fn persist_skill(s: &SkillCandidate) -> Result<std::path::PathBuf> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut extra = serde_yaml::Mapping::new();
    extra.insert(
        serde_yaml::Value::String("source".into()),
        serde_yaml::Value::String("reflection".into()),
    );
    extra.insert(
        serde_yaml::Value::String("confidence".into()),
        serde_yaml::Value::String(format!("{:?}", s.confidence).to_lowercase()),
    );
    let fm = SkillFrontmatter {
        name: s.name.clone(),
        description: s.description.clone(),
        triggers: s.triggers.clone(),
        version: Some("0.1.0".into()),
        license: None,
        extra,
    };
    store
        .put(SkillScope::User, fm, &s.body)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn persist_memory(c: &MemoryCandidate) -> Result<std::path::PathBuf> {
    let store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut fm = MemoryFrontmatter::new(MemorySource::Reflection, c.confidence, c.tags.clone());
    fm.supersedes = c.supersedes.clone();
    let scope = match c.scope {
        MemoryScope::User => MemoryScope::User,
        MemoryScope::Project => MemoryScope::Project,
    };
    store.put(scope, fm, &c.fact).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Re-read the memory store after a write so subsequent /memory and
/// /context commands reflect what just landed.
fn refresh_memories(app: &mut App) {
    if let Ok(store) = FsMemoryStore::standard() {
        if let Ok(active) = store.list_active() {
            app.pinned_memories =
                active.iter().filter(|m| m.frontmatter.pinned).cloned().collect();
            app.status.memories = active.len();
            app.status.pinned = app.pinned_memories.len();
            app.active_memories = active;
        }
    }
}
