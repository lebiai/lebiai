//! Ensure session load/delete paths stay under the product sessions root.

use std::path::{Path, PathBuf};

use crate::session::SessionError;

/// Absolute sessions root: `~/.lebi-ai/sessions` (or `$LEBI_DATA_DIR/sessions`).
pub fn sessions_root() -> PathBuf {
    hermes_core::data_path("sessions")
}

fn io_err(path: impl Into<PathBuf>, source: std::io::Error) -> SessionError {
    SessionError::Io {
        path: path.into(),
        source,
    }
}

/// Resolve `user_path` and require it to live under [`sessions_root`].
/// Rejects path traversal and absolute paths outside the sessions tree.
pub fn ensure_session_path(user_path: &str) -> Result<PathBuf, SessionError> {
    let trimmed = user_path.trim();
    if trimmed.is_empty() {
        return Err(io_err(
            "session path",
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "session path is empty"),
        ));
    }

    let root = sessions_root();
    std::fs::create_dir_all(&root).map_err(|source| io_err(&root, source))?;
    let root_canon = std::fs::canonicalize(&root).map_err(|source| io_err(&root, source))?;

    let raw = PathBuf::from(trimmed);
    let candidate = if raw.is_absolute() {
        raw
    } else {
        root.join(raw)
    };
    let normalized = normalize_path(&candidate);

    let checked = if normalized.exists() {
        std::fs::canonicalize(&normalized).map_err(|source| io_err(&normalized, source))?
    } else {
        let parent = normalized.parent().ok_or_else(|| {
            io_err(
                &normalized,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "session path has no parent",
                ),
            )
        })?;
        let mut ancestor = parent.to_path_buf();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"));
        }
        let anc = std::fs::canonicalize(&ancestor).map_err(|source| io_err(&ancestor, source))?;
        if !anc.starts_with(&root_canon) {
            return Err(path_escape_err(trimmed, &root_canon));
        }
        normalized
    };

    if !checked.starts_with(&root_canon) {
        return Err(path_escape_err(trimmed, &root_canon));
    }
    Ok(checked)
}

fn path_escape_err(user_path: &str, root: &Path) -> SessionError {
    io_err(
        user_path,
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "session path escapes sessions root (allowed under {})",
                root.display()
            ),
        ),
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_etc_passwd() {
        let err = ensure_session_path("/etc/passwd").unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("escapes") || s.contains("Permission") || s.contains("permission"),
            "{s}"
        );
    }

    #[test]
    fn rejects_dotdot_escape() {
        let err = ensure_session_path("../../../etc/passwd").unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("escapes") || s.contains("Permission") || s.contains("permission"),
            "{s}"
        );
    }
}
