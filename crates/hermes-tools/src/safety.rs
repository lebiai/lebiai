//! Workspace path safety: resolve + validate that a path stays inside the
//! workspace root.

use std::path::{Path, PathBuf};

use hermes_core::{Error, Result};

/// Resolve `user_path` relative to `workspace`, then verify it stays
/// inside the workspace boundary. Returns the canonical absolute path.
pub fn resolve(workspace: &Path, user_path: &str) -> Result<PathBuf> {
    let candidate = if Path::new(user_path).is_absolute() {
        PathBuf::from(user_path)
    } else {
        workspace.join(user_path)
    };

    // Normalize without requiring the file to exist: resolve `..` manually.
    let normalized = normalize_path(&candidate);

    let ws_canon = dunce_canonicalize(workspace)?;
    let norm_canon = if normalized.exists() {
        dunce_canonicalize(&normalized)?
    } else {
        // File doesn't exist yet (write/edit creating new). Check parent.
        let parent = normalized
            .parent()
            .ok_or_else(|| Error::ToolHost("path has no parent".into()))?;
        if !parent.exists() {
            // Will be created by mkdir_all; check the deepest existing ancestor.
            let mut ancestor = parent.to_path_buf();
            while !ancestor.exists() {
                ancestor = ancestor
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/"));
            }
            let ancestor_canon = dunce_canonicalize(&ancestor)?;
            if !ancestor_canon.starts_with(&ws_canon) {
                return Err(Error::ToolHost(format!(
                    "path escapes workspace: {} is outside {}",
                    user_path,
                    workspace.display()
                )));
            }
        }
        normalized
    };

    if !norm_canon.starts_with(&ws_canon) {
        let hint = if user_path.contains("memories") || user_path.contains("memory") {
            " Use memory_save for durable memories (not write/edit)."
        } else {
            ""
        };
        return Err(Error::ToolHost(format!(
            "path escapes workspace: {} resolves to {} which is outside {}{hint}",
            user_path,
            norm_canon.display(),
            ws_canon.display()
        )));
    }
    Ok(norm_canon)
}

/// Like `std::fs::canonicalize` but doesn't fail on macOS `/tmp` → `/private/tmp`.
fn dunce_canonicalize(p: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(p)
        .map_err(|e| Error::ToolHost(format!("cannot canonicalize {}: {e}", p.display())))
}

/// Normalize `..` and `.` components without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Join workspace + user_path and normalize (public for write.rs fallback).
pub fn normalize_join(workspace: &Path, user_path: &str) -> PathBuf {
    normalize_path(&workspace.join(user_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_resolves_inside() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("a.txt"), b"").unwrap();
        let p = resolve(ws.path(), "a.txt").unwrap();
        assert!(p.starts_with(std::fs::canonicalize(ws.path()).unwrap()));
    }

    #[test]
    fn dotdot_escape_rejected() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve(ws.path(), "../../../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("escapes workspace"));
    }

    #[test]
    fn absolute_inside_workspace_ok() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("b.txt"), b"").unwrap();
        let abs = ws.path().join("b.txt").to_string_lossy().to_string();
        let p = resolve(ws.path(), &abs).unwrap();
        assert!(p.exists());
    }

    #[test]
    fn absolute_outside_workspace_rejected() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve(ws.path(), "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("escapes workspace"));
    }
}
