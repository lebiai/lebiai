//! Aggregator over multiple MCP servers, exposed as a single
//! [`hermes_core::ToolHost`].
//!
//! Tool name namespacing: when forwarded to the LLM each tool is renamed to
//! `<server>__<original_name>`. On a tool call coming back, we split on the
//! first `__` to find the target server, then call with the original tool
//! name. Server names are validated to contain no `__` to avoid ambiguity.

use std::collections::HashMap;

use async_trait::async_trait;
use hermes_core::{Error as CoreError, Result as CoreResult, ToolCallOutcome, ToolHost, ToolSpec};
use rmcp::model::{Content, RawContent};
use thiserror::Error;
use tracing::warn;

use crate::config::{McpConfig, ServerSpec};
use crate::server::{schema_to_value, McpServer, McpServerError};

const SEP: &str = "__";

#[derive(Debug, Error)]
pub enum McpHostError {
    #[error("server name {0:?} contains the reserved `__` separator")]
    BadServerName(String),

    #[error("no server in tool name {0:?} (expected `<server>__<tool>`)")]
    UnnamespacedTool(String),

    #[error("unknown server {server:?} for tool {tool:?}")]
    UnknownServer { server: String, tool: String },

    #[error(transparent)]
    Server(#[from] McpServerError),
}

pub struct McpToolHost {
    servers: HashMap<String, McpServer>,
}

impl McpToolHost {
    /// Connect to every server in the config. Connection failures are
    /// logged and skipped so a broken server does not bring down the whole
    /// chat session.
    pub async fn connect_all(cfg: &McpConfig) -> Result<Self, McpHostError> {
        for name in cfg.servers.keys() {
            if name.contains(SEP) {
                return Err(McpHostError::BadServerName(name.clone()));
            }
        }
        let mut servers = HashMap::new();
        for (name, spec) in &cfg.servers {
            if spec.is_disabled() {
                tracing::info!(server = %name, "skipping disabled MCP server");
                continue;
            }
            match Self::connect_one(name.clone(), spec.clone()).await {
                Ok(s) => {
                    servers.insert(name.clone(), s);
                }
                Err(e) => {
                    warn!(server = %name, error = %e, "failed to connect MCP server, skipping");
                }
            }
        }
        Ok(Self { servers })
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    async fn connect_one(name: String, spec: ServerSpec) -> Result<McpServer, McpServerError> {
        match spec {
            ServerSpec::Stdio {
                command, args, env, ..
            } => McpServer::connect_stdio(name, command, args, env).await,
            ServerSpec::Http { url, .. } => McpServer::connect_http(name, url).await,
        }
    }
}

#[async_trait]
impl ToolHost for McpToolHost {
    async fn list_tools(&self) -> CoreResult<Vec<ToolSpec>> {
        let mut out = Vec::new();
        for (server_name, server) in &self.servers {
            for tool in server.tools() {
                out.push(ToolSpec {
                    name: format!("{server_name}{SEP}{}", tool.name),
                    description: tool.description.as_deref().unwrap_or("").to_string(),
                    input_schema: schema_to_value(&tool.input_schema),
                    requires_confirmation: true,
                });
            }
        }
        Ok(out)
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> CoreResult<ToolCallOutcome> {
        let (server_name, tool_name) = name.split_once(SEP).ok_or_else(|| {
            CoreError::ToolHost(McpHostError::UnnamespacedTool(name.to_string()).to_string())
        })?;
        let server = self.servers.get(server_name).ok_or_else(|| {
            CoreError::ToolHost(
                McpHostError::UnknownServer {
                    server: server_name.to_string(),
                    tool: tool_name.to_string(),
                }
                .to_string(),
            )
        })?;

        let result = server
            .call(tool_name, args)
            .await
            .map_err(|e| CoreError::ToolHost(format!("{server_name}: {}", error_chain(&e))))?;

        let content = flatten_content(&result.content);
        Ok(ToolCallOutcome {
            content,
            is_error: result.is_error.unwrap_or(false),
        })
    }
}

/// Concatenate text blocks; describe non-text blocks inline so the LLM has
/// some signal that there was attached media / resource content.
fn flatten_content(blocks: &[Content]) -> String {
    let mut buf = String::new();
    for block in blocks {
        match &block.raw {
            RawContent::Text(t) => buf.push_str(&t.text),
            RawContent::Image(img) => {
                buf.push_str(&format!(
                    "\n[image: {} bytes, mime={}]\n",
                    img.data.len(),
                    img.mime_type
                ));
            }
            RawContent::Audio(a) => {
                buf.push_str(&format!(
                    "\n[audio: {} bytes, mime={}]\n",
                    a.data.len(),
                    a.mime_type
                ));
            }
            RawContent::Resource(_) => buf.push_str("\n[embedded resource]\n"),
            RawContent::ResourceLink(_) => buf.push_str("\n[resource link]\n"),
        }
    }
    buf
}

/// Format an error with its full source chain (`a: b: c`) so the LLM / user
/// sees the root cause of an MCP failure, not just the outermost message.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Debug)]
    struct Leaf;

    impl fmt::Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "leaf cause")
        }
    }

    impl std::error::Error for Leaf {}

    #[derive(Debug)]
    struct Mid(Leaf);

    impl fmt::Display for Mid {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "mid wrap")
        }
    }

    impl std::error::Error for Mid {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn chain_includes_root_cause() {
        let err = Mid(Leaf);
        assert_eq!(error_chain(&err), "mid wrap: leaf cause");
    }

    #[test]
    fn chain_single_layer() {
        assert_eq!(error_chain(&Leaf), "leaf cause");
    }
}
