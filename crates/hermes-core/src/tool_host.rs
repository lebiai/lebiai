//! Tool host abstraction.
//!
//! A [`ToolHost`] is an aggregator over zero or more tool sources (typically
//! MCP servers). The chat / agent loop talks to a single `ToolHost` and is
//! agnostic to whether tools come from one server or many.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::ToolSpec;
use crate::Result;

/// Result of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallOutcome {
    /// Plain-text content suitable for feeding back into the LLM's
    /// `tool_result` block. Non-text MCP content is summarised inline
    /// (e.g. `[image: 12 KB png]`).
    pub content: String,
    pub is_error: bool,
}

#[async_trait]
pub trait ToolHost: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>>;
    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome>;
}

/// A tool host that exposes nothing. Useful default when MCP is not
/// configured — the chat loop still works, just without tools.
pub struct NullToolHost;

#[async_trait]
impl ToolHost for NullToolHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        Ok(Vec::new())
    }

    async fn call(&self, name: &str, _: serde_json::Value) -> Result<ToolCallOutcome> {
        Err(crate::Error::ToolHost(format!(
            "no tool host configured; cannot call {name:?}"
        )))
    }
}
