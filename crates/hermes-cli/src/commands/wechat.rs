//! `hermes wechat` — WeChat (iLink Bot) bridge.
//!
//! Two subcommands:
//!   - `login` — render a terminal QR; user scans it in WeChat; we persist
//!     the resulting `bot_token` to `~/.small-rust-hermes/wechat.toml`.
//!   - `run`   — long-poll the iLink Bot endpoint and reply to inbound text
//!     messages via `hermes_turn::run_turn`. Each WeChat user gets their own
//!     session JSONL under `~/.small-rust-hermes/sessions/wechat/{user_id}/`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use hermes_core::{
    ContentBlock, LlmProvider, Message, Role, SessionEvent, SessionMeta, ToolHost, ToolSpec,
};
use hermes_llm::{Config, ContextLimits};
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryEffectiveness, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillEffectiveness, SkillStore};
use hermes_store::SessionWriter;
use hermes_turn::{PermissionChecker, TurnConfig, TurnEvent};
use hermes_weixin::auth::{LoginSession, QrPollState, StoredCreds};
use hermes_weixin::client::{Client as WxClient, DEFAULT_BASE_URL};
use hermes_weixin::types::WeixinMessage;

use super::chat::{auto_install_palace_skill, compose_system_prompt, inject_time_header};
use super::context::ContextSources;
use super::util::{build_active_provider, load_tool_host};

// ===== login =================================================================

pub async fn login() -> Result<()> {
    let mut session = LoginSession::start(DEFAULT_BASE_URL)
        .await
        .context("starting QR login")?;

    println!("📱 用微信扫描下面的二维码：\n");
    println!("{}", session.render_terminal()?);
    println!("等待确认中… (Ctrl-C 中止)\n");

    let creds = session
        .await_confirmation(3, |state| match state {
            QrPollState::Waiting => {}
            QrPollState::Scanned => eprintln!("  已扫码，请在手机上确认…"),
            QrPollState::Confirmed(_) => eprintln!("  已确认。"),
            QrPollState::Refreshed(qr) => {
                eprintln!("  二维码已过期，已刷新：\n");
                println!("{qr}");
            }
        })
        .await
        .context("awaiting QR confirmation")?;

    let path = StoredCreds::default_path()?;
    creds
        .save(&path)
        .with_context(|| format!("saving creds to {}", path.display()))?;

    if let Some(bot) = &creds.bot_id {
        println!(
            "✓ 登录成功 (bot_id={bot})。Token 已保存到 {}",
            path.display()
        );
    } else {
        println!("✓ 登录成功。Token 已保存到 {}", path.display());
    }
    println!("接着执行: hermes wechat run");
    Ok(())
}

// ===== run ===================================================================

/// Path where we persist the long-poll cursor between `wechat run` invocations.
fn cursor_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    Ok(home.join(".small-rust-hermes").join("wechat-cursor.txt"))
}

/// Directory where per-WeChat-user sessions live.
fn user_session_dir(user_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    let safe = sanitize_user_id(user_id);
    Ok(home
        .join(".small-rust-hermes")
        .join("sessions")
        .join("wechat")
        .join(safe))
}

/// WeChat user ids are opaque tokens; strip anything that's not a safe path char.
fn sanitize_user_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn read_cursor() -> String {
    cursor_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn save_cursor(buf: &str) {
    if let Ok(p) = cursor_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&p, buf) {
            tracing::warn!(error = %e, path = %p.display(), "saving cursor failed");
        }
    }
}

/// In-memory state for one WeChat user: their session writer and message history.
struct UserState {
    history: Vec<Message>,
    writer: SessionWriter,
}

impl UserState {
    fn new(user_id: &str, model: &str, provider: &str) -> Result<Self> {
        let dir = user_session_dir(user_id)?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating {}", dir.display()))?;
        let meta = SessionMeta::new(model, provider);
        let stamp = meta.created_at.format("%Y-%m-%dT%H-%M-%S");
        let short = &meta.id[..8.min(meta.id.len())];
        let path = dir.join(format!("{stamp}-{short}.jsonl"));
        let mut writer = SessionWriter::create(&path)
            .with_context(|| format!("creating session {}", path.display()))?;
        writer
            .append(&SessionEvent::Meta(meta))
            .context("writing meta line")?;
        Ok(Self {
            history: Vec::new(),
            writer,
        })
    }
}

