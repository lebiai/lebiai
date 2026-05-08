//! hermes-mcp: MCP client wrapper over `rmcp`.
//!
//! Exposes a single [`McpToolHost`] that aggregates one or more configured
//! MCP servers (stdio child-process or Streamable HTTP) behind the
//! [`hermes_core::ToolHost`] trait. Tool names are namespaced as
//! `<server>__<tool>` so the LLM's tool_use round-trip is unambiguous.

pub mod config;
pub mod host;
pub mod server;

pub use config::{McpConfig, ServerSpec};
pub use host::{McpHostError, McpToolHost};
pub use server::{McpServer, McpServerError};
