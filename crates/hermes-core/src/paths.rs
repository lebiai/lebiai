//! Product data directory resolution.
//!
//! **Generic product (this tree): lebi-AI** defaults to `~/.lebi-ai`.
//! The lawyer edition uses a different default (`~/.lebi-law`) so the two
//! products never share skills / sessions / knowledge by accident.
//!
//! Override (both products): environment variable [`ENV_DATA_DIR`]
//! (`LEBI_DATA_DIR`; the legacy `HERMES_DATA_DIR` is honored as a fallback
//! so existing installs keep working during the rename).

use std::path::{Path, PathBuf};

/// Env var that overrides the default data root for any lebi-family product.
pub const ENV_DATA_DIR: &str = "LEBI_DATA_DIR";

/// Legacy env var, honored as a fallback for installs configured before the
/// lebi-AI rename.
pub const LEGACY_ENV_DATA_DIR: &str = "HERMES_DATA_DIR";

/// Directory name under `$HOME` when [`ENV_DATA_DIR`] is unset.
/// Generic product (lebi-AI).
pub const DEFAULT_DATA_DIRNAME: &str = ".lebi-ai";

/// Pre-branding data directory, migrated once on first run (see
/// [`maybe_migrate_data_root`]). Never the lawyer edition's `.lebi-law`.
pub const LEGACY_DATA_DIRNAME: &str = ".small-rust-hermes";

/// Project-scoped data folder name (cwd-relative), same product identity.
pub fn project_data_dirname() -> &'static str {
    DEFAULT_DATA_DIRNAME
}

/// System-level pointer file remembering a user-chosen data root
/// (Settings → data location migration). Lives **outside** the data root so it
/// survives the move itself. Windows: `%APPDATA%\lebi-ai\data-dir.txt`;
/// macOS: `~/Library/Application Support/lebi-ai/data-dir.txt`; Linux:
/// `$XDG_CONFIG_HOME/lebi-ai/data-dir.txt`.
pub fn data_dir_pointer_path() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join("AppData").join("Roaming")))
            .unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join("Library").join("Application Support"))
            .unwrap_or_default()
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .unwrap_or_default()
    };
    base.join("lebi-ai").join("data-dir.txt")
}

/// Whether the current data root was chosen by the user via Settings migration
/// (pointer file present and valid).
pub fn is_user_chosen_data_root() -> bool {
    read_data_dir_pointer().is_some()
}

/// Read the pointer file; returns `Some` only when it names an existing
/// absolute directory (a stale pointer silently falls back to defaults).
pub fn read_data_dir_pointer() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(data_dir_pointer_path()).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let pb = PathBuf::from(trimmed);
    if pb.is_absolute() && pb.is_dir() {
        Some(pb)
    } else {
        None
    }
}

/// Persist a user-chosen data root. Callers must validate the directory
/// (non-empty target is rejected before this is ever reached).
pub fn write_data_dir_pointer(dir: &Path) -> std::io::Result<()> {
    let p = data_dir_pointer_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, dir.to_string_lossy().as_bytes())
}

/// Forget a previously chosen data root (restores env/home resolution).
pub fn clear_data_dir_pointer() -> std::io::Result<()> {
    match std::fs::remove_file(data_dir_pointer_path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Resolve the product data root (never fails).
///
/// 1. `LEBI_DATA_DIR` if non-empty (this process said so — tests / CI)
/// 2. legacy `HERMES_DATA_DIR` if non-empty
/// 3. User-chosen data root (pointer file, set via Settings migration)
/// 4. `$HOME/{DEFAULT_DATA_DIRNAME}`
/// 5. `./{DEFAULT_DATA_DIRNAME}` if `$HOME` is missing
pub fn data_root() -> PathBuf {
    if let Some(root) = env_root(ENV_DATA_DIR).or_else(|| env_root(LEGACY_ENV_DATA_DIR)) {
        return root;
    }
    if let Some(chosen) = read_data_dir_pointer() {
        return chosen;
    }
    dirs::home_dir()
        .map(|h| h.join(DEFAULT_DATA_DIRNAME))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIRNAME))
}

