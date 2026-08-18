//! Provider configuration loaded from `~/.lebi-ai/config.toml`.

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
    /// Defaults to `~/.lebi-ai/workspace`.
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
    hermes_core::data_path("workspace")
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
    /// are persisted automatically without user prompting. **Default false**
    /// (P0: candidates must be user-confirmed); enabling it is an explicit
    /// user opt-in. Skills and conflict resolutions always require manual
    /// review.
    #[serde(default)]
    pub auto_accept_memories: bool,

    /// Minimum confidence (`low` / `medium` / `high`) for a reflection memory
    /// candidate to be auto-accepted. Parsed at the use site into
    /// `hermes_memory::Confidence`; an unrecognized value falls back to
    /// `medium`. Turns explicitly teaching the agent ("记住…", "always…")
    /// bypass this floor — they persist regardless.
    #[serde(default = "default_min_confidence")]
    pub auto_accept_min_confidence: String,

    /// When true, leaving a session may open the review modal (legacy, noisy).
    /// **Default false**: quiet evolution — candidates go to the pending-review
    /// inbox; user opens「待审」when ready.
    #[serde(default)]
    pub pop_inbox_on_leave: bool,
}

impl Default for ReflectConfig {
    fn default() -> Self {
        Self {
            min_turns: default_min_turns(),
            auto_accept_memories: false,
            auto_accept_min_confidence: default_min_confidence(),
            pop_inbox_on_leave: false,
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
    /// GUI color theme. Supported: "system" | "light" | "dark".
    #[serde(default = "default_ui_theme")]
    pub theme: String,
    /// When false (default), thinking blocks are stripped before session JSONL
    /// append — keeps transcripts small. Streaming UI still shows thinking live.
    #[serde(default)]
    pub persist_thinking: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: default_ui_language(),
            theme: default_ui_theme(),
            persist_thinking: false,
        }
    }
}

fn default_ui_language() -> String {
    "zh-CN".to_string()
}

fn default_ui_theme() -> String {
    "system".to_string()
}

/// Configuration for the `web_search` / `web_fetch` tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Preferred search backend: `scraper` | `tavily` | `brave_api` | `searxng`.
    /// Failures cascade through free fallbacks (DuckDuckGo / Bing / curl).
    #[serde(default = "default_search_backend")]
    pub search_backend: String,
    /// Tavily Search API key (used when `search_backend = "tavily"`).
    #[serde(default)]
    pub tavily_api_key: String,
    /// Brave Search API subscription token (used when `search_backend = "brave_api"`).
    #[serde(default)]
    pub brave_api_key: String,
    /// SearXNG base URL (self-hosted recommended), e.g. `http://127.0.0.1:8080`.
    #[serde(default)]
    pub searxng_url: String,
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
            searxng_url: String::new(),
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
    #[serde(default)]
    pub deepseek: Option<ProviderConfig>,
}

impl ProvidersMap {
    /// Look up a provider section by preset key (`deepseek` / `anthropic` /
    /// `openai`). Unknown keys return `None` so callers can fall back to the
    /// bundled preset instead of inventing tables.
    pub fn get(&self, key: &str) -> Option<&ProviderConfig> {
        match key {
            "anthropic" => self.anthropic.as_ref(),
            "openai" => self.openai.as_ref(),
            "deepseek" => self.deepseek.as_ref(),
            _ => None,
        }
    }
}

/// One bundled provider preset. Single source of truth shared by the default
/// config template, `hermes init` and the GUI settings selector — adding a
/// provider means adding one entry here (plus the matching `ProvidersMap`
/// field and `active_provider`/`active_kind` branches).
#[derive(Debug, Clone, Copy)]
pub struct ProviderPreset {
    pub key: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    pub model: &'static str,
    pub max_tokens: u32,
}

/// Bundled provider presets, in recommendation order (DeepSeek first — the
/// default for fresh installs and the top pick in `hermes init`).
pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        key: "deepseek",
        label: "DeepSeek (recommended)",
        base_url: "https://api.deepseek.com/v1",
        model: "deepseek-v4-flash",
        max_tokens: 16_384,
    },
    ProviderPreset {
        key: "anthropic",
        label: "Anthropic (Claude)",
        base_url: "https://api.anthropic.com",
        model: "claude-sonnet-4-20250514",
        max_tokens: 16_384,
    },
    ProviderPreset {
        key: "openai",
        label: "OpenAI (GPT)",
        base_url: "https://api.openai.com",
        model: "gpt-4o-mini",
        max_tokens: 16_384,
    },
];

