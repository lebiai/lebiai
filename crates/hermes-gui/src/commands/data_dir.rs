//! Data location: show the current data root and migrate it to a new
//! directory (Settings → 数据位置). Migration copies everything, then writes
//! a system-level pointer so every entry point resolves the new root.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use hermes_core::{clear_data_dir_pointer, data_root, write_data_dir_pointer};

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DataDirView {
    pub data_root: String,
    pub workspace_root: String,
    /// True when the root was chosen by the user (pointer file), false for
    /// env-var / home defaults.
    pub user_chosen: bool,
}

fn current_view(state: &AppState) -> DataDirView {
    DataDirView {
        data_root: data_root().to_string_lossy().into_owned(),
        workspace_root: state.workspace_root(),
        user_chosen: false,
    }
}

#[tauri::command]
pub fn data_dir_get(state: State<'_, AppState>) -> Result<DataDirView, GuiError> {
    Ok(current_view(&state))
}

#[tauri::command]
pub fn data_dir_migrate(
    state: State<'_, AppState>,
    target: String,
) -> Result<DataDirView, GuiError> {
    let raw = target.trim();
    if raw.is_empty() {
        return Err(GuiError::Config("empty target directory".into()));
    }
    let target = PathBuf::from(raw);
    if !target.is_absolute() {
        return Err(GuiError::Config(format!(
            "请选择绝对路径（当前输入：{raw}）"
        )));
    }
    let current = data_root();
    if target == current {
        return Err(GuiError::Config("新位置与当前数据目录相同".into()));
    }
    if current.starts_with(&target) || target.starts_with(&current) {
        return Err(GuiError::Config(
            "新位置不能是当前数据目录的子目录或父目录".into(),
        ));
    }
    if target.join("config.toml").exists() {
        return Err(GuiError::Config(
            "目标目录已包含 lebi-AI 数据，请换一个空目录".into(),
        ));
    }
    if target.exists()
        && target
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(GuiError::Config("目标目录非空，请选择空目录".into()));
    }

    copy_dir_all(&current, &target)
        .map_err(|e| GuiError::Config(format!("复制数据失败（{e}）；原数据未受影响，请重试")))?;
    if dir_file_count(&current) != dir_file_count(&target) {
        return Err(GuiError::Config(
            "复制校验未通过，已回滚指针；原数据保持完好，请重试".into(),
        ));
    }
    write_data_dir_pointer(&target)
        .map_err(|e| GuiError::Config(format!("记录新位置失败：{e}")))?;

    Ok(DataDirView {
        data_root: target.to_string_lossy().into_owned(),
        workspace_root: state.workspace_root(),
        user_chosen: true,
    })
}

/// Open the system folder picker and return the chosen absolute path.
/// Returns None when the user cancels.
#[tauri::command]
pub async fn data_dir_pick(app: AppHandle) -> Result<Option<String>, GuiError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<std::path::PathBuf>>();
    app.dialog()
        .file()
        .set_title("选择 lebi-AI 数据目录")
        .pick_folder(move |folder| {
            let _ = tx.send(folder.and_then(|f| f.into_path().ok()));
        });
    match rx.await {
        Ok(folder) => Ok(folder.map(|p| p.to_string_lossy().into_owned())),
        Err(_) => Err(GuiError::Config("目录选择窗口已关闭，请重试".into())),
    }
}

#[tauri::command]
pub fn data_dir_reset(state: State<'_, AppState>) -> Result<DataDirView, GuiError> {
    clear_data_dir_pointer().map_err(|e| GuiError::Config(format!("重置失败：{e}")))?;
    Ok(current_view(&state))
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

fn dir_file_count(p: &Path) -> usize {
    let mut n = 0;
    let Ok(entries) = std::fs::read_dir(p) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(ty) = entry.file_type() else {
            continue;
        };
        if ty.is_dir() {
            n += dir_file_count(&entry.path());
        } else {
            n += 1;
        }
    }
    n
}
