//! `~/.small-rust-hermes/mcp.json` configuration.
//!
//! Schema:
//! ```json
//! {
//!   "servers": {
//!     "fs":   { "transport": "stdio", "command": "npx",
//!               "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
//!     "http": { "transport": "http",  "url":  "http://localhost:8000/mcp" }
//!   }
//! }
//! ```
//!
//! Servers with `"disabled": true` are skipped during loading. This is useful
//! for suppressing auto-detected servers (e.g. OfficeCLI) without deleting the
//! entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum ServerSpec {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        disabled: bool,
    },
    Http {
        url: String,
        #[serde(default)]
        disabled: bool,
    },
}

impl ServerSpec {
    pub fn is_disabled(&self) -> bool {
        match self {
            ServerSpec::Stdio { disabled, .. } => *disabled,
            ServerSpec::Http { disabled, .. } => *disabled,
        }
    }
}

impl McpConfig {
    /// Default location: `~/.small-rust-hermes/mcp.json`. Returns an empty
    /// config if the file does not exist (MCP is optional).
    pub fn load_default() -> Result<Self> {
        let path = Self::default_path()?;
        let mut cfg = Self::load_or_empty(&path)?;

        // Auto-detect OfficeCLI if the binary is on PATH and not already
        // configured (or configured but not disabled).
        if is_officecli_available() {
            let key = "officecli";
            let should_add = match cfg.servers.get(key) {
                None => true,
                Some(spec) => !spec.is_disabled(),
            };
            if should_add {
                cfg.servers
                    .entry(key.to_string())
                    .or_insert_with(default_officecli_server);
            }
            ensure_officecli_skill();
        }

        Ok(cfg)
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("could not resolve $HOME"))?;
        Ok(home.join(".small-rust-hermes").join("mcp.json"))
    }

    pub fn load_or_empty(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(raw) => {
                let cfg: Self = serde_json::from_str(&raw)
                    .with_context(|| format!("parsing {}", path.display()))?;
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
}

/// Check whether the `officecli` binary is available on `$PATH`.
fn is_officecli_available() -> bool {
    std::process::Command::new("officecli")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Default MCP server spec for OfficeCLI.
fn default_officecli_server() -> ServerSpec {
    ServerSpec::Stdio {
        command: "officecli".into(),
        args: vec!["mcp".into()],
        env: HashMap::new(),
        disabled: false,
    }
}

/// Auto-create the OfficeCLI skill if it doesn't already exist.
fn ensure_officecli_skill() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let skill_path = home
        .join(".small-rust-hermes")
        .join("skills")
        .join("officecli")
        .join("SKILL.md");
    if skill_path.exists() {
        return;
    }
    let content = "\
---
name: officecli
description: Strategy for editing Office documents via OfficeCLI
triggers:
  - docx
  - xlsx
  - pptx
  - word
  - excel
  - powerpoint
  - office
  - document
  - spreadsheet
  - presentation
version: \"0.1.0\"
license: Apache-2.0
---

When working with Office documents, use the three-layer strategy:

1. **L1 (Read)**: Use `view` to understand the document structure first.
2. **L2 (DOM edit)**: Use `get`/`set`/`add`/`remove`/`move`/`swap` for structured edits.
3. **L3 (Raw XML)**: Use `raw`/`raw-set` only when L2 cannot express the change.

Always `view` before editing. Use `--json` for structured output.
Use `batch` for multi-step edits to avoid repeated save/load cycles.
";
    if let Some(parent) = skill_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&skill_path, content);
}