pub async fn run() -> Result<()> {
    // ----- credentials & WeChat client ---------------------------------
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)
        .with_context(|| format!("reading {}", creds_path.display()))?
        .ok_or_else(|| {
            anyhow!(
                "no WeChat credentials at {} — run `hermes wechat login` first",
                creds_path.display()
            )
        })?;
    let wx = WxClient::with_token(creds.base_url.clone(), creds.bot_token.clone())
        .context("building WeChat client")?;
    eprintln!(
        "✓ WeChat client ready{}",
        creds
            .bot_id
            .as_deref()
            .map(|b| format!(" (bot_id={b})"))
            .unwrap_or_default()
    );

    // ----- Hermes config, provider --------------------------------------
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;
    let provider_name = provider.name().to_string();
    let model = provider_cfg.model.clone();
    let workspace_root = cfg.workspace.root.clone();

    // ----- memory store (passed to host so memory_* / palace_* tools work)
    let memory_store_arc: Arc<dyn MemoryStore> = Arc::new(
        FsMemoryStore::standard().map_err(|e| anyhow!("memory store: {e}"))?,
    );
    let skill_store_arc: Arc<FsSkillStore> = Arc::new(
        FsSkillStore::standard().map_err(|e| anyhow!("skill store: {e}"))?,
    );
    auto_install_palace_skill(skill_store_arc.as_ref());

    // ----- tool host (memory + skill tools enabled), then whitelist ------
    let host = load_tool_host(
        &workspace_root,
        Some(memory_store_arc.clone()),
        Some(skill_store_arc.clone() as Arc<dyn SkillStore>),
        None,
    )
    .await?;
    let all_tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow!("listing tools: {e}"))?;
    // Whitelist: tools the bot can safely run from WeChat (no confirmation
    // UI is available over chat). Skill discovery/activation tools are on
    // the list so the bot can answer "what skills do I have?" and actually
    // load a skill before acting on it; `skill_create` is `requires_confirmation`
    // so it's filtered out below regardless of being listed here.
    let allow_names: &[&str] = &[
        "web_search",
        "web_fetch",
        "memory_search",
        "memory_save",
        "memory_delete",
        "palace_zones",
        "palace_read_zone",
        "palace_recall",
        "skill_list",
        "skill_read",
        "skill_read_file",
        "skill_create",
        "think",
    ];
    let tools: Vec<ToolSpec> = all_tools
        .into_iter()
        .filter(|t| !t.requires_confirmation && allow_names.contains(&t.name.as_str()))
        .collect();
    eprintln!("✓ tools ready: {} whitelisted", tools.len());

    // ----- memory + skills snapshot (mirror `chat` startup) -------------
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
        if compiled_profile.is_some() { "✓" } else { "—" },
    );
    eprintln!("skills:   {} loaded", all_skills.len());

    // ----- base TurnConfig (system filled per turn) ---------------------
    let base_system = compose_system_prompt(None, &workspace_root);
    let base_turn_cfg = TurnConfig {
        model: model.clone(),
        system: None,
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
    };

    let ctx = Arc::new(RunCtx {
        wx,
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
    });

    // ----- ctrl-c handling ---------------------------------------------
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let s = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\n收到 Ctrl-C，准备退出…");
                s.store(true, Ordering::SeqCst);
            }
        });
    }

    // ----- main loop ---------------------------------------------------
    let mut cursor = read_cursor();
    let mut users: HashMap<String, UserState> = HashMap::new();
    eprintln!(
        "📡 监听中（cursor={} bytes）。在微信里给 bot 发消息即可对话。",
        cursor.len()
    );

    while !shutdown.load(Ordering::SeqCst) {
        let resp = match ctx.wx.get_updates(&cursor).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("ret=-14") || msg.contains("token expired") {
                    bail!("token 已失效，请重新运行 `hermes wechat login`");
                }
                tracing::warn!(error = %msg, "getupdates failed; retrying in 3s");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        cursor = resp.get_updates_buf.clone();
        save_cursor(&cursor);

        for msg in resp.msgs {
            if let Err(e) = handle_inbound(ctx.as_ref(), &mut users, msg).await {
                tracing::warn!(error = format!("{e:#}"), "handling inbound message failed");
            }
        }
    }

    eprintln!("✓ 已退出。cursor 已保存。");
    Ok(())
}

/// Snapshot of everything the WeChat agent needs to assemble per-turn
/// context. Mirrors what `hermes chat` carries on the stack — loaded once
/// at startup, reused for every inbound message.
struct RunCtx {
    wx: WxClient,
    provider: Arc<dyn LlmProvider>,
    host: Arc<dyn ToolHost>,
    tools: Vec<ToolSpec>,
    base_turn_cfg: TurnConfig,
    model: String,
    provider_name: String,
    base_system: Option<String>,
    palace_index: Option<String>,
    compiled_profile: Option<String>,
    always_active_skills: Vec<LoadedSkill>,
    pinned_memories: Vec<LoadedMemory>,
    active_memories: Vec<LoadedMemory>,
    all_skills: Vec<LoadedSkill>,
    skill_effectiveness: HashMap<String, SkillEffectiveness>,
    memory_effectiveness: HashMap<String, MemoryEffectiveness>,
    limits: ContextLimits,
}

