//! MCP REST (GUI MCP commands 的 Flutter 子集；非全量 1:1)。

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
    // 现问宿主，不读启动时那份缓存：工具面只有「本轮宿主说了算」一个来源，
    // 缓存过的那份会在宿主换了之后开始说谎（与 chat 同一口径）。
    let tools = state.host.list_tools().await.unwrap_or_default();
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
