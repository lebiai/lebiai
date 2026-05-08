//! Shared helpers (config / session path / tool host) — duplicated
//! locally from hermes-cli to avoid a cli→tui dependency.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, NullToolHost, SessionMeta, ToolHost};
use hermes_llm::Config;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};

pub fn build_active_provider(cfg: &Config) -> Result<Arc<dyn LlmProvider>> {
    cfg.build_active_provider()
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

pub async fn load_tool_host(workspace_root: &Path) -> Result<Arc<dyn ToolHost>> {
    std::fs::create_dir_all(workspace_root)
        .with_context(|| format!("ensuring workspace exists: {}", workspace_root.display()))?;
    let mut cfg = McpConfig::load_default().context("loading mcp.json")?;
    rewrite_filesystem_servers(&mut cfg, workspace_root);
    if cfg.servers.is_empty() {
        return Ok(Arc::new(NullToolHost));
    }
    let host = McpToolHost::connect_all(&cfg)
        .await
        .map_err(|e| anyhow::anyhow!("connecting MCP servers: {e}"))?;
    Ok(Arc::new(host))
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
