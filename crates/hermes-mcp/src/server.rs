//! Single MCP server connection (stdio child process or Streamable HTTP).

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::ServiceExt;
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum McpServerError {
    #[error("transport setup for server {server:?}: {source}")]
    Transport {
        server: String,
        #[source]
        source: std::io::Error,
    },

    #[error("rmcp service error for server {server:?}: {source}")]
    Service {
        server: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("tool arguments must be a JSON object or null, got {0}")]
    BadArguments(&'static str),
}

pub type Result<T> = std::result::Result<T, McpServerError>;

/// One live MCP server connection plus its cached tool list.
pub struct McpServer {
    name: String,
    peer: Peer<RoleClient>,
    /// Holds the connection alive; dropping cancels the service.
    _service: RunningService<RoleClient, ()>,
    tools: Vec<Tool>,
}

impl McpServer {
    pub async fn connect_stdio(
        name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(&command);
        cmd.args(&args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let transport =
            TokioChildProcess::new(cmd).map_err(|source| McpServerError::Transport {
                server: name.clone(),
                source,
            })?;
        let service = ().serve(transport).await.map_err(|e| boxed_service_error(&name, e))?;
        Self::finalize(name, service).await
    }

    pub async fn connect_http(name: String, url: String) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(url);
        let service = ().serve(transport).await.map_err(|e| boxed_service_error(&name, e))?;
        Self::finalize(name, service).await
    }

    async fn finalize(name: String, service: RunningService<RoleClient, ()>) -> Result<Self> {
        let peer = service.peer().clone();
        let tools = peer
            .list_tools(Default::default())
            .await
            .map_err(|e| boxed_service_error(&name, e))?
            .tools;
        Ok(Self {
            name,
            peer,
            _service: service,
            tools,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    pub async fn call(&self, tool_name: &str, args: serde_json::Value) -> Result<CallToolResult> {
        let params = match args {
            serde_json::Value::Object(map) => {
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(map)
            }
            serde_json::Value::Null => CallToolRequestParams::new(tool_name.to_string()),
            serde_json::Value::String(_) => return Err(McpServerError::BadArguments("string")),
            serde_json::Value::Number(_) => return Err(McpServerError::BadArguments("number")),
            serde_json::Value::Bool(_) => return Err(McpServerError::BadArguments("bool")),
            serde_json::Value::Array(_) => return Err(McpServerError::BadArguments("array")),
        };
        self.peer
            .call_tool(params)
            .await
            .map_err(|e| boxed_service_error(&self.name, e))
    }
}

fn boxed_service_error<E>(server: &str, e: E) -> McpServerError
where
    E: std::error::Error + Send + Sync + 'static,
{
    McpServerError::Service {
        server: server.to_string(),
        source: Box::new(e),
    }
}

/// Convert an rmcp Tool's input_schema (Arc<JsonObject>) into a plain
/// serde_json::Value for transport to the LLM.
pub(crate) fn schema_to_value(
    schema: &Arc<serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Value {
    serde_json::Value::Object(schema.as_ref().clone())
}
