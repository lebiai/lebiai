use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, Session, SessionMeta, ToolHost, ToolSpec};
use hermes_llm::Config;
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore};
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};
use hermes_skills::{FsSkillStore, LoadedSkill, SkillStore};
use hermes_store::SessionWriter;
use hermes_tools::{BuiltinToolHost, CompositeToolHost};
use tokio::sync::Mutex;

pub type Sessions = Arc<Mutex<HashMap<String, ActiveSession>>>;
pub type CancelTokens = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>;
pub type ConfirmTokens = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<hermes_turn::ConfirmAction>>>>;
/// Session-scoped allowlist populated when the user clicks "Always Allow" on
/// a tool confirmation. Lives only for the lifetime of the GUI process; to
/// persist allow rules, the user edits `config.toml` directly.
pub type AlwaysAllowedTools = Arc<Mutex<HashSet<String>>>;

#[allow(dead_code)]
pub struct AppState {
    pub provider: Arc<dyn LlmProvider>,
    pub host: Arc<dyn ToolHost>,
    pub config: Config,
    pub skill_store: FsSkillStore,
    pub memory_store: FsMemoryStore,
    pub sessions: Sessions,
    pub cancel_tokens: CancelTokens,
    pub confirm_tokens: ConfirmTokens,
    pub always_allowed_tools: AlwaysAllowedTools,
    pub tools: Mutex<Vec<ToolSpec>>,
    pub skills: Mutex<Vec<LoadedSkill>>,
    pub pinned_memories: Mutex<Vec<LoadedMemory>>,
    pub active_memories: Mutex<Vec<LoadedMemory>>,
}

#[allow(dead_code)]
pub struct ActiveSession {
    pub session: Session,
    pub writer: SessionWriter,
    pub path: PathBuf,
}

impl AppState {
    pub async fn init() -> Result<Self> {
        let config = Config::load_default().context("loading config.toml")?;
        let provider = config.build_active_provider().context("building provider")?;

        let home = dirs::home_dir().context("resolving $HOME")?;
        let base = home.join(".small-rust-hermes");
        let workspace_root = base.join("workspace");
        std::fs::create_dir_all(&workspace_root)?;

        let host = load_tool_host(&workspace_root).await?;
        let tools = host.list_tools().await.unwrap_or_default();

        let skill_store = FsSkillStore::new(base.join("skills"), None);
        let memory_store = FsMemoryStore::new(base.join("memories"), None);

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
        self.config
            .workspace
            .root
            .to_string_lossy()
            .into_owned()
    }
}

async fn load_tool_host(workspace_root: &Path) -> Result<Arc<dyn ToolHost>> {
    let builtin = BuiltinToolHost::new(workspace_root.to_path_buf());

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
