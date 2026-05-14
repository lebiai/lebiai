use serde::Serialize;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpToolItem {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn list_mcp_tools(state: State<'_, AppState>) -> Result<Vec<McpToolItem>, GuiError> {
    let tools = state.tools.lock().await;
    Ok(tools
        .iter()
        .map(|t| McpToolItem {
            name: t.name.clone(),
            description: t.description.clone(),
        })
        .collect())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub name: String,
    pub kind: String,
    pub detail: String,
}

#[tauri::command]
pub fn list_mcp_servers(_state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, GuiError> {
    let cfg =
        hermes_mcp::McpConfig::load_default().map_err(|e| GuiError::Config(e.to_string()))?;
    Ok(cfg
        .servers
        .iter()
        .map(|(name, spec)| {
            let (kind, detail) = match spec {
                hermes_mcp::ServerSpec::Stdio { command, args, .. } => {
                    ("stdio".into(), format!("{} {}", command, args.join(" ")))
                }
                hermes_mcp::ServerSpec::Http { url } => ("http".into(), url.clone()),
            };
            McpServerInfo { name: name.clone(), kind, detail }
        })
        .collect())
}
