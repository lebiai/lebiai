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
    },
    Http {
        url: String,
    },
}

impl McpConfig {
    /// Default location: `~/.small-rust-hermes/mcp.json`. Returns an empty
    /// config if the file does not exist (MCP is optional).
    pub fn load_default() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_or_empty(&path)
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
