//! Shared application state for the Hermes server.
//!
//! 1:1 with `hermes-gui/src/state.rs` — same `AppState`, type aliases,
//! `init()`, and `load_tool_host()`. The only difference is no Tauri
//! dependency; this state lives behind axum extractors instead.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, Session, SessionMeta, ToolHost, ToolSpec};
use hermes_llm::Config;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_reflect::SkillCandidate;
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::{BuiltinToolHost, CompositeToolHost, ProposeContext, SearchBackend, WebToolsContext};
use tokio::sync::Mutex;

pub type Sessions = Arc<Mutex<HashMap<String, ActiveSession>>>;
pub type CancelTokens = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>;
pub type ConfirmTokens =
    Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<hermes_turn::ConfirmAction>>>>;
/// Session-scoped allowlist populated when the user clicks "Always Allow" on
/// a tool confirmation. Lives only for the lifetime of the server process;
/// to persist allow rules, the user edits `config.toml` directly.
pub type AlwaysAllowedTools = Arc<Mutex<HashSet<String>>>;
pub type ProposeMessages = Arc<RwLock<Vec<hermes_core::Message>>>;
pub type ProposeQueue = Arc<std::sync::Mutex<Vec<SkillCandidate>>>;

pub struct AppState {
    pub provider: Arc<dyn LlmProvider>,
    pub host: Arc<dyn ToolHost>,
    pub config: Config,
    pub skill_store: Arc<FsSkillStore>,
    pub memory_store: FsMemoryStore,
    pub sessions: Sessions,
    pub cancel_tokens: CancelTokens,
    pub confirm_tokens: ConfirmTokens,
    pub always_allowed_tools: AlwaysAllowedTools,
    pub tools: Mutex<Vec<ToolSpec>>,
    pub skills: Mutex<Vec<LoadedSkill>>,
    pub pinned_memories: Mutex<Vec<LoadedMemory>>,
    pub active_memories: Mutex<Vec<LoadedMemory>>,
    pub propose_messages: ProposeMessages,
    pub propose_queue: ProposeQueue,
}

pub struct ActiveSession {
    pub session: Session,
    pub writer: SessionWriter,
    pub path: PathBuf,
}

impl AppState {
    pub async fn init() -> Result<Self> {
        let config = Config::load_default_or_create().context("loading config.toml")?;
        let provider = config
            .build_active_provider()
            .context("building provider")?;

        let home = dirs::home_dir().context("resolving $HOME")?;
        let base = home.join(".small-rust-hermes");
        let workspace_root = base.join("workspace");
        std::fs::create_dir_all(&workspace_root)?;

        let propose_messages: ProposeMessages = Arc::new(RwLock::new(Vec::new()));
        let propose_queue: ProposeQueue = Arc::new(std::sync::Mutex::new(Vec::new()));
        let propose_ctx = Arc::new(ProposeContext {
            provider: provider.clone(),
            messages: propose_messages.clone(),
            queue: propose_queue.clone(),
        });

        let skill_store: Arc<FsSkillStore> = Arc::new(FsSkillStore::new(base.join("skills"), None));
        let memory_store = FsMemoryStore::new(base.join("memories"), None);

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
            provider,
            host,
            config,
            skill_store,
            memory_store,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            confirm_tokens: Arc::new(Mutex::new(HashMap::new())),
            always_allowed_tools: Arc::new(Mutex::new(HashSet::new())),
            tools: Mutex::new(tools),
            skills: Mutex::new(skills),
            pinned_memories: Mutex::new(pinned),
            active_memories: Mutex::new(all_memories),
            propose_messages,
            propose_queue,
        })
    }

    pub fn model(&self) -> &str {
        self.config
            .active_provider()
            .map(|p| p.model.as_str())
            .unwrap_or("claude-sonnet-4-20250514")
    }

    pub fn max_tokens(&self) -> u32 {
        self.config
            .active_provider()
            .map(|p| p.max_tokens)
            .unwrap_or(16_384)
    }

    pub fn workspace_root(&self) -> String {
        self.config.workspace.root.to_string_lossy().into_owned()
    }
}

async fn load_tool_host(
    workspace_root: &Path,
    skill_store: Option<Arc<dyn SkillStore>>,
    propose_ctx: Option<Arc<ProposeContext>>,
    web_ctx: Option<Arc<WebToolsContext>>,
) -> Result<Arc<dyn ToolHost>> {
    let mut builtin = BuiltinToolHost::new(workspace_root.to_path_buf());
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
    let home = dirs::home_dir().context("resolving $HOME")?;
    let stamp = meta.created_at.format("%Y-%m-%dT%H-%M-%S");
    let short_id = &meta.id[..8.min(meta.id.len())];
    Ok(home
        .join(".small-rust-hermes")
        .join("sessions")
        .join(format!("{stamp}-{short_id}.jsonl")))
}
