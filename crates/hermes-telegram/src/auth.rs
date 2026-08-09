//! Telegram bot-token storage.
//!
//! Storage path: `~/.lebi-ai/telegram.toml`, mode `0600`. The file
//! contains the bot token issued by @BotFather — treat it like an API key.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Persisted Telegram bot credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCreds {
    /// Bot token, e.g. `123456:ABC-DEF...`, issued by @BotFather.
    pub bot_token: String,
}

impl StoredCreds {
    pub fn default_path() -> Result<PathBuf> {
        Ok(hermes_core::data_path("telegram.toml"))
    }

    /// Load credentials from `path`. Returns `Ok(None)` if the file does not
    /// exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let c: Self =
                    toml::from_str(&s).with_context(|| format!("parsing {}", path.display()))?;
                Ok(Some(c))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Write credentials with mode 0600. Creates parent directory if missing.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let s = toml::to_string(self).context("serializing creds")?;
        std::fs::write(path, s).with_context(|| format!("writing {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(path)?.permissions();
            perm.set_mode(0o600);
            std::fs::set_permissions(path, perm)?;
        }
        Ok(())
    }
}
