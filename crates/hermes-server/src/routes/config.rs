//! Config REST. 1:1 with `hermes-gui/src/commands/config.rs` — same `toml_edit`
//! in-place edit + atomic 0600 write, minus `#[tauri::command]`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, Item, Table};

use hermes_core::{clear_data_dir_pointer, data_root, write_data_dir_pointer};
use hermes_llm::{Config, PROVIDER_PRESETS};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub default_provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub api_key_masked: String,
    pub base_url: String,
    /// Every bundled provider preset with its current on-disk values (falls
    /// back to the preset when the config section is absent). Drives the
    /// Settings provider selector.
    pub providers: Vec<ProviderOption>,
    pub reflect_min_turns: usize,
    pub reflect_auto_accept_memories: bool,
    pub context_model_limit: usize,
    pub permissions_allow: Vec<String>,
    pub permissions_deny: Vec<String>,
    pub workspace_root: String,
    /// Product data root — movable via `data_dir_migrate` (1:1 with GUI).
    pub data_dir: String,
    pub ui_language: String,
    pub ui_theme: String,
    pub persist_thinking: bool,
    pub has_api_key: bool,
}

/// One selectable provider for the Settings selector.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOption {
    pub key: String,
    pub model: String,
    pub max_tokens: u32,
    pub base_url: String,
    pub api_key_masked: String,
    pub has_api_key: bool,
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".into()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<ConfigView>, ApiError> {
    // Prefer on-disk config (same as GUI): update_config writes disk only.
    let cfg = Config::load_default().unwrap_or_else(|_| state.config.read().unwrap().clone());
    let prov = cfg
        .active_provider()
        .map_err(|e| ApiError::Config(e.to_string()))?;
    let providers = PROVIDER_PRESETS
        .iter()
        .map(|preset| {
            let section = cfg.providers.get(preset.key);
            let (model, max_tokens, base_url, api_key) = match section {
                Some(s) => (
                    s.model.clone(),
                    s.max_tokens,
                    s.base_url.clone(),
                    s.api_key.clone(),
                ),
                None => (
                    preset.model.to_string(),
                    preset.max_tokens,
                    preset.base_url.to_string(),
                    String::new(),
                ),
            };
            ProviderOption {
                key: preset.key.to_string(),
                model,
                max_tokens,
                base_url,
                api_key_masked: mask_key(&api_key),
                has_api_key: !api_key.trim().is_empty(),
            }
        })
        .collect();
    Ok(Json(ConfigView {
        default_provider: cfg.default_provider.clone(),
        model: prov.model.clone(),
        max_tokens: prov.max_tokens,
        api_key_masked: mask_key(&prov.api_key),
        base_url: prov.base_url.clone(),
        providers,
        reflect_min_turns: cfg.reflect.min_turns,
        reflect_auto_accept_memories: cfg.reflect.auto_accept_memories,
        context_model_limit: cfg.context.model_limit,
        permissions_allow: cfg.permissions.allow.clone(),
        permissions_deny: cfg.permissions.deny.clone(),
        workspace_root: cfg.workspace.root.to_string_lossy().into_owned(),
        data_dir: hermes_core::data_root().to_string_lossy().into_owned(),
        ui_language: cfg.ui.language.clone(),
        ui_theme: cfg.ui.theme.clone(),
        persist_thinking: cfg.ui.persist_thinking,
        has_api_key: !prov.api_key.trim().is_empty(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    /// Switch the active provider (must be one of `PROVIDER_PRESETS`).
    pub default_provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub base_url: Option<String>,
    /// Only written when `Some(...)` and non-empty; otherwise the on-disk
    /// value is preserved (UI shows a masked placeholder).
    pub api_key: Option<String>,
    /// Explicitly remove the on-disk API key for the active provider.
    #[serde(default)]
    pub clear_api_key: bool,
    pub reflect_min_turns: Option<usize>,
    pub reflect_auto_accept_memories: Option<bool>,
    pub context_model_limit: Option<usize>,
    pub permissions_allow: Option<Vec<String>>,
    pub permissions_deny: Option<Vec<String>>,
    pub ui_language: Option<String>,
    pub ui_theme: Option<String>,
    #[serde(default, alias = "persist_thinking")]
    pub persist_thinking: Option<bool>,
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<()>, ApiError> {
    // Validate the provider switch before touching the file.
    if let Some(provider) = update.default_provider.as_deref() {
        if !PROVIDER_PRESETS.iter().any(|p| p.key == provider) {
            return Err(ApiError::Config(format!("unknown provider: {provider}")));
        }
    }

    let path = Config::default_path().map_err(|e| ApiError::Config(e.to_string()))?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Config(format!("reading {}: {e}", path.display())))?;
    let mut doc: DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| ApiError::Config(format!("parsing config.toml: {e}")))?;

    let active = update
        .default_provider
        .clone()
        .unwrap_or_else(|| state.config.read().unwrap().default_provider.clone());
    if update.default_provider.is_some() {
        doc["default_provider"] = value(active.clone());
    }
    {
        let provider_table = ensure_table(doc.as_table_mut(), "providers");
        let provider_entry = ensure_table(provider_table, &active);

        if let Some(model) = update.model {
            provider_entry["model"] = value(model);
        }
        if let Some(max_tokens) = update.max_tokens {
            provider_entry["max_tokens"] = value(max_tokens as i64);
        }
        if let Some(base_url) = update.base_url {
            provider_entry["base_url"] = value(base_url);
        }
        if let Some(api_key) = update.api_key {
            if !api_key.trim().is_empty() {
                provider_entry["api_key"] = value(api_key);
            }
        }
        if update.clear_api_key {
            provider_entry["api_key"] = value("");
        }
    }

    if let Some(min_turns) = update.reflect_min_turns {
        let reflect = ensure_table(doc.as_table_mut(), "reflect");
        reflect["min_turns"] = value(min_turns as i64);
    }
    if let Some(auto) = update.reflect_auto_accept_memories {
        let reflect = ensure_table(doc.as_table_mut(), "reflect");
        reflect["auto_accept_memories"] = value(auto);
    }
    if let Some(limit) = update.context_model_limit {
        let context = ensure_table(doc.as_table_mut(), "context");
        context["model_limit"] = value(limit as i64);
    }
    if let Some(allow) = update.permissions_allow {
        let perms = ensure_table(doc.as_table_mut(), "permissions");
        perms["allow"] = value(toml_edit::Array::from_iter(allow));
    }
    if let Some(deny) = update.permissions_deny {
        let perms = ensure_table(doc.as_table_mut(), "permissions");
        perms["deny"] = value(toml_edit::Array::from_iter(deny));
    }
    if update.ui_language.is_some()
        || update.ui_theme.is_some()
        || update.persist_thinking.is_some()
    {
        let ui = ensure_table(doc.as_table_mut(), "ui");
        if let Some(language) = update.ui_language.as_ref() {
            let lang = language.trim();
            if lang != "en-US" && lang != "zh-CN" {
                return Err(ApiError::Config(format!(
                    "unsupported UI language: {lang}. Expected en-US or zh-CN"
                )));
            }
            ui["language"] = value(lang);
        }
        if let Some(theme) = update.ui_theme.as_ref() {
            let theme = theme.trim();
            if theme != "system" && theme != "light" && theme != "dark" {
                return Err(ApiError::Config(format!(
                    "unsupported UI theme: {theme}. Expected system, light, or dark"
                )));
            }
            ui["theme"] = value(theme);
        }
        if let Some(persist) = update.persist_thinking {
            ui["persist_thinking"] = value(persist);
        }
    }

    write_atomically_600(&path, doc.to_string().as_bytes())?;

    // Hot-swap: rebind the provider and in-memory config from disk so a new
    // API key or provider applies without restarting the server.
    let fresh = Config::load_default().map_err(|e| ApiError::Config(e.to_string()))?;
    let provider = fresh
        .build_active_provider()
        .map_err(|e| ApiError::Config(e.to_string()))?;
    *state.config.write().unwrap() = fresh;
    *state.provider.write().unwrap() = provider;
    Ok(Json(()))
}

// ---------------------------------------------------------------------------
// Data location (1:1 with `hermes-gui/src/commands/data_dir.rs`)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DataDirView {
    pub data_root: String,
    pub workspace_root: String,
    pub user_chosen: bool,
}

fn current_view(state: &AppState) -> DataDirView {
    DataDirView {
        data_root: data_root().to_string_lossy().into_owned(),
        workspace_root: state.workspace_root(),
        user_chosen: false,
    }
}

pub async fn data_dir_get(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DataDirView>, ApiError> {
    Ok(Json(current_view(&state)))
}

pub async fn data_dir_migrate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DataDirMigrateRequest>,
) -> Result<Json<DataDirView>, ApiError> {
    let raw = req.target.trim().to_string();
    if raw.is_empty() {
        return Err(ApiError::Config("empty target directory".into()));
    }
    let target = std::path::PathBuf::from(&raw);
    if !target.is_absolute() {
        return Err(ApiError::Config(format!(
            "请选择绝对路径（当前输入：{raw}）"
        )));
    }
    let current = data_root();
    if target == current {
        return Err(ApiError::Config("新位置与当前数据目录相同".into()));
    }
    if current.starts_with(&target) || target.starts_with(&current) {
        return Err(ApiError::Config(
            "新位置不能是当前数据目录的子目录或父目录".into(),
        ));
    }
    if target.join("config.toml").exists() {
        return Err(ApiError::Config(
            "目标目录已包含 lebi-AI 数据，请换一个空目录".into(),
        ));
    }
    if target.exists()
        && target
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(ApiError::Config("目标目录非空，请选择空目录".into()));
    }

    copy_dir_all(&current, &target)
        .map_err(|e| ApiError::Config(format!("复制数据失败（{e}）；原数据未受影响，请重试")))?;
    if dir_file_count(&current) != dir_file_count(&target) {
        return Err(ApiError::Config(
            "复制校验未通过，已回滚指针；原数据保持完好，请重试".into(),
        ));
    }
    write_data_dir_pointer(&target)
        .map_err(|e| ApiError::Config(format!("记录新位置失败：{e}")))?;

    Ok(Json(DataDirView {
        data_root: target.to_string_lossy().into_owned(),
        workspace_root: state.workspace_root(),
        user_chosen: true,
    }))
}

pub async fn data_dir_reset(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DataDirView>, ApiError> {
    clear_data_dir_pointer().map_err(|e| ApiError::Config(format!("重置失败：{e}")))?;
    Ok(Json(current_view(&state)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirMigrateRequest {
    pub target: String,
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
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

fn dir_file_count(p: &std::path::Path) -> usize {
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

fn ensure_table<'a>(parent: &'a mut Table, key: &str) -> &'a mut Table {
    if !parent.contains_key(key) {
        parent.insert(key, Item::Table(Table::new()));
    }
    parent[key].as_table_mut().expect("ensured table")
}

fn write_atomically_600(path: &PathBuf, bytes: &[u8]) -> Result<(), ApiError> {
    let dir = path.parent().ok_or_else(|| {
        ApiError::Config(format!("config path has no parent: {}", path.display()))
    })?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config.toml".into())
    ));
    std::fs::write(&tmp, bytes).map_err(|e| ApiError::Config(format!("writing temp file: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| ApiError::Config(format!("chmod 600: {e}")))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| ApiError::Config(format!("rename into place: {e}")))?;
    Ok(())
}
