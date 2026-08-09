//! MCP REST. 1:1 with `hermes-gui/src/commands/mcp.rs`.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpToolItem {
    pub name: String,
    pub description: String,
}

pub async fn list_mcp_tools(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<McpToolItem>>, ApiError> {
    let tools = state.tools.lock().await;
    Ok(Json(
        tools
            .iter()
            .map(|t| McpToolItem {
                name: t.name.clone(),
                description: t.description.clone(),
            })
            .collect(),
    ))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub name: String,
    pub kind: String,
    pub detail: String,
}

pub async fn list_mcp_servers() -> Result<Json<Vec<McpServerInfo>>, ApiError> {
    let cfg = hermes_mcp::McpConfig::load_default().map_err(|e| ApiError::Config(e.to_string()))?;
    Ok(Json(
        cfg.servers
            .iter()
            .map(|(name, spec)| {
                let (kind, detail) = match spec {
                    hermes_mcp::ServerSpec::Stdio { command, args, .. } => {
                        ("stdio".into(), format!("{} {}", command, args.join(" ")))
                    }
                    hermes_mcp::ServerSpec::Http { url, .. } => ("http".into(), url.clone()),
                };
                McpServerInfo {
                    name: name.clone(),
                    kind,
                    detail,
                }
            })
            .collect(),
    ))
}
