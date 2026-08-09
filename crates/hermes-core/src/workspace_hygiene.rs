//! One-shot hygiene for the generic lebi-AI workspace.
//!
//! After product data isolation, lawyer-era files may still sit under
//! `workspace/` and pollute agent context (identity / case reports).
//! On startup we move **top-level** files whose names match known
//! lawyer markers into `_quarantine_lawyer/workspace/`.

use std::path::{Path, PathBuf};

/// Substrings (case-sensitive for CJK, ASCII matched case-insensitively)
/// that mark leftover lawyer product materials.
const LAWYER_NAME_MARKERS: &[&str] = &[
    "离婚律师",
    "继承纠纷",
    "再审案",
    "民初",
    "民再",
    "判决",
    "法条",
    "案件深度分析",
    "案件卡片",
    "重点记忆_刘",
    "用户身份_离婚",
    "litigation",
    "statute",
    "judgment",
];

fn name_looks_lawyer(name: &str) -> bool {
    let lower = name.to_lowercase();
    LAWYER_NAME_MARKERS.iter().any(|m| {
        if m.is_ascii() {
            lower.contains(&m.to_lowercase())
        } else {
            name.contains(m)
        }
    })
}

/// Move matching top-level files from `workspace` into
/// `{data_root}/_quarantine_lawyer/workspace/`.
///
/// Does **not** recurse into subfolders (user `uploads/` stays).
/// Returns the number of files moved.
pub fn quarantine_lawyer_workspace_files(
    data_root: &Path,
    workspace: &Path,
) -> std::io::Result<usize> {
    if !workspace.is_dir() {
        return Ok(0);
    }
    let dest_root = data_root.join("_quarantine_lawyer").join("workspace");
    let mut moved = 0usize;

    for entry in std::fs::read_dir(workspace)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Leave OS junk and generic notes alone unless they match.
        if name == ".DS_Store" || name == "TODOS.md" {
            continue;
        }
        if !name_looks_lawyer(name) {
            continue;
        }
        std::fs::create_dir_all(&dest_root)?;
        let dest = unique_dest(&dest_root, name);
        match std::fs::rename(&path, &dest) {
            Ok(()) => moved += 1,
            Err(_) => {
                // Cross-device: copy + remove
                if std::fs::copy(&path, &dest).is_ok() && std::fs::remove_file(&path).is_ok() {
                    moved += 1;
                }
            }
        }
    }
    Ok(moved)
}

fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for i in 1..1000 {
        let p = dir.join(format!("{stem}_{i}{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{stem}_{}{ext}", chrono::Utc::now().timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lawyer_names() {
        assert!(name_looks_lawyer("用户身份_离婚律师.md"));
        assert!(name_looks_lawyer("刘某等继承纠纷再审案_分析报告.md"));
        assert!(!name_looks_lawyer("短视频运营手册.md"));
        assert!(!name_looks_lawyer("TODOS.md"));
    }

    #[test]
    fn moves_matching_top_level_only() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let ws = data.join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("用户身份_离婚律师.md"), b"x").unwrap();
        std::fs::write(ws.join("短视频运营手册.md"), b"y").unwrap();
        std::fs::create_dir_all(ws.join("uploads")).unwrap();
        std::fs::write(ws.join("uploads").join("继承纠纷.pdf"), b"z").unwrap();

        let n = quarantine_lawyer_workspace_files(data, &ws).unwrap();
        assert_eq!(n, 1);
        assert!(!ws.join("用户身份_离婚律师.md").exists());
        assert!(ws.join("短视频运营手册.md").exists());
        assert!(ws.join("uploads").join("继承纠纷.pdf").exists());
        assert!(data
            .join("_quarantine_lawyer/workspace/用户身份_离婚律师.md")
            .exists());
    }
}
