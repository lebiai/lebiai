//! Workspace path safety: resolve + validate that a path stays inside the
//! workspace root — or, for explicit user deliverables, under common home
//! export folders (Desktop / Documents / Downloads).

use std::path::{Path, PathBuf};

use hermes_core::{Error, Result};

/// Resolve `user_path` relative to `workspace`, then verify it stays
/// inside the workspace boundary. Returns the canonical absolute path
/// (or, for a not-yet-created file, the path under the canonical workspace).
pub fn resolve(workspace: &Path, user_path: &str) -> Result<PathBuf> {
    let expanded = expand_user_path(user_path);
    if expanded.is_absolute() && path_under_any(&normalize_path(&expanded), &user_export_roots()) {
        let candidate = normalize_path(&expanded);
        if candidate.exists() {
            return dunce_canonicalize(&candidate);
        }
        return Ok(candidate);
    }

    let ws_canon = dunce_canonicalize(workspace)?;

    let candidate = if expanded.is_absolute() {
        normalize_path(&expanded)
    } else {
        normalize_path(&ws_canon.join(user_path))
    };

    if candidate.exists() {
        let p = dunce_canonicalize(&candidate)?;
        return if is_under(&p, &ws_canon) {
            Ok(p)
        } else {
            Err(escape_err(user_path, &p, &ws_canon))
        };
    }

    if !is_under(&candidate, &ws_canon) {
        return Err(escape_err(user_path, &candidate, &ws_canon));
    }

    // Not-yet-created: keep the path under the canonical workspace so macOS
    // `/var` vs `/private/var` cannot look like an escape.
    Ok(candidate)
}

fn escape_err(user_path: &str, resolved: &Path, workspace: &Path) -> Error {
    let hint = if user_path.contains("memories") || user_path.contains("memory") {
        " Use memory_save for durable memories (not write/edit)."
    } else {
        ""
    };
    Error::ToolHost(format!(
        "path escapes workspace: {} resolves to {} which is outside {}{hint}",
        user_path,
        resolved.display(),
        workspace.display()
    ))
}

/// Prefix check that treats macOS `/var` and `/private/var` as the same root.
fn is_under(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    let p = strip_macos_private(path);
    let r = strip_macos_private(root);
    p.starts_with(&r)
}

fn strip_macos_private(p: &Path) -> PathBuf {
    match p.strip_prefix("/private") {
        Ok(rest) => Path::new("/").join(rest),
        Err(_) => p.to_path_buf(),
    }
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

/// Expand `~/`, `～/`, `$HOME/`, and a leading export folder
/// (`Desktop/`, `Documents/`, `Downloads/`, `桌面/`).
pub fn expand_user_path(user_path: &str) -> PathBuf {
    let p = user_path.trim();
    let p = p
        .strip_prefix('～')
        .map(|r| format!("~{r}"))
        .unwrap_or_else(|| p.to_string());
    if let Some(rest) = p.strip_prefix("$HOME/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if p == "$HOME" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if p == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(home) = dirs::home_dir() {
        const EXPORT: &[&str] = &["Desktop", "Documents", "Downloads", "桌面", "文稿", "下载"];
        for name in EXPORT {
            let prefix = format!("{name}/");
            if let Some(rest) = p.strip_prefix(&prefix) {
                return home.join(name).join(rest);
            }
            if p == *name {
                return home.join(name);
            }
        }
    }
    PathBuf::from(p)
}

/// Folders the user commonly wants deliverables written to (outside workspace).
pub fn user_export_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for name in ["Desktop", "Documents", "Downloads", "桌面", "文稿", "下载"] {
            roots.push(home.join(name));
        }
    }
    if let Some(d) = dirs::desktop_dir() {
        roots.push(d);
    }
    if let Some(d) = dirs::document_dir() {
        roots.push(d);
    }
    if let Some(d) = dirs::download_dir() {
        roots.push(d);
    }
    // Dedupe while preserving order
    let mut out = Vec::new();
    for r in roots {
        if !out.iter().any(|e: &PathBuf| e == &r) {
            out.push(r);
        }
    }
    out
}

fn path_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let root_ok = if root.exists() {
            dunce_canonicalize(root).ok()
        } else {
            Some(normalize_path(root))
        };
        let Some(root_c) = root_ok else {
            return false;
        };
        if path.starts_with(&root_c) {
            return true;
        }
        // Non-existing file: check ancestor
        let mut anc = path.to_path_buf();
        while !anc.exists() {
            match anc.parent() {
                Some(p) if p != anc => anc = p.to_path_buf(),
                _ => break,
            }
        }
        dunce_canonicalize(&anc)
            .map(|c| c.starts_with(&root_c))
            .unwrap_or(false)
    })
}

/// Resolve a path for write/edit: `~/` and export folders first, else workspace.
/// Returns `(absolute_path, is_export_outside_workspace)`.
///
/// `~/Desktop/x` must never be treated as workspace-relative `~/Desktop/x`.
pub fn resolve_for_write(workspace: &Path, user_path: &str) -> Result<(PathBuf, bool)> {
    let expanded = expand_user_path(user_path);
    let normalized = normalize_path(&expanded);
    if expanded.is_absolute() {
        let roots = user_export_roots();
        if path_under_any(&normalized, &roots) {
            return Ok((normalized, true));
        }
        // Absolute path inside the workspace (or a real escape) — same rules as read.
        return resolve(workspace, &expanded.to_string_lossy()).map(|p| (p, false));
    }
    resolve(workspace, user_path).map(|p| (p, false))
}