impl ProviderPreset {
    pub fn by_key(key: &str) -> Option<&'static ProviderPreset> {
        PROVIDER_PRESETS.iter().find(|p| p.key == key)
    }
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
    /// Default config location: `~/.lebi-ai/config.toml`.
    pub fn default_path() -> Result<PathBuf> {
        Ok(hermes_core::data_path("config.toml"))
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
        let mut cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        cfg.rehome_workspace_if_needed();
        Ok(cfg)
    }

    /// Keep workspace under the current product `data_root()` so products never
    /// share upload/tool paths after data-dir isolation.
    fn rehome_workspace_if_needed(&mut self) {
        let expected = hermes_core::data_path("workspace");
        let data = hermes_core::data_root();
        if !self.workspace.root.starts_with(&data) {
            tracing::warn!(
                old = %self.workspace.root.display(),
                new = %expected.display(),
                "workspace.root re-homed under product data root"
            );
            self.workspace.root = expected;
        }
    }

    /// Fresh-install config template. Provider sections are generated from
    /// [`PROVIDER_PRESETS`] so the GUI selector, `hermes init` and the on-disk
    /// default can never drift apart.
    pub fn default_config_toml() -> String {
        let mut toml = String::from("default_provider = \"");
        toml.push_str(PROVIDER_PRESETS[0].key);
        toml.push_str("\"\n\n");
        for preset in PROVIDER_PRESETS {
            toml.push_str(&format!(
                "[providers.{}]\nbase_url = {:?}\napi_key = \"\"\nmodel = {:?}\nmax_tokens = {}\n\n",
                preset.key, preset.base_url, preset.model, preset.max_tokens
            ));
        }
        toml.push_str(Self::DEFAULT_CONFIG_TAIL);
        toml
    }

    /// Non-provider sections of the fresh-install template.
    const DEFAULT_CONFIG_TAIL: &str = r#"[reflect]
min_turns = 3
auto_accept_memories = false
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
language = "zh-CN"
theme = "system"
persist_thinking = false

[web]
search_backend = "scraper"   # scraper | tavily | brave_api | searxng
tavily_api_key = ""
brave_api_key = ""
searxng_url = ""             # e.g. http://127.0.0.1:8080  (self-hosted SearXNG)
cache_ttl_secs = 900
extract_model = ""           # empty = reuse main model; e.g. a Haiku / mini tier
"#;

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

    /// Serialize this config to TOML and write it to `path` (0600 on Unix),
    /// creating parent directories as needed. Used by `hermes init`.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating config dir {}", dir.display()))?;
        }
        let toml = toml::to_string_pretty(self).context("serializing config to TOML")?;
        std::fs::write(path, toml)
            .with_context(|| format!("writing config at {}", path.display()))?;
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
            "deepseek" => self.providers.deepseek.as_ref().ok_or_else(|| {
                anyhow!("default_provider=deepseek but [providers.deepseek] missing")
            }),
            other => Err(anyhow!("unknown default_provider {other:?}")),
        }
    }

    pub fn active_kind(&self) -> Result<ProviderKind> {
        match self.default_provider.as_str() {
            "anthropic" => Ok(ProviderKind::Anthropic),
            "openai" => Ok(ProviderKind::OpenAi),
            // DeepSeek's official API is OpenAI-compatible (https://api.deepseek.com/v1).
            "deepseek" => Ok(ProviderKind::OpenAi),
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
    use super::{Config, ProviderPreset, PROVIDER_PRESETS};

    #[test]
    fn default_config_template_loads() {
        let cfg: Config = toml::from_str(&Config::default_config_toml()).unwrap();

        assert_eq!(cfg.default_provider, "deepseek");
        // P0: reflection candidates must be user-confirmed — the default
        // config must never auto-write memories.
        assert!(!cfg.reflect.auto_accept_memories);
        assert!(cfg.active_provider().is_ok());
        assert_eq!(cfg.reflect.min_turns, 3);
        assert_eq!(cfg.context.model_limit, 128_000);
        assert_eq!(cfg.ui.language, "zh-CN");
        assert_eq!(cfg.ui.theme, "system");
        assert!(!cfg.ui.persist_thinking);
        assert_eq!(cfg.limits.max_tool_rounds, 25);
        assert_eq!(cfg.web.search_backend, "scraper");
        assert_eq!(cfg.web.cache_ttl_secs, 900);
        assert!(cfg.web.extract_model.is_empty());
    }

    #[test]
    fn provider_presets_drive_template_and_lookup() {
        // Presets cover exactly the keys the engine can activate.
        assert_eq!(PROVIDER_PRESETS.len(), 3);
        for preset in PROVIDER_PRESETS {
            assert!(matches!(preset.key, "deepseek" | "anthropic" | "openai"));
        }
        assert_eq!(PROVIDER_PRESETS[0].key, "deepseek");
        assert!(ProviderPreset::by_key("openai").is_some());
        assert!(ProviderPreset::by_key("nope").is_none());

        // The fresh template must contain every preset section with matching
        // values, and the default provider is the first preset.
        let cfg: Config = toml::from_str(&Config::default_config_toml()).unwrap();
        assert_eq!(cfg.default_provider, PROVIDER_PRESETS[0].key);
        for preset in PROVIDER_PRESETS {
            let section = cfg.providers.get(preset.key).unwrap();
            assert_eq!(section.base_url, preset.base_url);
            assert_eq!(section.model, preset.model);
            assert_eq!(section.max_tokens, preset.max_tokens);
            assert!(section.api_key.is_empty());
        }
        assert!(cfg.providers.get("nope").is_none());
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

        assert_eq!(cfg.default_provider, "deepseek");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
