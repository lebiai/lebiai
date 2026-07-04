//! Config REST. 1:1 with `hermes-gui/src/commands/config.rs` — same `toml_edit`
//! in-place edit + atomic 0600 write, minus `#[tauri::command]`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, Item, Table};

use hermes_llm::Config;

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
    pub reflect_min_turns: usize,
    pub reflect_auto_accept_memories: bool,
    pub context_model_limit: usize,
    pub permissions_allow: Vec<String>,
    pub permissions_deny: Vec<String>,
    pub workspace_root: String,
    pub ui_language: String,
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".into()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<ConfigView>, ApiError> {
    let cfg = &state.config;
    let prov = cfg
        .active_provider()
        .map_err(|e| ApiError::Config(e.to_string()))?;
    Ok(Json(ConfigView {
        default_provider: cfg.default_provider.clone(),
        model: prov.model.clone(),
        max_tokens: prov.max_tokens,
        api_key_masked: mask_key(&prov.api_key),
        base_url: prov.base_url.clone(),
        reflect_min_turns: cfg.reflect.min_turns,
        reflect_auto_accept_memories: cfg.reflect.auto_accept_memories,
        context_model_limit: cfg.context.model_limit,
        permissions_allow: cfg.permissions.allow.clone(),
        permissions_deny: cfg.permissions.deny.clone(),
        workspace_root: cfg.workspace.root.to_string_lossy().into_owned(),
        ui_language: cfg.ui.language.clone(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub base_url: Option<String>,
    /// Only written when `Some(...)` and non-empty; otherwise the on-disk
    /// value is preserved (UI shows a masked placeholder).
    pub api_key: Option<String>,
    pub reflect_min_turns: Option<usize>,
    pub reflect_auto_accept_memories: Option<bool>,
    pub context_model_limit: Option<usize>,
    pub permissions_allow: Option<Vec<String>>,
    pub permissions_deny: Option<Vec<String>>,
    pub ui_language: Option<String>,
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<()>, ApiError> {
    let path = Config::default_path().map_err(|e| ApiError::Config(e.to_string()))?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Config(format!("reading {}: {e}", path.display())))?;
    let mut doc: DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| ApiError::Config(format!("parsing config.toml: {e}")))?;

    let active = state.config.default_provider.clone();
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
    if let Some(language) = update.ui_language {
        let lang = language.trim();
        if lang != "en-US" && lang != "zh-CN" {
            return Err(ApiError::Config(format!(
                "unsupported UI language: {lang}. Expected en-US or zh-CN"
            )));
        }
        let ui = ensure_table(doc.as_table_mut(), "ui");
        ui["language"] = value(lang);
    }

    write_atomically_600(&path, doc.to_string().as_bytes())?;
    Ok(Json(()))
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
    std::fs::write(&tmp, bytes)
        .map_err(|e| ApiError::Config(format!("writing temp file: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| ApiError::Config(format!("chmod 600: {e}")))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| ApiError::Config(format!("rename into place: {e}")))?;
    Ok(())
}
