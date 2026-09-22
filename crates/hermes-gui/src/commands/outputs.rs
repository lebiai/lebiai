//! 「我产出的」——工作区里的交付物，与「我的材料」分开。
//!
//! 「材料」是用户**带进来的**底料（`hermes_sources::SourceStore`）；这里是搭子
//! **产出的**结果（`workspace/outputs/…`）。两条线互不相认是有意的：产出不进
//! grounding 索引，否则搭子会把自己的草稿当成依据。
//!
//! 展示层兼容两种形态（见 `docs/records/20260913-quiet-failures-and-visible-outputs.md`）：
//! `outputs/<YYYY-MM-DD>/…`（2026-09-13 起的默认）与平铺在 `outputs/` 里的老文件。
//! 另外把**工作区根目录**的交付物也算进来 —— 搭子常常把成品直接写在根上，
//! 以前这些文件在面板里永远看不见（`docs/records/20260918-reaudit.md` P1-11）。
//!
//! 不再收录旧的 `output/`：那里留的是上一代（律师版）的产物，混进来就是串味。
//! 不迁移、不改名、不删除任何文件 —— 只是不再把它当成本产品的产出展示。

use hermes_core::companion::looks_like_code_file;

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

/// Output roots shown in the panel. `outputs` is the product default; the legacy
/// `output/` of the previous product is deliberately **not** listed.
const OUTPUT_ROOTS: &[&str] = &["outputs"];
/// Recursion cap — a stray deep tree must not stall the panel.
const MAX_DEPTH: usize = 4;
/// Newest-first cap on rows returned.
const MAX_ITEMS: usize = 500;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputItem {
    /// Workspace-relative path — the only handle the frontend passes back.
    pub rel_path: String,
    pub name: String,
    pub ext: String,
    pub bytes: u64,
    /// Local `YYYY-MM-DD HH:MM`; also the sort key.
    pub modified: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputGroup {
    /// `YYYY-MM-DD` when the file sits under a day folder; `None` → 「更早」.
    pub day: Option<String>,
    pub items: Vec<OutputItem>,
}

#[tauri::command]
pub fn list_outputs(state: State<'_, AppState>) -> Result<Vec<OutputGroup>, GuiError> {
    Ok(collect_outputs(Path::new(&state.workspace_root())))
}

#[tauri::command]
pub fn open_output(state: State<'_, AppState>, path: String) -> Result<(), GuiError> {
    let resolved = resolve_workspace_file(Path::new(&state.workspace_root()), &path)?;
    super::source::open_path(&resolved).map_err(GuiError::Internal)
}

/// Walk every output root and group by day (newest day first, 「更早」 last).
pub(crate) fn collect_outputs(workspace: &Path) -> Vec<OutputGroup> {
    let mut rows = Vec::new();
    for root in OUTPUT_ROOTS {
        walk_outputs(&workspace.join(root), workspace, 0, &mut rows);
    }
    collect_root_deliverables(workspace, &mut rows);
    group_outputs(rows)
}

/// Files sitting **directly** in the workspace root. No recursion: the root also
/// holds the agent's own working dirs, and a deliverable is a file, not a tree.
fn collect_root_deliverables(workspace: &Path, out: &mut Vec<(Option<String>, OutputItem)>) {
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_ITEMS {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() || looks_like_code_file(&name) {
            continue;
        }
        let Some(item) = output_item(&entry.path(), workspace) else {
            continue;
        };
        let day = day_of(&item.rel_path).or_else(|| date_of(&item.modified));
        out.push((day, item));
    }
}

fn walk_outputs(
    dir: &Path,
    workspace: &Path,
    depth: usize,
    out: &mut Vec<(Option<String>, OutputItem)>,
) {
    if depth > MAX_DEPTH || out.len() >= MAX_ITEMS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk_outputs(&entry.path(), workspace, depth + 1, out);
        } else if kind.is_file() {
            // A helper script the agent wrote is process, not a deliverable —
            // same rule the Care nudge uses (`hermes_core::companion`).
            if looks_like_code_file(&name) {
                continue;
            }
            let Some(item) = output_item(&entry.path(), workspace) else {
                continue;
            };
            let day = day_of(&item.rel_path).or_else(|| date_of(&item.modified));
            out.push((day, item));
            if out.len() >= MAX_ITEMS {
                return;
            }
        }
    }
}

fn output_item(path: &Path, workspace: &Path) -> Option<OutputItem> {
    let rel = path.strip_prefix(workspace).ok()?;
    let meta = std::fs::metadata(path).ok()?;
    let modified: chrono::DateTime<chrono::Local> = meta.modified().ok()?.into();
    Some(OutputItem {
        rel_path: rel.to_string_lossy().replace('\\', "/"),
        name: path.file_name()?.to_string_lossy().into_owned(),
        ext: path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase(),
        bytes: meta.len(),
        modified: modified.format("%Y-%m-%d %H:%M").to_string(),
    })
}

