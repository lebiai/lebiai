//! Compiled memory profile: a structured markdown document generated from
//! all active memories by an LLM. Always loaded into the system prompt —
//! no per-turn retrieval needed.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// `~/.lebi-ai/profile.md`
pub fn profile_path() -> Result<PathBuf> {
    Ok(hermes_core::data_path("profile.md"))
}

/// Read the compiled profile. Returns `Ok(None)` if the file does not exist.
pub fn load_profile() -> Result<Option<String>> {
    let path = profile_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(content))
}

/// Write the compiled profile atomically (tmp + rename).
pub fn save_profile(content: &str) -> Result<PathBuf> {
    let path = profile_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(".profile.md.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::with_data_dir;

    #[test]
    fn round_trip_via_save_and_load() {
        with_data_dir(|_| {
            let content = "## User\n- architect\n";
            let written = save_profile(content).unwrap();
            assert_eq!(written, profile_path().unwrap());
            let loaded = load_profile().unwrap().expect("profile should exist");
            assert_eq!(loaded, content);
        });
    }

    #[test]
    fn load_missing_returns_none() {
        with_data_dir(|_| {
            assert_eq!(load_profile().unwrap(), None);
        });
    }

    #[test]
    fn save_is_atomic_no_tmp_leftover() {
        with_data_dir(|_| {
            save_profile("v1").unwrap();
            let tmp = profile_path().unwrap().with_file_name(".profile.md.tmp");
            assert!(!tmp.exists());
        });
    }
}
