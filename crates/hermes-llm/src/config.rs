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
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub web: WebConfig,
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
    25
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

    /// When true, micro-reflection memory candidates that meet
    /// `auto_accept_min_confidence` (and have no conflicts/supersedes links)
    /// are persisted automatically without user prompting. Skills and
    /// conflict resolutions always require manual review.
    #[serde(default)]
    pub auto_accept_memories: bool,

    /// Minimum confidence (`low` / `medium` / `high`) for a reflection memory
    /// candidate to be auto-accepted. Parsed at the use site into
    /// `hermes_memory::Confidence`; an unrecognized value falls back to
    /// `medium`. Turns explicitly teaching the agent ("记住…", "always…")
    /// bypass this floor — they persist regardless.
    #[serde(default = "default_min_confidence")]
    pub auto_accept_min_confidence: String,
}

impl Default for ReflectConfig {
    fn default() -> Self {
        Self {
            min_turns: default_min_turns(),
            auto_accept_memories: true,
            auto_accept_min_confidence: default_min_confidence(),
        }
    }
}

fn default_min_turns() -> usize {
    3
}

fn default_min_confidence() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// UI display language. Supported values: "en-US" and "zh-CN".
    #[serde(default = "default_ui_language")]
    pub language: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: default_ui_language(),
        }
    }
}

fn default_ui_language() -> String {
    "en-US".to_string()
}

/// Configuration for the `web_search` / `web_fetch` tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Search backend: `"scraper"` (Brave HTML, no key), `"tavily"`, or
    /// `"brave_api"`. API backends fall back to the scraper when their key is
    /// empty.
    #[serde(default = "default_search_backend")]
    pub search_backend: String,
    /// Tavily Search API key (used when `search_backend = "tavily"`).
    #[serde(default)]
    pub tavily_api_key: String,
    /// Brave Search API subscription token (used when `search_backend = "brave_api"`).
    #[serde(default)]
    pub brave_api_key: String,
    /// TTL (seconds) for the in-process fetch/search result cache.
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
    /// Model id used for `web_fetch` prompt-extraction. Empty → reuse the main
    /// provider's model. Set to a cheaper model (e.g. a Haiku / mini tier) to
    /// cut extraction cost.
    #[serde(default)]
    pub extract_model: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            search_backend: default_search_backend(),
            tavily_api_key: String::new(),
            brave_api_key: String::new(),
            cache_ttl_secs: default_cache_ttl_secs(),
            extract_model: String::new(),
        }
    }
}

fn default_search_backend() -> String {
    "scraper".to_string()
}

fn default_cache_ttl_secs() -> u64 {
    900
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

    pub fn load_default_or_create() -> Result<Self> {
        let path = Self::default_path()?;
        if !path.exists() {
            Self::write_default_config(&path)?;
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }

    pub fn default_config_toml() -> &'static str {
        r#"default_provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = ""
model = "claude-sonnet-4-20250514"
max_tokens = 16384

[providers.openai]
base_url = "https://api.openai.com"
api_key = ""
model = "gpt-4o-mini"
max_tokens = 16384

[reflect]
min_turns = 3
auto_accept_memories = true
auto_accept_min_confidence = "medium"

[context]
model_limit = 128000
headroom = 0.18
keep_recent_turns = 4

[limits]
max_tool_rounds = 25
agent_max_iterations = 50
active_memory_index_cap = 50
skill_index_cap = 50
relevant_memory_cap = 3
triggered_skill_cap = 3

[permissions]
allow = []
deny = []

[ui]
language = "en-US"

[web]
search_backend = "scraper"   # scraper | tavily | brave_api
tavily_api_key = ""
brave_api_key = ""
cache_ttl_secs = 900
extract_model = ""           # empty = reuse main model; e.g. a Haiku / mini tier
"#
    }

    fn write_default_config(path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .ok_or_else(|| anyhow!("config path has no parent: {}", path.display()))?;
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;
        std::fs::write(path, Self::default_config_toml())
            .with_context(|| format!("writing default config at {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("chmod 600 {}", path.display()))?;
        }
        Ok(())
    }

    pub fn active_provider(&self) -> Result<&ProviderConfig> {
        match self.default_provider.as_str() {
            "anthropic" => self.providers.anthropic.as_ref().ok_or_else(|| {
                anyhow!("default_provider=anthropic but [providers.anthropic] missing")
            }),
            "openai" => {
                self.providers.openai.as_ref().ok_or_else(|| {
                    anyhow!("default_provider=openai but [providers.openai] missing")
                })
            }
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

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn default_config_template_loads() {
        let cfg: Config = toml::from_str(Config::default_config_toml()).unwrap();

        assert_eq!(cfg.default_provider, "anthropic");
        assert!(cfg.active_provider().is_ok());
        assert_eq!(cfg.reflect.min_turns, 3);
        assert_eq!(cfg.context.model_limit, 128_000);
        assert_eq!(cfg.ui.language, "en-US");
        assert_eq!(cfg.limits.max_tool_rounds, 25);
        assert_eq!(cfg.web.search_backend, "scraper");
        assert_eq!(cfg.web.cache_ttl_secs, 900);
        assert!(cfg.web.extract_model.is_empty());
    }

    #[test]
    fn web_config_defaults_when_section_absent() {
        // A config with no [web] section must still parse, using WebConfig::default().
        let minimal = r#"default_provider = "anthropic"
[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = ""
model = "claude-sonnet-4-20250514"
"#;
        let cfg: Config = toml::from_str(minimal).unwrap();
        assert_eq!(cfg.web.search_backend, "scraper");
        assert_eq!(cfg.web.cache_ttl_secs, 900);
    }

    #[test]
    fn write_default_config_creates_parent_and_file() {
        let dir = std::env::temp_dir().join(format!(
            "small-rust-hermes-config-test-{}",
            std::process::id()
        ));
        let path = dir.join("nested").join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);

        Config::write_default_config(&path).unwrap();
        let cfg = Config::load_from(&path).unwrap();

        assert_eq!(cfg.default_provider, "anthropic");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
