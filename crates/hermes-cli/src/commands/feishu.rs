//! `hermes feishu` — Feishu (Lark) long-connection bridge.
//!
//! Two subcommands:
//!   - `auth` — validate app_id/app_secret and persist them to
//!     `~/.small-rust-hermes/feishu.toml`.
//!   - `run`  — establish WS long-connection, receive events, reply to
//!     inbound text messages via `hermes_turn::run_turn`. Each Feishu user
//!     gets their own session JSONL under
//!     `~/.small-rust-hermes/sessions/feishu/{user_id}/`.

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
use hermes_tools::SubagentContext;
use hermes_feishu::auth::StoredCreds;
use hermes_feishu::client::{EventPayload, FeishuClient, MessageReceiveEvent};

use super::chat::{
    auto_install_find_skills_skill, auto_install_palace_skill, auto_install_skill_creator_skill,
    compose_system_prompt, inject_time_header,
};
use super::context::ContextSources;
use super::util::{build_active_provider, build_web_ctx, load_tool_host};

// ===== auth =================================================================

pub async fn auth() -> Result<()> {
    // Interactive: prompt for app_id and app_secret
    println!("🔑 飞书应用凭证配置");
    println!("请在飞书开放平台 (https://open.feishu.cn) 创建应用，获取 App ID 和 App Secret。");
    println!();

    let app_id = prompt_secret("App ID")?;
    let app_secret = prompt_secret("App Secret")?;

    if app_id.is_empty() || app_secret.is_empty() {
        bail!("app_id 和 app_secret 不能为空");
    }

    // Validate by attempting to get a tenant_access_token
    println!("正在验证凭证…");
    let client = FeishuClient::new(&app_id, &app_secret);
    let token = client.get_tenant_token_for_validation().await?;

    let creds = StoredCreds {
        app_id,
        app_secret,
        domain: hermes_feishu::client::DEFAULT_DOMAIN.to_string(),
    };
    let path = StoredCreds::default_path()?;
    creds
        .save(&path)
        .with_context(|| format!("saving creds to {}", path.display()))?;

    println!(
        "✓ 凭证验证成功 (tenant_access_token 前8位: {}…)",
        &token[..8.min(token.len())]
    );
    println!("Token 已保存到 {}", path.display());
    println!("接着执行: hermes feishu run");
    Ok(())
}

/// Prompt for a secret value from stdin (no echo if possible).
fn prompt_secret(label: &str) -> Result<String> {
    eprint!("{label}: ");
    // Try rpassword for no-echo; fall back to plain stdin.
    match rpassword::read_password() {
        Ok(s) => Ok(s.trim().to_string()),
        Err(_) => {
            // Fallback: just read a line (visible, but works in non-tty)
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .context("reading from stdin")?;
            Ok(buf.trim().to_string())
        }
    }
}

// ===== run ==================================================================

/// Directory where per-Feishu-user sessions live.
fn user_session_dir(user_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    let safe = sanitize_user_id(user_id);
    Ok(home
        .join(".small-rust-hermes")
        .join("sessions")
        .join("feishu")
        .join(safe))
}

fn sanitize_user_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// In-memory state for one Feishu user: their session writer and message history.
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
    // ----- credentials & Feishu client ---------------------------------
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)
        .with_context(|| format!("reading {}", creds_path.display()))?
        .ok_or_else(|| {
            anyhow!(
                "no Feishu credentials at {} — run `hermes feishu auth` first",
                creds_path.display()
            )
        })?;

    let feishu = FeishuClient::with_domain(&creds.app_id, &creds.app_secret, &creds.domain);
    eprintln!("✓ Feishu client ready (app_id={}…)", &creds.app_id[..8.min(creds.app_id.len())]);

    // ----- Hermes config, provider --------------------------------------
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;
    let provider_name = provider.name().to_string();
    let model = provider_cfg.model.clone();
    let workspace_root = cfg.workspace.root.clone();

    // ----- memory store
    let memory_store_arc: Arc<dyn MemoryStore> = Arc::new(
        FsMemoryStore::standard().map_err(|e| anyhow!("memory store: {e}"))?,
    );
    let skill_store_arc: Arc<FsSkillStore> = Arc::new(
        FsSkillStore::standard().map_err(|e| anyhow!("skill store: {e}"))?,
    );
    auto_install_palace_skill(skill_store_arc.as_ref());
    auto_install_skill_creator_skill(skill_store_arc.as_ref());
    auto_install_find_skills_skill(skill_store_arc.as_ref());

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

    // ----- tool host
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
        "skill_install",
        "skill_delete",
        "subagent",
        "think",
    ];
    let tools: Vec<ToolSpec> = all_tools
        .into_iter()
        .filter(|t| allow_names.contains(&t.name.as_str()))
        .collect();
    eprintln!("✓ tools ready: {} whitelisted", tools.len());

    // ----- memory + skills snapshot
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

    // ----- base TurnConfig
    let base_system = compose_system_prompt(None, &workspace_root);
    let base_turn_cfg = TurnConfig {
        model: model.clone(),
        system: None,
        max_tokens: provider_cfg.max_tokens,
        max_tool_rounds: cfg.limits.max_tool_rounds,
        permissions: PermissionChecker::new(&cfg.permissions.allow, &cfg.permissions.deny),
    };

    let ctx = Arc::new(RunCtx {
        feishu,
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

    // ----- ctrl-c handling
    let shutdown = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    {
        let s = shutdown.clone();
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\n收到 Ctrl-C，准备退出…");
                s.store(true, Ordering::SeqCst);
                let _ = tx.send(()).await;
            }
        });
    }

    // ----- main loop: WS long-connection
    let users: Arc<tokio::sync::Mutex<HashMap<String, UserState>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let ctx_clone = ctx.clone();
    let users_clone = users.clone();
    let shutdown_clone = shutdown.clone();

    let on_event = Box::new(move |payload: EventPayload| {
        let ctx = ctx_clone.clone();
        let users = users_clone.clone();
        let shutdown = shutdown_clone.clone();

        tokio::spawn(async move {
            if shutdown.load(Ordering::SeqCst) {
                return;
            }
            if let Err(e) = handle_event(ctx.as_ref(), &users, payload).await {
                tracing::warn!(error = format!("{e:#}"), "handling Feishu event failed");
            }
        });
    });

    eprintln!("📡 正在连接飞书长连接…");
    if let Err(e) = ctx.feishu.start(on_event, shutdown_rx).await {
        if !shutdown.load(Ordering::SeqCst) {
            return Err(e);
        }
    }

    eprintln!("✓ 已退出。");
    Ok(())
}

