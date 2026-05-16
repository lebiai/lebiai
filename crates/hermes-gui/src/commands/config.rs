use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::GuiError;
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
    pub context_model_limit: usize,
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".into()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<ConfigView, GuiError> {
    let cfg = &state.config;
    let prov = cfg.active_provider().map_err(|e| GuiError::Config(e.to_string()))?;
    Ok(ConfigView {
        default_provider: cfg.default_provider.clone(),
        model: prov.model.clone(),
        max_tokens: prov.max_tokens,
        api_key_masked: mask_key(&prov.api_key),
        base_url: prov.base_url.clone(),
        reflect_min_turns: cfg.reflect.min_turns,
        context_model_limit: cfg.context.model_limit,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ConfigUpdate {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub base_url: Option<String>,
}

#[tauri::command]
pub fn update_config(
    _state: State<'_, AppState>,
    _update: ConfigUpdate,
) -> Result<(), GuiError> {
    Err(GuiError::Config("Config update requires app restart. Edit ~/.small-rust-hermes/config.toml directly.".into()))
}
