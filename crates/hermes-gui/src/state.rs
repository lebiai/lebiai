use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use hermes_channel::{compose_system_prompt, ServeCtx, UserState, CHAT_TOOL_WHITELIST};
use hermes_core::{LlmProvider, Session, SessionMeta, ToolHost, ToolSpec};
use hermes_llm::Config;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryEffectiveness, MemoryStore};
use hermes_reflect::SkillCandidate;
use hermes_skills::{FsSkillStore, LoadedSkill, SkillEffectiveness, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::{
    BuiltinToolHost, CompositeToolHost, ProposeContext, SearchBackend, WebToolsContext,
};
use hermes_turn::{PermissionChecker, TurnConfig};
use tokio::sync::Mutex;

pub type Sessions = Arc<Mutex<HashMap<String, ActiveSession>>>;
pub type CancelTokens = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>;
pub type ConfirmTokens =
    Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<hermes_turn::ConfirmAction>>>>;
/// Session-scoped allowlist populated when the user clicks "Always Allow" on
/// a tool confirmation. Lives only for the lifetime of the GUI process; to
/// persist allow rules, the user edits `config.toml` directly.
pub type AlwaysAllowedTools = Arc<Mutex<HashSet<String>>>;
pub type ProposeMessages = Arc<RwLock<Vec<hermes_core::Message>>>;
pub type ProposeQueue = Arc<std::sync::Mutex<Vec<SkillCandidate>>>;

/// WeChat (iLink Bot) connection state for the GUI surface.
#[allow(dead_code)]
pub struct WechatState {
    /// In-flight QR login session (`wechat_login_start` -> poll loop).
    pub login: Arc<Mutex<Option<hermes_weixin::auth::LoginSession>>>,
    /// Shared per-user handler state for inbound messages.
    pub serve_users: Arc<Mutex<HashMap<String, UserState>>>,
    /// Shutdown flag for the serve loop.
    pub shutdown: Arc<AtomicBool>,
    /// Serve task handle (Some while running).
    pub serve_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Status snapshot for the frontend.
    pub status: Arc<Mutex<WechatStatus>>,
}

/// Frontend-visible WeChat connection status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WechatStatus {
    /// "stopped" | "listening" | "token_expired" | "error"
    pub state: String,
    pub bot_id: Option<String>,
    pub last_error: Option<String>,
}

#[allow(dead_code)]
pub struct AppState {
    /// Active model provider. `update_config` hot-swaps this after saving so
    /// provider/API-key changes apply without restarting the app.
    pub provider: RwLock<Arc<dyn LlmProvider>>,
    pub host: Arc<dyn ToolHost>,
    /// Latest on-disk config; hot-swapped alongside `provider`.
    pub config: RwLock<Config>,
    pub skill_store: Arc<FsSkillStore>,
    /// Shared with BuiltinToolHost so agent tools (`memory_save` etc.) hit the same store.
    pub memory_store: Arc<FsMemoryStore>,
    pub sessions: Sessions,
    pub cancel_tokens: CancelTokens,
    pub confirm_tokens: ConfirmTokens,
    pub always_allowed_tools: AlwaysAllowedTools,
    pub tools: Mutex<Vec<ToolSpec>>,
    pub skills: Mutex<Vec<LoadedSkill>>,
    /// Shared so background micro-reflection can refresh context after auto-accept.
    pub pinned_memories: Arc<Mutex<Vec<LoadedMemory>>>,
    pub active_memories: Arc<Mutex<Vec<LoadedMemory>>>,
    pub propose_messages: ProposeMessages,
    pub propose_queue: ProposeQueue,
    /// Per-session turns since last micro-reflection (cooldown for periodic trigger).
    pub micro_turns_since: Arc<Mutex<HashMap<String, usize>>>,
    /// WeChat (iLink Bot) connection state.
    pub wechat: WechatState,
}

/// In-memory chat session. Disk is deferred until the first user message
/// so empty "New Chat" drafts never pollute the session list.
#[allow(dead_code)]
pub struct ActiveSession {
    pub session: Session,
    /// `None` until the first event is flushed (meta + first message).
    pub writer: Option<SessionWriter>,
    pub path: PathBuf,
}

impl ActiveSession {
    /// Create the JSONL file (with meta) if needed, then return the writer.
    pub fn ensure_writer(&mut self) -> Result<&mut SessionWriter, hermes_store::SessionError> {
        if self.writer.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| {
                    hermes_store::SessionError::Io {
                        path: parent.to_path_buf(),
                        source,
                    }
                })?;
            }
            let w = if self.path.exists() {
                SessionWriter::open_append(&self.path)?
            } else {
                let mut w = SessionWriter::create(&self.path)?;
                w.append(&hermes_core::SessionEvent::Meta(self.session.meta.clone()))?;
                w
            };
            self.writer = Some(w);
        }
        Ok(self.writer.as_mut().expect("writer present after ensure"))
    }
}