/// Snapshot of everything the Feishu agent needs to assemble per-turn
/// context. Mirrors `RunCtx` in `wechat.rs`.
struct RunCtx {
    feishu: FeishuClient,
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

async fn handle_event(
    ctx: &RunCtx,
    users: &Arc<tokio::sync::Mutex<HashMap<String, UserState>>>,
    payload: EventPayload,
) -> Result<()> {
    // Only handle im.message.receive_v1 events
    let header = payload.header.as_ref();
    let event_type = header.and_then(|h| h.event_type.as_deref()).unwrap_or("");
    if event_type != "im.message.receive_v1" {
        return Ok(());
    }

    let event_data = payload.event.as_ref().ok_or_else(|| anyhow!("event body missing"))?;
    let msg_event: MessageReceiveEvent = serde_json::from_value(event_data.clone())
        .context("parsing im.message.receive_v1")?;

    // Only handle text messages
    if msg_event.message.message_type != "text" {
        let open_id = msg_event.sender.sender_id.open_id.as_deref().unwrap_or("unknown");
        let _ = ctx.feishu.send_text(open_id, "目前只支持文本消息。").await;
        return Ok(());
    }

    // Extract text content
    let text_content: serde_json::Value =
        serde_json::from_str(&msg_event.message.content).unwrap_or_default();
    let text = text_content
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        return Ok(());
    }

    let open_id = msg_event
        .sender
        .sender_id
        .open_id
        .as_deref()
        .unwrap_or("unknown");
    let chat_id = &msg_event.message.chat_id;
    let message_id = &msg_event.message.message_id;

    eprintln!("📩 {open_id} (chat={chat_id}): {text}");

    // Look up / create per-user session
    let mut users_guard = users.lock().await;
    let state = match users_guard.get_mut(open_id) {
        Some(s) => s,
        None => {
            let s = UserState::new(open_id, &ctx.model, &ctx.provider_name)?;
            users_guard.insert(open_id.to_string(), s);
            users_guard.get_mut(open_id).unwrap()
        }
    };

    // Append the user message to history + writer
    let user_msg = Message::user_text(text.clone());
    state.history.push(user_msg.clone());
    if let Err(e) = state.writer.append(&SessionEvent::Message(user_msg)) {
        tracing::warn!(error = %e, "persisting user message failed");
    }

    // Per-turn system prompt
    let turn_system = ctx.turn_system_for(&text);
    let mut turn_cfg = ctx.base_turn_cfg.clone();
    turn_cfg.system = Some(turn_system);

    let history_for_turn = inject_time_header(state.history.clone());

    // Channel for streaming tool-call summaries back to Feishu
    let (tool_tx, mut tool_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let feishu_for_echo = ctx.feishu.clone();
    let open_id_for_echo = open_id.to_string();
    let echo_task = tokio::spawn(async move {
        loop {
            let Some(first) = tool_rx.recv().await else {
                break;
            };
            let mut buf = vec![first];
            let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
            while let Ok(Some(more)) = tokio::time::timeout_at(deadline, tool_rx.recv()).await {
                buf.push(more);
            }
            let joined = buf
                .into_iter()
                .map(|s| format!("🔧 {s}"))
                .collect::<Vec<_>>()
                .join("\n");
            if let Err(e) = feishu_for_echo.send_text(&open_id_for_echo, &joined).await {
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

    // Drop the tool sender so the echo task drains and exits
    drop(tool_tx);
    let _ = echo_task.await;

    // Append all turn messages to history + writer
    for m in &output.new_messages {
        state.history.push(m.clone());
        if let Err(e) = state.writer.append(&SessionEvent::Message(m.clone())) {
            tracing::warn!(error = %e, "persisting assistant message failed");
        }
    }

    // Collect final assistant text and send back to Feishu
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

    eprintln!(
        "🤖 → {open_id}: {}",
        reply_text.lines().next().unwrap_or("")
    );
    ctx.feishu
        .send_text(open_id, &reply_text)
        .await
        .context("sending reply")?;

    let _ = (chat_id, message_id); // available for future "reply in thread" support
    Ok(())
}
