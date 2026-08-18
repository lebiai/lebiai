//! Shared chat-channel driver (hermes-channel): Channel trait, ServeCtx, per-user
//! session persistence and the inbound-turn driver. Surfaces (CLI / GUI / IM)
//! implement only protocol specifics and call [`serve_inbound`].
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

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use hermes_core::{
    ContentBlock, LlmProvider, Message, Role, SessionEvent, SessionMeta, ToolHost, ToolSpec,
};
use hermes_llm::ContextLimits;
use hermes_memory::{LoadedMemory, MemoryEffectiveness};
use hermes_skills::{LoadedSkill, SkillEffectiveness};
use hermes_store::SessionWriter;
use hermes_turn::{TurnConfig, TurnEvent};

use crate::context::ContextSources;
use crate::system_prompt::inject_time_header;

/// Tools for **IM surfaces** (WeChat / Feishu / Telegram): no confirmation UI.
/// Durable writes (`memory_save` / `skill_create`) are excluded so untrusted
/// senders cannot poison long-term state even if allowlisted to chat.
pub const IM_TOOL_WHITELIST: &[&str] = &[
    "web_search",
    "web_fetch",
    "memory_search",
    "palace_zones",
    "palace_read_zone",
    "palace_recall",
    "skill_list",
    "skill_read",
    "skill_read_file",
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
    /// `~/.lebi-ai/sessions/{name}/{user_id}/`.
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
    pub provider: Arc<dyn LlmProvider>,
    pub host: Arc<dyn ToolHost>,
    pub tools: Vec<ToolSpec>,
    pub base_turn_cfg: TurnConfig,
    pub model: String,
    pub provider_name: String,
    pub base_system: Option<String>,
    pub palace_index: Option<String>,
    pub compiled_profile: Option<String>,
    pub always_active_skills: Vec<LoadedSkill>,
    pub pinned_memories: Vec<LoadedMemory>,
    pub active_memories: Vec<LoadedMemory>,
    pub all_skills: Vec<LoadedSkill>,
    pub skill_effectiveness: HashMap<String, SkillEffectiveness>,
    pub memory_effectiveness: HashMap<String, MemoryEffectiveness>,
    pub limits: ContextLimits,
}

impl ServeCtx {
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
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
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
/// `~/.lebi-ai/sessions/{channel}/{sanitized_user_id}/`.
fn user_session_dir(channel: &str, user_id: &str) -> Result<PathBuf> {
    let safe = sanitize_user_id(user_id);
    Ok(hermes_core::data_path("sessions").join(channel).join(safe))
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

// ===== handle_text_message ==================================================

/// Handle one inbound text message under a shared per-user map: look up or
/// create the user's session, then drive a full turn via [`serve_inbound`].
///
/// Shared by CLI and GUI so per-user session bookkeeping stays single-source
/// (sessions live under `~/.lebi-ai/sessions/{channel}/{user_id}/`).
pub async fn handle_text_message<C>(
    ctx: &ServeCtx,
    channel: &C,
    users: &mut HashMap<String, UserState>,
    user_id: &str,
    text: String,
    reply: C::Reply,
) -> Result<()>
where
    C: Channel + Clone + Send + Sync + 'static,
{
    let state = match users.get_mut(user_id) {
        Some(s) => s,
        None => {
            let s = UserState::new(channel.name(), user_id, ctx.model(), ctx.provider_name())?;
            users.insert(user_id.to_string(), s);
            users.get_mut(user_id).unwrap()
        }
    };
    serve_inbound(ctx, channel, state, user_id, &text, reply).await
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
    // Sender allowlist (fail-closed). Config: ~/.lebi-ai/channel-allowlist.toml
    if !crate::access::is_sender_allowed(channel.name(), user_id) {
        tracing::warn!(
            channel = channel.name(),
            user_id,
            "IM sender not in channel-allowlist; denied"
        );
        let msg = crate::access::deny_message(channel.name());
        if let Err(e) = channel.send(&reply, &msg).await {
            tracing::warn!(error = %e, "sending allowlist denial failed");
        }
        return Ok(());
    }

    // Append the user message to history + writer.
    let user_msg = Message::user_sent(text.to_string());
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
    let history_for_turn =
        inject_time_header(hermes_core::sanitize_history_for_provider(&state.history));

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

    // Append turn messages. Care / time-header nudges stay off disk and
    // out of the next turn's history (same contract as GUI).
    for m in &output.new_messages {
        let to_disk = m.for_persist(false);
        if to_disk.content.is_empty() {
            continue;
        }
        state.history.push(to_disk.clone());
        if let Err(e) = state.writer.append(&SessionEvent::Message(to_disk)) {
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

#[cfg(test)]
mod whitelist_tests {
    use super::IM_TOOL_WHITELIST;

    #[test]
    fn im_cannot_open_or_write() {
        for denied in [
            "open",
            "write",
            "edit",
            "bash",
            "memory_save",
            "skill_create",
        ] {
            assert!(
                !IM_TOOL_WHITELIST.contains(&denied),
                "IM must not expose {denied}"
            );
        }
        assert!(IM_TOOL_WHITELIST.contains(&"web_search"));
        assert!(IM_TOOL_WHITELIST.contains(&"think"));
    }
}
