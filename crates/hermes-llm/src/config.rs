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
    #[serde(default)]
    pub limits: ContextLimits,
    #[serde(default)]
    pub permissions: PermissionsConfig,
}

/// Numeric caps applied to per-turn context assembly and tool-loop bounds.
/// All values are read once at startup; in-flight values are not hot-reloaded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ContextLimits {
    /// Maximum tool-use rounds the chat / ask REPL will run within a single
    /// user turn before forcing a textual answer (safety bound on runaway
    /// agentic loops).
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,

    /// Default `--max-iterations` for the autonomous `run` (agent) command;
    /// the CLI flag overrides this on a per-invocation basis.
    #[serde(default = "default_agent_max_iterations")]
    pub agent_max_iterations: usize,

    /// Max number of episodic memories listed in the session-level
    /// "Active memory index" section.
    #[serde(default = "default_active_memory_index_cap")]
    pub active_memory_index_cap: usize,

    /// Max number of skills listed in the session-level
    /// "Available skills" index.
    #[serde(default = "default_skill_index_cap")]
    pub skill_index_cap: usize,

    /// Max number of episodic memory bodies injected per turn in the
    /// "Relevant memories for this turn" section.
    #[serde(default = "default_relevant_memory_cap")]
    pub relevant_memory_cap: usize,

    /// Max number of skill bodies expanded per turn in the
    /// "Skills triggered for this turn" section.
    #[serde(default = "default_triggered_skill_cap")]
    pub triggered_skill_cap: usize,
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self {
            max_tool_rounds: default_max_tool_rounds(),
            agent_max_iterations: default_agent_max_iterations(),
            active_memory_index_cap: default_active_memory_index_cap(),
            skill_index_cap: default_skill_index_cap(),
            relevant_memory_cap: default_relevant_memory_cap(),
            triggered_skill_cap: default_triggered_skill_cap(),
        }
    }
}

fn default_max_tool_rounds() -> usize {
    10
}
fn default_agent_max_iterations() -> usize {
    50
}
fn default_active_memory_index_cap() -> usize {
    50
}
fn default_skill_index_cap() -> usize {
    50
}
fn default_relevant_memory_cap() -> usize {
    3
}
fn default_triggered_skill_cap() -> usize {
    3
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

    /// When true, micro-reflection memory candidates with confidence=Medium,
    /// no conflicts, and no supersedes links are persisted automatically
    /// without user prompting. Skills and conflict resolutions always
    /// require manual review.
    #[serde(default)]
    pub auto_accept_memories: bool,
}

impl Default for ReflectConfig {
    fn default() -> Self {
        Self {
            min_turns: default_min_turns(),
            auto_accept_memories: true,
        }
    }
}

fn default_min_turns() -> usize {
    3
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionsConfig {
    /// Rules that auto-approve matching tool calls without prompting.
    /// Format: `<tool>` or `<tool>:<glob_pattern>`.
    /// Example: `bash:git *`, `edit:*.rs`, `read`, `mcp:github__*`.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that auto-deny matching tool calls without prompting.
    /// Same format as `allow`. Evaluated before `allow`.
    #[serde(default)]
    pub deny: Vec<String>,
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