/// Resolve a local path the user asked to **open** (must already exist).
///
/// Opening is not writing: any existing file under the workspace or the
/// user's home is allowed (videos in Movies, a doc they named). System
/// paths (`/etc`, `/System`, …) and well-known secret locations stay closed.
pub fn resolve_for_open(workspace: &Path, user_path: &str) -> Result<PathBuf> {
    let expanded = expand_user_path(user_path);
    let normalized = normalize_path(&expanded);
    let candidate = if normalized.is_absolute() {
        normalized
    } else {
        normalize_join(workspace, user_path)
    };

    if !candidate.exists() {
        return Err(Error::ToolHost(format!(
            "nothing to open: {} does not exist",
            candidate.display()
        )));
    }

    let path = dunce_canonicalize(&candidate)?;
    if is_secret_open_path(&path) {
        return Err(Error::ToolHost(format!(
            "refusing to open a secret path: {}",
            path.display()
        )));
    }

    let ws = dunce_canonicalize(workspace)?;
    if path.starts_with(&ws) {
        return Ok(path);
    }

    if let Some(home) = dirs::home_dir() {
        let home = dunce_canonicalize(&home).unwrap_or(home);
        if path.starts_with(&home) {
            return Ok(path);
        }
    }

    if path_under_any(&path, &user_export_roots()) {
        return Ok(path);
    }

    Err(Error::ToolHost(format!(
        "path not allowed to open: {} (workspace or your home only)",
        path.display()
    )))
}

fn is_secret_open_path(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    const MARKERS: &[&str] = &[
        "/.ssh/",
        "/.gnupg/",
        "/.aws/",
        "/.kube/",
        "/.netrc",
        "server.token",
        "/id_rsa",
        "/id_ed25519",
        "/.ssh",
    ];
    MARKERS.iter().any(|m| {
        if *m == "/.ssh" {
            s.ends_with("/.ssh")
        } else {
            s.contains(m)
        }
    })
}

/// True when `user_path` clearly targets outside the workspace (absolute or ~/).
pub fn path_looks_outside_workspace(user_path: &str) -> bool {
    let p = user_path.trim();
    if p.starts_with("~/")
        || p == "~"
        || p.starts_with('～')
        || p.starts_with("$HOME")
        || Path::new(p).is_absolute()
    {
        return true;
    }
    [
        "Desktop/",
        "Documents/",
        "Downloads/",
        "桌面/",
        "文稿/",
        "下载/",
    ]
    .iter()
    .any(|n| p.starts_with(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_roots_include_desktop_name() {
        let roots = user_export_roots();
        assert!(!roots.is_empty());
        // At least one path ends with Desktop / 桌面 / Documents-like folder.
        let ok = roots.iter().any(|r| {
            let s = r.to_string_lossy();
            s.contains("Desktop")
                || s.contains("Documents")
                || s.contains("Downloads")
                || s.contains("桌面")
                || s.contains("文稿")
                || s.contains("下载")
        });
        assert!(ok, "roots={roots:?}");
    }

    #[test]
    fn path_looks_outside() {
        assert!(path_looks_outside_workspace("~/Desktop/a.docx"));
        assert!(path_looks_outside_workspace("/tmp/x"));
        assert!(!path_looks_outside_workspace("outputs/a.md"));
    }

    #[test]
    fn tilde_desktop_is_export_not_workspace_folder() {
        let ws = tempfile::tempdir().unwrap();
        let (path, is_export) =
            resolve_for_write(ws.path(), "~/Desktop/lebi-export-test.docx").unwrap();
        assert!(is_export, "path={}", path.display());
        let s = path.to_string_lossy();
        assert!(
            s.contains("Desktop") || s.contains("桌面"),
            "expected Desktop export, got {s}"
        );
        assert!(
            !path.starts_with(ws.path()),
            "must not write workspace/~/Desktop: {}",
            path.display()
        );
    }

    #[test]
    fn bare_desktop_folder_is_home_desktop() {
        let ws = tempfile::tempdir().unwrap();
        let (path, is_export) = resolve_for_write(ws.path(), "Desktop/lebi-bare.docx").unwrap();
        assert!(is_export, "path={}", path.display());
        assert!(
            !path.starts_with(ws.path()),
            "Desktop/ must not land in workspace: {}",
            path.display()
        );
    }

    #[test]
    fn relative_path_resolves_inside() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("a.txt"), b"").unwrap();
        let p = resolve(ws.path(), "a.txt").unwrap();
        assert!(p.starts_with(std::fs::canonicalize(ws.path()).unwrap()));
    }

    #[test]
    fn new_file_under_missing_subdir_stays_inside() {
        let ws = tempfile::tempdir().unwrap();
        let p = resolve(ws.path(), "outputs/brief.docx").unwrap();
        let ws_c = std::fs::canonicalize(ws.path()).unwrap();
        assert!(
            p.starts_with(&ws_c) || is_under(&p, &ws_c),
            "new file {} must stay under {}",
            p.display(),
            ws_c.display()
        );
        assert!(p.ends_with("outputs/brief.docx"));
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

    #[test]
    fn open_workspace_file_ok() {
        let ws = tempfile::tempdir().unwrap();
        let f = ws.path().join("clip.mp4");
        std::fs::write(&f, b"not-a-real-video").unwrap();
        let p = resolve_for_open(ws.path(), "clip.mp4").unwrap();
        assert_eq!(p, std::fs::canonicalize(&f).unwrap());
    }

    #[test]
    fn open_missing_errors() {
        let ws = tempfile::tempdir().unwrap();
        let err = resolve_for_open(ws.path(), "nope.docx").unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn open_etc_passwd_rejected() {
        let ws = tempfile::tempdir().unwrap();
        if !Path::new("/etc/passwd").exists() {
            return;
        }
        let err = resolve_for_open(ws.path(), "/etc/passwd").unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("not allowed") || s.contains("secret") || s.contains("escapes"),
            "{s}"
        );
    }
}
