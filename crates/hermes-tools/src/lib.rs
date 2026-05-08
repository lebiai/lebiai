//! Built-in tools for the self-evolving agent.
//!
//! These run in-process (no MCP subprocess), are always available, and
//! enforce workspace-root boundaries for all file operations.

pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod safety;
pub mod todo;
pub mod web_fetch;
pub mod web_search;
pub mod write;

use std::path::PathBuf;

use async_trait::async_trait;
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};

const BASIC_TOOLS: &[&str] = &["read", "write", "edit", "bash", "glob", "grep"];

pub struct BuiltinToolHost {
    workspace: PathBuf,
}

impl BuiltinToolHost {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub fn handles(&self, name: &str) -> bool {
        BASIC_TOOLS.contains(&name)
            || todo::handles(name)
            || matches!(name, "web_fetch" | "web_search")
    }
}

#[async_trait]
impl ToolHost for BuiltinToolHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        let mut tools = vec![
            read::spec(),
            write::spec(),
            edit::spec(),
            bash::spec(),
            glob::spec(),
            grep::spec(),
            web_fetch::spec(),
            web_search::spec(),
        ];
        tools.extend(todo::specs());
        Ok(tools)
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
        match name {
            "read" => read::run(&self.workspace, args).await,
            "write" => write::run(&self.workspace, args).await,
            "edit" => edit::run(&self.workspace, args).await,
            "bash" => bash::run(&self.workspace, args).await,
            "glob" => glob::run(&self.workspace, args).await,
            "grep" => grep::run(&self.workspace, args).await,
            "web_fetch" => web_fetch::run(&self.workspace, args).await,
            "web_search" => web_search::run(&self.workspace, args).await,
            n if todo::handles(n) => todo::run(&self.workspace, n, args).await,
            _ => Err(Error::ToolHost(format!("unknown built-in tool: {name}"))),
        }
    }
}

/// Composite host: built-in tools first, then MCP tools.
pub struct CompositeToolHost {
    pub builtin: BuiltinToolHost,
    pub mcp: Option<Box<dyn ToolHost>>,
}

#[async_trait]
impl ToolHost for CompositeToolHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        let mut tools = self.builtin.list_tools().await?;
        if let Some(mcp) = &self.mcp {
            tools.extend(mcp.list_tools().await?);
        }
        Ok(tools)
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
        if self.builtin.handles(name) {
            return self.builtin.call(name, args).await;
        }
        if let Some(mcp) = &self.mcp {
            return mcp.call(name, args).await;
        }
        Err(Error::ToolHost(format!("unknown tool: {name}")))
    }
}
