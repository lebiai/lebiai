//! Provider configuration loaded from `~/.small-rust-hermes/config.toml`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use hermes_core::LlmProvider;
use serde::{Deserialize, Serialize};

use crate::{AnthropicProvider, OpenAiProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub default_provider: String,
    #[serde(default)]
    pub providers: ProvidersMap,
    #[serde(default)]
    pub reflect: ReflectConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub context: ContextConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "default_model_limit")]
    pub model_limit: usize,
    #[serde(default = "default_headroom")]
    pub headroom: f64,
    #[serde(default = "default_keep_recent_turns")]
    pub keep_recent_turns: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            model_limit: default_model_limit(),
            headroom: default_headroom(),
            keep_recent_turns: default_keep_recent_turns(),
        }
    }
}

fn default_model_limit() -> usize {
    128_000
}
fn default_headroom() -> f64 {
    0.18
}
fn default_keep_recent_turns() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Absolute path the agent treats as its sandbox. All file IO via MCP
    /// tools is expected to stay inside this directory; the chat banner
    /// announces it to the LLM and the filesystem MCP server (if configured
    /// with `command: "npx"`/`"@modelcontextprotocol/server-filesystem"`)
    /// has its allowed-directory argument auto-rewritten to match.
    ///
    /// Defaults to `~/.small-rust-hermes/workspace`.
    #[serde(default = "default_workspace_root")]
    pub root: std::path::PathBuf,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            root: default_workspace_root(),
        }
    }
}

fn default_workspace_root() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".small-rust-hermes").join("workspace"))
        .unwrap_or_else(|| std::path::PathBuf::from("./workspace"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectConfig {
    /// Minimum number of completed user turns before quit-driven
    /// reflection runs. Below this, the reflection pass is skipped
    /// silently. The explicit `/reflect` command always runs regardless.
    #[serde(default = "default_min_turns")]
    pub min_turns: usize,
}

impl Default for ReflectConfig {
    fn default() -> Self {
        Self {
            min_turns: default_min_turns(),
        }
    }
}

fn default_min_turns() -> usize {
    3
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersMap {
    #[serde(default)]
    pub anthropic: Option<ProviderConfig>,
    #[serde(default)]
    pub openai: Option<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    16_384
}

impl Config {
    /// Default config location: `~/.small-rust-hermes/config.toml`.
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("could not resolve $HOME"))?;
        Ok(home.join(".small-rust-hermes").join("config.toml"))
    }

    pub fn load_default() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }

    pub fn active_provider(&self) -> Result<&ProviderConfig> {
        match self.default_provider.as_str() {
            "anthropic" => self
                .providers
                .anthropic
                .as_ref()
                .ok_or_else(|| anyhow!("default_provider=anthropic but [providers.anthropic] missing")),
            "openai" => self
                .providers
                .openai
                .as_ref()
                .ok_or_else(|| anyhow!("default_provider=openai but [providers.openai] missing")),
            other => Err(anyhow!("unknown default_provider {other:?}")),
        }
    }

    pub fn active_kind(&self) -> Result<ProviderKind> {
        match self.default_provider.as_str() {
            "anthropic" => Ok(ProviderKind::Anthropic),
            "openai" => Ok(ProviderKind::OpenAi),
            other => Err(anyhow!("unknown default_provider {other:?}")),
        }
    }

    /// Build the [`LlmProvider`] selected by `default_provider`. Wraps in
    /// `Arc` so the caller can share it across spawned tasks.
    pub fn build_active_provider(&self) -> Result<Arc<dyn LlmProvider>> {
        let cfg = self.active_provider()?;
        let kind = self.active_kind()?;
        match kind {
            ProviderKind::Anthropic => {
                let p = AnthropicProvider::new(
                    cfg.base_url.clone(),
                    cfg.api_key.clone(),
                    cfg.model.clone(),
                    cfg.supports_caching(),
                )
                .map_err(|e| anyhow!("building anthropic provider: {e}"))?;
                Ok(Arc::new(p))
            }
            ProviderKind::OpenAi => {
                let p = OpenAiProvider::new(
                    cfg.base_url.clone(),
                    cfg.api_key.clone(),
                    cfg.model.clone(),
                )
                .map_err(|e| anyhow!("building openai provider: {e}"))?;
                Ok(Arc::new(p))
            }
        }
    }
}

impl ProviderConfig {
    /// Heuristic: an Anthropic config talking to a DeepSeek-compat endpoint
    /// cannot honour prompt caching (DeepSeek ignores `cache_control`).
    pub fn supports_caching(&self) -> bool {
        let host = self.base_url.to_ascii_lowercase();
        !host.contains("deepseek.com")
    }
}
