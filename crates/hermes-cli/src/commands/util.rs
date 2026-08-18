//! Shared command helpers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, SessionMeta, ToolHost};
use hermes_llm::Config;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};
use hermes_memory::MemoryStore;
use hermes_skills::SkillStore;
use hermes_tools::{
    BuiltinToolHost, CompositeToolHost, ProposeContext, SearchBackend, SubagentContext,
    WebToolsContext,
};

/// Build the active [`LlmProvider`] selected by `default_provider` in
/// `~/.lebi-ai/config.toml`.
pub fn build_active_provider(cfg: &Config) -> Result<Arc<dyn LlmProvider>> {
    cfg.build_active_provider()
}

/// Load the default config, turning the two most common first-run failures
/// (no config file, or an empty API key) into a short, actionable message
/// that points at `hermes init` instead of a raw deserialize / IO error.
pub fn load_config_or_hint() -> Result<Config> {
    let path = Config::default_path()?;
    if !path.exists() {
        anyhow::bail!(
            "no config found at {}\n  \u{2192} run `hermes init` to set up your provider and API key",
            path.display()
        );
    }
    let cfg = Config::load_default()
        .with_context(|| format!("loading config from {}", path.display()))?;
    if let Ok(provider) = cfg.active_provider() {
        if provider.api_key.trim().is_empty() {
            anyhow::bail!(
                "API key for provider `{}` is empty\n  \u{2192} run `hermes init`, or edit {} to set it",
                cfg.default_provider,
                path.display()
            );
        }
    }
    Ok(cfg)
}

/// `max_tokens` budget for `web_fetch` prompt-extraction answers — concise by
/// design, independent of the (larger) main-turn budget.
const WEB_EXTRACT_MAX_TOKENS: u32 = 2048;

/// Build the [`WebToolsContext`] wiring the `web_fetch` / `web_search` tools:
/// the extraction provider (reused from the main provider), the configured
/// search backend + keys, and the cache TTL — all from the `[web]` config.
pub fn build_web_ctx(cfg: &Config, provider: Arc<dyn LlmProvider>) -> Arc<WebToolsContext> {
    Arc::new(WebToolsContext {
        extract_provider: provider,
        extract_model: cfg.web.extract_model.clone(),
        extract_max_tokens: WEB_EXTRACT_MAX_TOKENS,
        search_backend: SearchBackend::parse(&cfg.web.search_backend),
        tavily_api_key: cfg.web.tavily_api_key.clone(),
        brave_api_key: cfg.web.brave_api_key.clone(),
        searxng_url: cfg.web.searxng_url.clone(),
        cache_ttl_secs: cfg.web.cache_ttl_secs,
    })
}

/// Where to write a session JSONL given its metadata.
///
/// Path: `~/.lebi-ai/sessions/<timestamp>-<short-id>.jsonl`.
pub fn session_path_for(meta: &SessionMeta) -> Result<PathBuf> {
    let stamp = meta.created_at.format("%Y-%m-%dT%H-%M-%S");
    let short_id = &meta.id[..8.min(meta.id.len())];
    Ok(hermes_core::data_path("sessions").join(format!("{stamp}-{short_id}.jsonl")))
}

/// Load built-in tools + MCP tools as a composite host.
///
/// Pass `propose_ctx = Some(...)` to enable the `propose_skill` tool; pass
/// `None` for one-shot modes (e.g. `hermes ask`) where reflective skill
/// drafting doesn't apply.
///
/// Pass `skill_store = Some(...)` to enable the `skill_list` / `skill_read` /
/// `skill_read_file` / `skill_create` tools — the Activation and Execution
/// stages of the Agent Skills Progressive Disclosure model.
///
/// Pass `subagent_ctx = Some(...)` to enable the `subagent` tool — the runtime
/// primitive the bundled `skill-creator` meta-skill uses for real evaluations
/// (each test case runs in a fresh child context, no parent reasoning leakage).
pub async fn load_tool_host(
    workspace_root: &Path,
    memory_store: Option<Arc<dyn MemoryStore>>,
    skill_store: Option<Arc<dyn SkillStore>>,
    propose_ctx: Option<Arc<ProposeContext>>,
    subagent_ctx: Option<Arc<SubagentContext>>,
    web_ctx: Option<Arc<WebToolsContext>>,
) -> Result<Arc<dyn ToolHost>> {
    std::fs::create_dir_all(workspace_root)
        .with_context(|| format!("ensuring workspace exists: {}", workspace_root.display()))?;

    let mut builtin = BuiltinToolHost::new(workspace_root.to_path_buf());
    if let Some(store) = memory_store {
        builtin = builtin.with_memory_store(store);
    }
    builtin = builtin.with_commitment_store(Arc::new(
        hermes_commitments::CommitmentStore::standard(),
    ));
    if let Some(store) = skill_store {
        builtin = builtin.with_skill_store(store);
    }
    if let Some(ctx) = propose_ctx {
        builtin = builtin.with_propose_ctx(ctx);
    }
    if let Some(ctx) = subagent_ctx {
        builtin = builtin.with_subagent_ctx(ctx);
    }
    if let Some(ctx) = web_ctx {
        builtin = builtin.with_web_ctx(ctx);
    }

    let mut cfg = McpConfig::load_default().context("loading mcp.json")?;
    rewrite_filesystem_servers(&mut cfg, workspace_root);

    let mcp: Option<Box<dyn ToolHost>> = if cfg.servers.is_empty() {
        None
    } else {
        match McpToolHost::connect_all(&cfg).await {
            Ok(host) => Some(Box::new(host)),
            Err(e) => {
                tracing::warn!(error=%e, "MCP connection failed; continuing with built-in tools only");
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
            // Keep args up to and including the package name; replace any
            // trailing path args with our workspace root. Args are typically:
            //   ["-y", "@modelcontextprotocol/server-filesystem", "<path>"...]
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