fn env_root(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
        .map(PathBuf::from)
}

/// `$data_root/<component>` (e.g. `sessions`, `skills`, `config.toml`).
pub fn data_path(component: impl AsRef<Path>) -> PathBuf {
    data_root().join(component)
}

/// Relative directory under the agent **workspace** for all *user-requested
/// generated artifacts* (reports, minutes, exports, drafts).
///
/// Product rule (2026-08-03):
/// - Default: put new generated files under `outputs/`
/// - Do **not** redirect edits of existing files into `outputs/`
/// - If the user names an explicit path, obey the user
pub const WORKSPACE_OUTPUTS_DIR: &str = "outputs";

/// Ensure the data root directory exists. Returns the root path.
pub fn ensure_data_root() -> std::io::Result<PathBuf> {
    let root = data_root();
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// One-time migration from the pre-branding default
/// (`~/.small-rust-hermes`) to `~/.lebi-ai`.
///
/// Rules:
/// - Pure moves: no copying, no deleting, nothing is dropped.
/// - Target absent → rename the whole directory.
/// - Target present → move the *missing* entries in; entries that already
///   exist in the target are left untouched (target wins, the legacy copy
///   stays inside the legacy dir as a backup). This covers a partially
///   populated target (e.g. created during a test run) without losing the
///   real data.
/// - Never touches the lawyer edition (`~/.lebi-law`).
/// - Only applies to the *default* home root; an explicit env override means
///   the user already chose a location.
///
/// Returns `true` when at least one entry moved. Failures are non-fatal and
/// logged by the caller; the legacy directory stays in place untouched.
pub fn maybe_migrate_data_root() -> bool {
    if env_root(ENV_DATA_DIR).is_some() || env_root(LEGACY_ENV_DATA_DIR).is_some() {
        return false;
    }
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let old = home.join(LEGACY_DATA_DIRNAME);
    let new = home.join(DEFAULT_DATA_DIRNAME);
    if !old.exists() || new == home.join(".lebi-law") {
        return false;
    }
    if !new.exists() {
        return std::fs::rename(&old, &new).is_ok();
    }
    let mut moved = 0;
    let Ok(entries) = std::fs::read_dir(&old) else {
        return false;
    };
    for entry in entries.flatten() {
        let dest = new.join(entry.file_name());
        if !dest.exists() && std::fs::rename(entry.path(), &dest).is_ok() {
            moved += 1;
        }
    }
    // Fold the legacy dir away only when every entry made it across.
    let legacy_empty = std::fs::read_dir(&old)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false);
    if moved > 0 && legacy_empty {
        let _ = std::fs::remove_dir(&old);
    }
    moved > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn env_override_wins() {
        let _g = env_lock();
        std::env::set_var(ENV_DATA_DIR, "/tmp/lebi-test-data-root");
        let root = data_root();
        std::env::remove_var(ENV_DATA_DIR);
        assert_eq!(root, PathBuf::from("/tmp/lebi-test-data-root"));
    }

    #[test]
    fn legacy_env_override_fallback() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::set_var(LEGACY_ENV_DATA_DIR, "/tmp/lebi-legacy-root");
        let root = data_root();
        std::env::remove_var(LEGACY_ENV_DATA_DIR);
        assert_eq!(root, PathBuf::from("/tmp/lebi-legacy-root"));
    }

    #[test]
    fn default_uses_product_dirname() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::remove_var(LEGACY_ENV_DATA_DIR);
        let root = data_root();
        if let Some(ptr) = read_data_dir_pointer() {
            assert_eq!(root, ptr, "user-chosen pointer wins over ~/.lebi-ai");
        } else {
            assert!(
                root.ends_with(DEFAULT_DATA_DIRNAME),
                "expected …/{DEFAULT_DATA_DIRNAME}, got {}",
                root.display()
            );
        }
    }

    #[test]
    fn migrate_renames_legacy_dir() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::remove_var(LEGACY_ENV_DATA_DIR);

        let tmp = std::env::temp_dir().join(format!("lebi-migrate-{}", std::process::id()));
        let old = tmp.join(LEGACY_DATA_DIRNAME);
        let new = tmp.join(DEFAULT_DATA_DIRNAME);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.toml"), b"[test]").unwrap();

        // Point HOME at the temp dir so the migration targets it.
        std::env::set_var("HOME", &tmp);
        let migrated = maybe_migrate_data_root();
        std::env::remove_var("HOME");

        assert!(migrated, "expected migration to run");
        assert!(!old.exists(), "legacy dir must move, not copy");
        assert!(new.join("config.toml").exists(), "content must survive");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrate_merges_missing_entries_into_existing_target() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::remove_var(LEGACY_ENV_DATA_DIR);

        let tmp = std::env::temp_dir().join(format!("lebi-migrate-merge-{}", std::process::id()));
        let old = tmp.join(LEGACY_DATA_DIRNAME);
        let new = tmp.join(DEFAULT_DATA_DIRNAME);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("keep.txt"), b"keep").unwrap();
        std::fs::write(old.join("config.toml"), b"[test]").unwrap();

        std::env::set_var("HOME", &tmp);
        let migrated = maybe_migrate_data_root();
        std::env::remove_var("HOME");

        assert!(migrated, "missing entries must be moved across");
        assert!(new.join("config.toml").exists(), "legacy entry merged in");
        assert_eq!(
            std::fs::read_to_string(new.join("keep.txt")).unwrap(),
            "keep"
        );
        assert!(!old.exists(), "fully-moved legacy dir folds away");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrate_keeps_conflicting_copy_in_legacy() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::remove_var(LEGACY_ENV_DATA_DIR);

        let tmp =
            std::env::temp_dir().join(format!("lebi-migrate-conflict-{}", std::process::id()));
        let old = tmp.join(LEGACY_DATA_DIRNAME);
        let new = tmp.join(DEFAULT_DATA_DIRNAME);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("config.toml"), b"newer").unwrap();
        std::fs::write(old.join("config.toml"), b"legacy").unwrap();
        std::fs::write(old.join("sessions.txt"), b"data").unwrap();

        std::env::set_var("HOME", &tmp);
        let migrated = maybe_migrate_data_root();
        std::env::remove_var("HOME");

        assert!(migrated, "non-conflicting entry moved");
        assert_eq!(
            std::fs::read_to_string(new.join("config.toml")).unwrap(),
            "newer"
        );
        assert_eq!(
            std::fs::read_to_string(old.join("config.toml")).unwrap(),
            "legacy"
        );
        assert!(
            !old.join("sessions.txt").exists(),
            "conflict-free entry moved out"
        );
        assert!(old.exists(), "legacy dir kept as backup for the conflict");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrate_never_touches_lawyer_dir() {
        let _g = env_lock();
        std::env::remove_var(ENV_DATA_DIR);
        std::env::remove_var(LEGACY_ENV_DATA_DIR);

        let tmp = std::env::temp_dir().join(format!("lebi-migrate-law-{}", std::process::id()));
        let old = tmp.join(".small-rust-hermes");
        let lawyer = tmp.join(".lebi-law");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&lawyer).unwrap();
        std::fs::write(lawyer.join("lawyer.txt"), b"private").unwrap();

        std::env::set_var("HOME", &tmp);
        let migrated = maybe_migrate_data_root();
        std::env::remove_var("HOME");

        // `.lebi-law` is a different directory: migration renames the legacy
        // generic dir to `.lebi-ai` and must never touch lawyer data.
        assert!(migrated, "expected migration to run");
        assert!(!old.exists(), "legacy dir must move");
        assert!(tmp.join(DEFAULT_DATA_DIRNAME).exists(), "new root created");
        assert!(lawyer.join("lawyer.txt").exists(), "lawyer data untouched");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
