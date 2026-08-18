//! IM channel sender allowlist.
//!
//! Surfaces without a confirmation UI (Telegram / Feishu / WeChat) must not
//! accept traffic from arbitrary senders by default. Configuration lives at
//! `~/.lebi-ai/channel-allowlist.toml` (or `$LEBI_DATA_DIR/...`):
//!
//! ```toml
//! # Default: empty list = deny everyone (safe).
//! # Explicit opt-in to open a channel to the world:
//! #   allowed = ["*"]
//!
//! [telegram]
//! allowed = ["123456789"]          # chat ids
//!
//! [feishu]
//! allowed = ["ou_xxx"]             # open_ids
//!
//! [wechat]
//! allowed = ["*"]                  # open to all (explicit)
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize, Clone)]
struct AllowFile {
    #[serde(default)]
    telegram: ChannelAllow,
    #[serde(default)]
    feishu: ChannelAllow,
    #[serde(default)]
    wechat: ChannelAllow,
    /// Optional generic map for future channels: `[channels.foo] allowed = [...]`
    #[serde(default)]
    channels: HashMap<String, ChannelAllow>,
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ChannelAllow {
    /// Sender / chat ids allowed to talk to the bot.
    /// Empty = deny all. Single entry `"*"` = allow all (explicit open).
    #[serde(default)]
    allowed: Vec<String>,
}

fn allowlist_path() -> PathBuf {
    hermes_core::data_path("channel-allowlist.toml")
}

fn load_allow_file() -> AllowFile {
    let path = allowlist_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => toml::from_str(&raw).unwrap_or_else(|e| {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "invalid channel-allowlist.toml; denying all senders"
            );
            AllowFile::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AllowFile::default(),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "cannot read channel-allowlist.toml; denying all senders"
            );
            AllowFile::default()
        }
    }
}

/// Cached allowlist (reloaded at most once per process — restart after edit).
fn cached() -> &'static AllowFile {
    static CACHE: OnceLock<AllowFile> = OnceLock::new();
    CACHE.get_or_init(load_allow_file)
}

/// Force-reload allowlist (tests / rare hot-reload).
#[cfg(test)]
pub fn reload_for_tests() {
    // OnceLock can't clear; tests use direct evaluation helpers instead.
}

fn entries_for<'a>(channel: &str, file: &'a AllowFile) -> &'a [String] {
    match channel {
        "telegram" => &file.telegram.allowed,
        "feishu" => &file.feishu.allowed,
        "wechat" | "weixin" => &file.wechat.allowed,
        other => file
            .channels
            .get(other)
            .map(|c| c.allowed.as_slice())
            .unwrap_or(&[]),
    }
}

/// Returns `true` when `sender_id` may talk on `channel`.
///
/// Policy:
/// - missing / empty allowlist → **deny**
/// - `allowed` contains `"*"` → allow all
/// - otherwise exact string match (after trim)
pub fn is_sender_allowed(channel: &str, sender_id: &str) -> bool {
    is_sender_allowed_with(channel, sender_id, cached())
}

fn is_sender_allowed_with(channel: &str, sender_id: &str, file: &AllowFile) -> bool {
    let id = sender_id.trim();
    if id.is_empty() {
        return false;
    }
    let list = entries_for(channel, file);
    if list.is_empty() {
        return false;
    }
    if list.iter().any(|e| e.trim() == "*") {
        return true;
    }
    list.iter().any(|e| e.trim() == id)
}

/// Human-readable denial for IM bots.
pub fn deny_message(channel: &str) -> String {
    format!(
        "未授权：此 {channel} 机器人仅接受白名单用户。\
         请在本机编辑 `~/.lebi-ai/channel-allowlist.toml`，\
         在 [{channel}] 下把你的 id 加入 allowed（或 allowed = [\"*\"] 显式对所有人开放），\
         然后重启 bot。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_denies() {
        let f = AllowFile::default();
        assert!(!is_sender_allowed_with("telegram", "1", &f));
    }

    #[test]
    fn star_allows() {
        let f = AllowFile {
            telegram: ChannelAllow {
                allowed: vec!["*".into()],
            },
            ..Default::default()
        };
        assert!(is_sender_allowed_with("telegram", "any", &f));
    }

    #[test]
    fn exact_match() {
        let f = AllowFile {
            telegram: ChannelAllow {
                allowed: vec!["42".into(), "99".into()],
            },
            ..Default::default()
        };
        assert!(is_sender_allowed_with("telegram", "42", &f));
        assert!(!is_sender_allowed_with("telegram", "7", &f));
    }
}
