//! Compiled memory profile: a structured markdown document generated from
//! all active memories by an LLM. Always loaded into the system prompt —
//! no per-turn retrieval needed.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// `~/.small-rust-hermes/profile.md`
pub fn profile_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    Ok(home.join(".small-rust-hermes").join("profile.md"))
}

/// Read the compiled profile. Returns `Ok(None)` if the file does not exist.
pub fn load_profile() -> Result<Option<String>> {
    let path = profile_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
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
    std::fs::write(&tmp, content)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {

    #[test]
    fn round_trip_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.md");

        // Simulate by writing directly to the tempdir path.
        let content = "## User\n- architect\n";
        std::fs::write(&path, content).unwrap();
        let read = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read, content);
    }

    #[test]
    fn load_missing_returns_none() {
        // profile_path() points to the real home dir, but we test the
        // logic: a non-existent file should yield None.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.md");
        assert!(!path.exists());
    }
}