impl RunCtx {
    fn turn_system_for(&self, query: &str) -> String {
        let always_active_refs: Vec<&LoadedSkill> = self.always_active_skills.iter().collect();
        let sources = ContextSources {
            base: self.base_system.as_deref(),
            palace_index: self.palace_index.as_deref(),
            compiled_profile: self.compiled_profile.as_deref(),
            always_active_skills: &always_active_refs,
            pinned: &self.pinned_memories,
            active: &self.active_memories,
            all_skills: &self.all_skills,
            effectiveness: Some(&self.skill_effectiveness),
            memory_effectiveness: Some(&self.memory_effectiveness),
            limits: self.limits,
        };
        sources.build_turn_system(query)
    }
}

async fn handle_inbound(
    ctx: &RunCtx,
    users: &mut HashMap<String, UserState>,
    inbound: WeixinMessage,
) -> Result<()> {
    let from = inbound.from_user_id.clone();
    if from.is_empty() {
        return Ok(());
    }

    let Some(text) = inbound.first_text() else {
        // Non-text payload (image/voice/file/video). MVP: politely decline.
        let reply = WeixinMessage::reply_text(&inbound, "目前只支持文本消息。");
        if let Err(e) = ctx.wx.send_message(reply).await {
            tracing::warn!(error = %e, "send_message (non-text refusal) failed");
        }
        return Ok(());
    };
    let text = text.to_string();
    eprintln!("📩 {from}: {text}");

    // Look up / create per-user session.
    let state = match users.get_mut(&from) {
        Some(s) => s,
        None => {
            let s = UserState::new(&from, &ctx.model, &ctx.provider_name)?;
            users.insert(from.clone(), s);
            users.get_mut(&from).unwrap()
        }
    };

    // Append the user message to history + writer.
    let user_msg = Message::user_text(text.clone());
    state.history.push(user_msg.clone());
    if let Err(e) = state.writer.append(&SessionEvent::Message(user_msg)) {
        tracing::warn!(error = %e, "persisting user message failed");
    }

    // Per-turn system prompt: workspace clause + palace index / profile /
    // pinned memories / always-active skills + skills matched for *this*
    // user input. Same assembly path as `hermes chat`, so the bot has the
    // same picture of who the user is.
    let turn_system = ctx.turn_system_for(&text);
    let mut turn_cfg = ctx.base_turn_cfg.clone();
    turn_cfg.system = Some(turn_system);

    // Prepend a current-time header to the last user message (won't be
    // persisted to the session log — only sent to the model).
    let history_for_turn = inject_time_header(state.history.clone());

    // Channel for streaming tool-call summaries back to WeChat.
    let (tool_tx, mut tool_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let inbound_for_echo = inbound.clone();
    let wx_for_echo = ctx.wx.clone();
    let echo_task = tokio::spawn(async move {
        // Coalesce bursts: collect for up to ~400ms then flush one message.
        loop {
            let Some(first) = tool_rx.recv().await else {
                break;
            };
            let mut buf = vec![first];
            let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
            while let Ok(Some(more)) =
                tokio::time::timeout_at(deadline, tool_rx.recv()).await
            {
                buf.push(more);
            }
            let joined = buf
                .into_iter()
                .map(|s| format!("🔧 {s}"))
                .collect::<Vec<_>>()
                .join("\n");
            let echo = WeixinMessage::reply_text(&inbound_for_echo, joined);
            if let Err(e) = wx_for_echo.send_message(echo).await {
                tracing::warn!(error = %e, "tool-echo send failed");
            }
        }
    });

    let on_event = {
        let tx = tool_tx.clone();
        move |ev: TurnEvent| {
            if let TurnEvent::ToolExecStart { summary, .. } = ev {
                let _ = tx.send(summary);
            }
        }
    };

    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let output = hermes_turn::run_turn(
        ctx.provider.as_ref(),
        ctx.host.as_ref(),
        &ctx.tools,
        &history_for_turn,
        &turn_cfg,
        None,
        on_event,
        cancel_rx,
    )
    .await
    .map_err(|e| anyhow!("{e}"))?;

    // Drop the tool sender so the echo task drains and exits.
    drop(tool_tx);
    let _ = echo_task.await;

    // Append all turn messages to history + writer.
    for m in &output.new_messages {
        state.history.push(m.clone());
        if let Err(e) = state.writer.append(&SessionEvent::Message(m.clone())) {
            tracing::warn!(error = %e, "persisting assistant message failed");
        }
    }

    // Collect final assistant text and send back to WeChat.
    let reply_text: String = output
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

    if reply_text.is_empty() {
        tracing::warn!("turn produced no assistant text");
        return Ok(());
    }
    eprintln!("🤖 → {from}: {}", reply_text.lines().next().unwrap_or(""));
    let reply = WeixinMessage::reply_text(&inbound, reply_text);
    ctx.wx.send_message(reply).await.context("sending reply")?;
    Ok(())
}
