//! Shared command helpers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, SessionMeta, ToolHost};
use hermes_llm::Config;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};
use hermes_memory::MemoryStore;
use hermes_tools::{BuiltinToolHost, CompositeToolHost};

/// Build the active [`LlmProvider`] selected by `default_provider` in
/// `~/.small-rust-hermes/config.toml`.
pub fn build_active_provider(cfg: &Config) -> Result<Arc<dyn LlmProvider>> {
    cfg.build_active_provider()
}

/// Where to write a session JSONL given its metadata.
///
/// Path: `~/.small-rust-hermes/sessions/<timestamp>-<short-id>.jsonl`.
pub fn session_path_for(meta: &SessionMeta) -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    let stamp = meta.created_at.format("%Y-%m-%dT%H-%M-%S");
    let short_id = &meta.id[..8.min(meta.id.len())];
    Ok(home
        .join(".small-rust-hermes")
        .join("sessions")
        .join(format!("{stamp}-{short_id}.jsonl")))
}

/// Load built-in tools + MCP tools as a composite host.
pub async fn load_tool_host(
    workspace_root: &Path,
    memory_store: Option<Arc<dyn MemoryStore>>,
) -> Result<Arc<dyn ToolHost>> {
    std::fs::create_dir_all(workspace_root).with_context(|| {
        format!("ensuring workspace exists: {}", workspace_root.display())
    })?;

    let mut builtin = BuiltinToolHost::new(workspace_root.to_path_buf());
    if let Some(store) = memory_store {
        builtin = builtin.with_memory_store(store);
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