/// First folder under an output root, when it reads as `YYYY-MM-DD`.
fn day_of(rel_path: &str) -> Option<String> {
    let mut parts = rel_path.split('/');
    parts.next()?; // the output root itself
    is_day(parts.next()?)
}

/// `YYYY-MM-DD` out of the `YYYY-MM-DD HH:MM` stamp.
///
/// Files written before the day-folder default (2026-09-13) have no folder to
/// read, and the whole existing corpus is flat — so they group by their own
/// timestamp instead of piling into one undated bucket. Same question either
/// way: which day did this land?
fn date_of(modified: &str) -> Option<String> {
    is_day(modified.get(..10)?)
}

fn is_day(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: &[u8]| r.iter().all(u8::is_ascii_digit);
    if !digits(&b[0..4]) || !digits(&b[5..7]) || !digits(&b[8..10]) {
        return None;
    }
    let ok_range = |v: &str, lo: u32, hi: u32| {
        v.parse::<u32>()
            .map(|n| (lo..=hi).contains(&n))
            .unwrap_or(false)
    };
    if !ok_range(&s[5..7], 1, 12) || !ok_range(&s[8..10], 1, 31) {
        return None;
    }
    Some(s.to_string())
}

/// Newest day first; anything whose day cannot be read at all comes last under
/// `None`, newest first.
fn group_outputs(mut rows: Vec<(Option<String>, OutputItem)>) -> Vec<OutputGroup> {
    rows.sort_by(|a, b| {
        b.1.modified
            .cmp(&a.1.modified)
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    let mut groups: Vec<OutputGroup> = Vec::new();
    for (day, item) in rows {
        match groups.iter_mut().find(|g| g.day == day) {
            Some(g) => g.items.push(item),
            None => groups.push(OutputGroup {
                day,
                items: vec![item],
            }),
        }
    }
    groups.sort_by(|a, b| match (&a.day, &b.day) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    groups
}

/// Resolve a frontend-supplied workspace-relative path, refusing anything that
/// is not a real file **inside** the workspace.
///
/// The export-folder exemption (`~/Desktop` and friends) that `write` has does
/// **not** apply here — this opens only what the panel already listed.
pub(crate) fn resolve_workspace_file(workspace: &Path, rel: &str) -> Result<PathBuf, GuiError> {
    let rel = rel.trim();
    let refuse = |why: &str| GuiError::NotFound(format!("{why}: {rel}"));
    let mut normalized = PathBuf::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(refuse("output path must stay inside the workspace"));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(refuse("empty output path"));
    }
    let real_root = workspace
        .canonicalize()
        .map_err(|e| GuiError::Internal(format!("workspace: {e}")))?;
    // `canonicalize` also resolves symlinks, so a link pointing outside the
    // workspace fails the prefix check below.
    let real = real_root
        .join(&normalized)
        .canonicalize()
        .map_err(|_| refuse("output file is missing"))?;
    if !real.starts_with(&real_root) || !real.is_file() {
        return Err(refuse("output path escapes the workspace"));
    }
    Ok(real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn item(rel: &str, modified: &str) -> OutputItem {
        OutputItem {
            rel_path: rel.to_string(),
            name: rel.rsplit('/').next().unwrap().to_string(),
            ext: "md".to_string(),
            bytes: 1,
            modified: modified.to_string(),
        }
    }

    fn write_file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    fn today() -> String {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    #[test]
    fn day_folders_are_recognized_and_rubbish_is_not() {
        assert_eq!(
            day_of("outputs/2026-09-13/a.md").as_deref(),
            Some("2026-09-13")
        );
        assert_eq!(
            day_of("outputs/2026-09-13/deep/a.md").as_deref(),
            Some("2026-09-13")
        );
        assert_eq!(day_of("outputs/old.md"), None);
        assert_eq!(day_of("output/legacy.md"), None);
        assert_eq!(day_of("outputs/2026-13-01/a.md"), None);
        assert_eq!(day_of("outputs/README/"), None);
        assert_eq!(day_of("outputs/26-09-13/a.md"), None);
    }

    #[test]
    fn flat_files_fall_back_to_their_own_timestamp() {
        assert_eq!(date_of("2026-09-13 22:04").as_deref(), Some("2026-09-13"));
        assert_eq!(date_of("2026-09-13").as_deref(), Some("2026-09-13"));
        assert_eq!(date_of("2026-9-13 22:04"), None);
        assert_eq!(date_of(""), None);
        assert_eq!(date_of("nonsense"), None);
    }

    #[test]
    fn groups_are_newest_day_first_and_unfiled_last() {
        let groups = group_outputs(vec![
            (
                Some("2026-09-11".into()),
                item("outputs/2026-09-11/b.md", "2026-09-11 09:00"),
            ),
            (None, item("outputs/old.md", "2026-09-13 08:00")),
            (
                Some("2026-09-13".into()),
                item("outputs/2026-09-13/a.md", "2026-09-13 10:00"),
            ),
        ]);
        let days: Vec<Option<&str>> = groups.iter().map(|g| g.day.as_deref()).collect();
        assert_eq!(days, vec![Some("2026-09-13"), Some("2026-09-11"), None]);
        assert_eq!(groups[2].items[0].rel_path, "outputs/old.md");
    }

    #[test]
    fn newest_file_leads_its_day() {
        let groups = group_outputs(vec![
            (
                Some("2026-09-13".into()),
                item("outputs/2026-09-13/old.md", "2026-09-13 08:00"),
            ),
            (
                Some("2026-09-13".into()),
                item("outputs/2026-09-13/new.md", "2026-09-13 20:00"),
            ),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items[0].rel_path, "outputs/2026-09-13/new.md");
    }

    #[test]
    fn collects_dated_flat_and_root_files_but_not_the_previous_products_output_folder() {
        let ws = tempdir().unwrap();
        write_file(&ws.path().join("outputs/1999-01-01/filed.md"));
        write_file(&ws.path().join("outputs/flat.md"));
        write_file(&ws.path().join("output/legacy/report.md"));
        write_file(&ws.path().join("outputs/.hidden.md"));
        write_file(&ws.path().join("交付.md"));
        let groups = collect_outputs(ws.path());
        let total: usize = groups.iter().map(|g| g.items.len()).sum();
        assert_eq!(total, 3, "hidden files are skipped: {groups:?}");
        // The day folder wins; the flat and root files group by their own
        // timestamp — which for a fresh temp file is today.
        assert_eq!(groups[0].day.as_deref(), Some(today().as_str()));
        let today: Vec<&str> = groups[0]
            .items
            .iter()
            .map(|i| i.rel_path.as_str())
            .collect();
        assert!(today.contains(&"outputs/flat.md"));
        assert!(today.contains(&"交付.md"), "工作区根的交付物必须可见");
        assert!(
            !today.contains(&"output/legacy/report.md"),
            "上一代产品的 output/ 不该再当产出展示"
        );
        let filed: Vec<&str> = groups
            .iter()
            .find(|g| g.day.as_deref() == Some("1999-01-01"))
            .map(|g| g.items.iter().map(|i| i.rel_path.as_str()).collect())
            .unwrap();
        assert_eq!(filed, vec!["outputs/1999-01-01/filed.md"]);
    }

    #[test]
    fn root_level_scripts_and_dirs_stay_out_of_the_deliverable_list() {
        let ws = tempdir().unwrap();
        write_file(&ws.path().join("workspace.md"));
        write_file(&ws.path().join("make_report.py"));
        write_file(&ws.path().join("nested/hidden.md"));
        let listed: Vec<String> = collect_outputs(ws.path())
            .iter()
            .flat_map(|g| g.items.iter().map(|i| i.rel_path.clone()))
            .collect();
        assert_eq!(
            listed,
            vec!["workspace.md".to_string()],
            "根上只认文件、不认脚本、不下钻"
        );
    }

    #[test]
    fn helper_scripts_stay_out_of_the_deliverable_list() {
        let ws = tempdir().unwrap();
        write_file(&ws.path().join("outputs/2026-09-13/财经资讯.md"));
        write_file(&ws.path().join("outputs/2026-09-13/make_news_docx.py"));
        write_file(&ws.path().join("outputs/make_galbot_docx_v2.py"));
        write_file(&ws.path().join("outputs/brief.json"));
        write_file(&ws.path().join("outputs/report.docx"));
        let groups = collect_outputs(ws.path());
        let mut listed: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.items.iter().map(|i| i.rel_path.as_str()))
            .collect();
        listed.sort_unstable();
        assert_eq!(
            listed,
            vec!["outputs/2026-09-13/财经资讯.md", "outputs/report.docx",],
            "scripts and machine config must not show up as deliverables"
        );
    }

    #[test]
    fn resolve_refuses_escapes_and_absolute_paths() {
        let ws = tempdir().unwrap();
        write_file(&ws.path().join("outputs/2026-09-13/ok.md"));
        for bad in [
            "../etc/passwd",
            "outputs/../../etc/passwd",
            "/etc/passwd",
            "",
            "   ",
        ] {
            assert!(
                resolve_workspace_file(ws.path(), bad).is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(resolve_workspace_file(ws.path(), "outputs/2026-09-13/ok.md").is_ok());
        assert!(resolve_workspace_file(ws.path(), "outputs/missing.md").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_refuses_symlinks_pointing_outside() {
        let ws = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::fs::write(outside.path().join("secret.md"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), ws.path().join("outputs")).unwrap();
        assert!(resolve_workspace_file(ws.path(), "outputs/secret.md").is_err());
        // A link that stays inside the workspace still resolves.
        let target = ws.path().join("target.md");
        std::fs::write(&target, b"x").unwrap();
        std::os::unix::fs::symlink(&target, ws.path().join("link.md")).unwrap();
        assert!(resolve_workspace_file(ws.path(), "link.md").is_ok());
    }
}
