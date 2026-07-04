//! Shared bridge layer for chat-style channels (WeChat / Feishu / Telegram).
//!
//! Each channel's command module (`wechat.rs`, `feishu.rs`, `telegram.rs`)
//! owns only its protocol specifics — how it receives inbound messages and
//! how it sends text back. Everything else (provider/host/tools wiring,
//! per-turn context assembly, the tool-call echo loop, the `run_turn` call,
//! session persistence) lives here and is identical across channels.
//!
//! The seam between "shared" and "protocol-specific" is the [`Channel`]
//! trait: a channel provides a cheap-to-clone reply handle ([`Channel::Reply`])
//! and a `send` that knows how to deliver text to that handle. The driver
//! ([`serve_inbound`]) is generic over `C: Channel`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use hermes_core::{ContentBlock, LlmProvider, Message, Role, SessionEvent, SessionMeta, ToolHost, ToolSpec};
use hermes_llm::{Config, ContextLimits};
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryEffectiveness, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillEffectiveness, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::SubagentContext;
use hermes_turn::{PermissionChecker, TurnConfig, TurnEvent};

use super::chat::{
    auto_install_find_skills_skill, auto_install_palace_skill, auto_install_skill_creator_skill,
    compose_system_prompt, inject_time_header,
};
use super::context::ContextSources;
use super::util::{build_active_provider, build_web_ctx, load_tool_host};

/// Tools a chat channel may invoke. The whitelist IS the safety boundary:
/// every entry is hand-vetted for a surface with no confirmation UI. Tools
/// marked `requires_confirmation: true` are auto-approved because
/// `serve_inbound` passes `confirm_tx: None`; each writer tool enforces its
/// own path/quota guards.
pub const CHAT_TOOL_WHITELIST: &[&str] = &[
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

// ===== Channel trait ========================================================

/// Outbound capability of one chat channel: it knows how to deliver text to a
/// specific conversation, addressed by a cheap-to-clone [`Reply`] handle.
///
/// - WeChat: `Reply = WeixinMessage` (the inbound message — replies must echo
///   its `context_token` / `client_id`, so the handle is the inbound itself).
/// - Feishu: `Reply = String` (the sender's `open_id`).
/// - Telegram: `Reply = i64` (the `chat_id`).
#[async_trait]
pub trait Channel: Send + Sync {
    type Reply: Send + Sync + Clone + 'static;

    /// Channel name, used as the per-user session directory:
    /// `~/.small-rust-hermes/sessions/{name}/{user_id}/`.
    fn name(&self) -> &str;

    /// Send `text` to the conversation identified by `reply`. Used for both
    /// the final assistant reply and the streaming tool-call echo.
    async fn send(&self, reply: &Self::Reply, text: &str) -> Result<()>;
}

// ===== ServeCtx =============================================================

/// Snapshot of everything a chat agent needs to assemble per-turn context —
/// built once at startup, reused for every inbound message. Mirrors what
/// `hermes chat` carries on the stack.
pub struct ServeCtx {
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

impl ServeCtx {
    /// Load the default config and build the serve context.
    pub async fn build() -> Result<Arc<Self>> {
        let cfg = Config::load_default()
            .context("loading config from ~/.small-rust-hermes/config.toml")?;
        Self::build_from(&cfg).await
    }

    /// Build from an already-loaded config.
    pub async fn build_from(cfg: &Config) -> Result<Arc<Self>> {
        let provider_cfg = cfg.active_provider()?.clone();
        let provider = build_active_provider(cfg)?;
        let provider_name = provider.name().to_string();
        let model = provider_cfg.model.clone();
        let workspace_root = cfg.workspace.root.clone();

        let memory_store_arc: Arc<dyn MemoryStore> = Arc::new(
            FsMemoryStore::standard().map_err(|e| anyhow!("memory store: {e}"))?,
        );
        let skill_store_arc: Arc<FsSkillStore> =
            Arc::new(FsSkillStore::standard().map_err(|e| anyhow!("skill store: {e}"))?);
        auto_install_palace_skill(skill_store_arc.as_ref());
        auto_install_skill_creator_skill(skill_store_arc.as_ref());
        auto_install_find_skills_skill(skill_store_arc.as_ref());

        // Wire up `subagent` so channel-driven skill authoring can do real
        // evals (skill-creator's grader needs fresh child contexts).
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
            Some(build_web_ctx(cfg, provider.clone())),
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

        Ok(Arc::new(Self {
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

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Build the per-turn system prompt for `query`.
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

// ===== UserState ============================================================

/// In-memory state for one channel user: their message history and the JSONL
/// session writer.
pub struct UserState {
    pub history: Vec<Message>,
    pub writer: SessionWriter,
}

impl UserState {
    pub fn new(channel: &str, user_id: &str, model: &str, provider: &str) -> Result<Self> {
        let dir = user_session_dir(channel, user_id)?;
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

/// Per-user session directory:
/// `~/.small-rust-hermes/sessions/{channel}/{sanitized_user_id}/`.
fn user_session_dir(channel: &str, user_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    let safe = sanitize_user_id(user_id);
    Ok(home
        .join(".small-rust-hermes")
        .join("sessions")
        .join(channel)
        .join(safe))
}

/// Channel user ids are opaque tokens; strip anything that's not a safe path char.
fn sanitize_user_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ===== serve_inbound ========================================================

/// Drive one inbound message through a full turn: append to history, assemble
/// the per-turn system prompt, stream tool-call summaries back via the echo
/// task, run the agent turn, persist the result, and send the final reply.
///
/// The caller (each channel's run loop) is responsible for receiving the raw
/// inbound, parsing out `(user_id, text, reply)`, looking up / creating the
/// [`UserState`], and printing the inbound log line. How that map is guarded
/// (plain `&mut HashMap` for poll loops, `Arc<Mutex<…>>` for callback-driven
/// channels) is channel-specific and stays out of this function.
///
/// NOTE: when the caller holds the map under a `Mutex`, the guard spans this
/// `await` (one turn at a time per process). That matches the prior per-channel
/// behavior; relaxing it (clone history out of the lock) is a future change.
pub async fn serve_inbound<C>(
    ctx: &ServeCtx,
    channel: &C,
    state: &mut UserState,
    user_id: &str,
    text: &str,
    reply: C::Reply,
) -> Result<()>
where
    C: Channel + Clone + Send + Sync + 'static,
{
    // Append the user message to history + writer.
    let user_msg = Message::user_text(text.to_string());
    state.history.push(user_msg.clone());
    if let Err(e) = state.writer.append(&SessionEvent::Message(user_msg)) {
        tracing::warn!(error = %e, "persisting user message failed");
    }

    // Per-turn system prompt.
    let turn_system = ctx.turn_system_for(text);
    let mut turn_cfg = ctx.base_turn_cfg.clone();
    turn_cfg.system = Some(turn_system);

    // Prepend a current-time header to the last user message (model-only;
    // not persisted to the session log).
    let history_for_turn = inject_time_header(state.history.clone());

    // Channel for streaming tool-call summaries back to the user.
    let (tool_tx, mut tool_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let chan_for_echo = channel.clone();
    let reply_for_echo = reply.clone();
    let echo_task = tokio::spawn(async move {
        // Coalesce bursts: collect for up to ~400ms then flush one message.
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
            if let Err(e) = chan_for_echo.send(&reply_for_echo, &joined).await {
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

    // Collect final assistant text and send back to the user.
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
        "🤖 → {user_id}: {}",
        reply_text.lines().next().unwrap_or("")
    );
    channel
        .send(&reply, &reply_text)
        .await
        .context("sending reply")?;
    Ok(())
}