impl AppState {
    pub async fn init() -> Result<Self> {
        let config = Config::load_default_or_create().context("loading config.toml")?;
        let provider = config
            .build_active_provider()
            .context("building provider")?;

        let base = hermes_core::data_root();
        let workspace_root = base.join("workspace");
        std::fs::create_dir_all(&workspace_root)?;
        // Product default: all user-requested *generated* artifacts land here.
        std::fs::create_dir_all(workspace_root.join(hermes_core::WORKSPACE_OUTPUTS_DIR))?;

        // S1: move leftover lawyer-era top-level files out of the agent workspace.
        match hermes_core::quarantine_lawyer_workspace_files(&base, &workspace_root) {
            Ok(n) if n > 0 => {
                tracing::info!(moved = n, "quarantined lawyer workspace leftovers");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(%e, "workspace lawyer quarantine skipped"),
        }
        // S1: drop empty draft sessions (meta-only) from previous runs.
        if let Ok(n) = hermes_store::purge_empty_sessions(base.join("sessions")) {
            if n > 0 {
                tracing::info!(removed = n, "purged empty session drafts");
            }
        }

        let propose_messages: ProposeMessages = Arc::new(RwLock::new(Vec::new()));
        let propose_queue: ProposeQueue = Arc::new(std::sync::Mutex::new(Vec::new()));
        let propose_ctx = Arc::new(ProposeContext {
            provider: provider.clone(),
            messages: propose_messages.clone(),
            queue: propose_queue.clone(),
        });

        let skill_store: Arc<FsSkillStore> = Arc::new(FsSkillStore::new(base.join("skills"), None));
        // Same engine contract as the CLI: bundled meta-skills auto-install
        // at startup so every entry surface sees the same skill set.
        hermes_skills::bundled::auto_install_bundled(&skill_store);
        let memory_store: Arc<FsMemoryStore> =
            Arc::new(FsMemoryStore::new(base.join("memories"), None));

        let web_ctx = Arc::new(WebToolsContext {
            extract_provider: provider.clone(),
            extract_model: config.web.extract_model.clone(),
            extract_max_tokens: 2048,
            search_backend: SearchBackend::parse(&config.web.search_backend),
            tavily_api_key: config.web.tavily_api_key.clone(),
            brave_api_key: config.web.brave_api_key.clone(),
            cache_ttl_secs: config.web.cache_ttl_secs,
        });

        let host = load_tool_host(
            &workspace_root,
            Some(memory_store.clone() as Arc<dyn MemoryStore>),
            Some(skill_store.clone() as Arc<dyn SkillStore>),
            Some(propose_ctx),
            Some(web_ctx),
        )
        .await?;
        let tools = host.list_tools().await.unwrap_or_default();

        let skills = skill_store.list().unwrap_or_default();
        let all_memories = memory_store.list_active().unwrap_or_default();
        let pinned: Vec<LoadedMemory> = all_memories
            .iter()
            .filter(|m| m.frontmatter.pinned)
            .cloned()
            .collect();

        Ok(Self {
            provider: RwLock::new(provider),
            host,
            config: RwLock::new(config),
            skill_store,
            memory_store,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            confirm_tokens: Arc::new(Mutex::new(HashMap::new())),
            always_allowed_tools: Arc::new(Mutex::new(HashSet::new())),
            tools: Mutex::new(tools),
            skills: Mutex::new(skills),
            pinned_memories: Arc::new(Mutex::new(pinned)),
            active_memories: Arc::new(Mutex::new(all_memories)),
            propose_messages,
            propose_queue,
            micro_turns_since: Arc::new(Mutex::new(HashMap::new())),
            wechat: WechatState {
                login: Arc::new(Mutex::new(None)),
                serve_users: Arc::new(Mutex::new(HashMap::new())),
                shutdown: Arc::new(AtomicBool::new(false)),
                serve_task: Arc::new(Mutex::new(None)),
                status: Arc::new(Mutex::new(WechatStatus {
                    state: "stopped".to_string(),
                    bot_id: None,
                    last_error: None,
                })),
            },
        })
    }

    pub fn model(&self) -> String {
        self.config
            .read()
            .unwrap()
            .active_provider()
            .map(|p| p.model.clone())
            .unwrap_or_else(|_| {
                hermes_llm::PROVIDER_PRESETS
                    .first()
                    .map(|p| p.model.to_string())
                    .unwrap_or_default()
            })
    }

    pub fn max_tokens(&self) -> u32 {
        self.config
            .read()
            .unwrap()
            .active_provider()
            .map(|p| p.max_tokens)
            .unwrap_or(16_384)
    }

    pub fn workspace_root(&self) -> String {
        self.config
            .read()
            .unwrap()
            .workspace
            .root
            .to_string_lossy()
            .into_owned()
    }

    /// Build the shared channel [`ServeCtx`] from this GUI's engine wiring,
    /// filtered to the IM whitelist — single source with the CLI's channel
    /// driver (`hermes_channel`).
    pub async fn build_serve_ctx(&self) -> Result<Arc<ServeCtx>> {
        let provider_cfg = self.config.read().unwrap().active_provider()?.clone();
        let provider = self.provider.read().unwrap().clone();
        let workspace_root = self.config.read().unwrap().workspace.root.clone();
        let tools: Vec<ToolSpec> = self
            .tools
            .lock()
            .await
            .iter()
            .filter(|t| CHAT_TOOL_WHITELIST.contains(&t.name.as_str()))
            .cloned()
            .collect();
        let active_memories = self.active_memories.lock().await.clone();
        let pinned_memories = self.pinned_memories.lock().await.clone();
        let all_skills = self.skills.lock().await.clone();
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
        let base_system = compose_system_prompt(None, &workspace_root);
        let provider_name = provider.name().to_string();
        let (limits, allow_rules, deny_rules) = {
            let cfg = self.config.read().unwrap();
            (
                cfg.limits,
                cfg.permissions.allow.clone(),
                cfg.permissions.deny.clone(),
            )
        };
        let base_turn_cfg = TurnConfig {
            model: provider_cfg.model.clone(),
            system: None,
            max_tokens: provider_cfg.max_tokens,
            max_tool_rounds: limits.max_tool_rounds,
            permissions: PermissionChecker::new(&allow_rules, &deny_rules),
        };
        Ok(Arc::new(ServeCtx {
            provider,
            host: self.host.clone(),
            tools,
            base_turn_cfg,
            model: provider_cfg.model.clone(),
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
            limits,
        }))
    }
}

async fn load_tool_host(
    workspace_root: &Path,
    memory_store: Option<Arc<dyn MemoryStore>>,
    skill_store: Option<Arc<dyn SkillStore>>,
    propose_ctx: Option<Arc<ProposeContext>>,
    web_ctx: Option<Arc<WebToolsContext>>,
) -> Result<Arc<dyn ToolHost>> {
    let mut builtin = BuiltinToolHost::new(workspace_root.to_path_buf());
    if let Some(store) = memory_store {
        builtin = builtin.with_memory_store(store);
    }
    if let Some(store) = skill_store {
        builtin = builtin.with_skill_store(store);
    }
    if let Some(ctx) = propose_ctx {
        builtin = builtin.with_propose_ctx(ctx);
    }
    if let Some(ctx) = web_ctx {
        builtin = builtin.with_web_ctx(ctx);
    }

    let mut cfg = McpConfig::load_default().unwrap_or_else(|_| McpConfig {
        servers: Default::default(),
    });
    rewrite_filesystem_servers(&mut cfg, workspace_root);

    let mcp: Option<Box<dyn ToolHost>> = if cfg.servers.is_empty() {
        None
    } else {
        match McpToolHost::connect_all(&cfg).await {
            Ok(h) => Some(Box::new(h)),
            Err(e) => {
                tracing::warn!("MCP connect failed: {e}");
                None
            }
        }
    };

    Ok(Arc::new(CompositeToolHost { builtin, mcp }))
}

fn rewrite_filesystem_servers(cfg: &mut McpConfig, workspace_root: &Path) {
    for spec in cfg.servers.values_mut() {
        if let ServerSpec::Stdio { args, .. } = spec {
            let is_fs_server = args
                .iter()
                .any(|a| a.contains("@modelcontextprotocol/server-filesystem"));
            if !is_fs_server {
                continue;
            }
            let pkg_idx = args
                .iter()
                .position(|a| a.contains("@modelcontextprotocol/server-filesystem"));
            if let Some(idx) = pkg_idx {
                args.truncate(idx + 1);
                args.push(workspace_root.to_string_lossy().into_owned());
            }
        }
    }
}

pub fn session_path_for(meta: &SessionMeta) -> Result<PathBuf> {
    let stamp = meta.created_at.format("%Y-%m-%dT%H-%M-%S");
    let short_id = &meta.id[..8.min(meta.id.len())];
    Ok(hermes_core::data_path("sessions").join(format!("{stamp}-{short_id}.jsonl")))
}
