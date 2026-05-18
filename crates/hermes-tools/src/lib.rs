//! Built-in tools for the self-evolving agent.
//!
//! These run in-process (no MCP subprocess), are always available, and
//! enforce workspace-root boundaries for all file operations.

pub mod bash;
pub mod edit;
pub mod git;
pub mod glob;
pub mod grep;
pub mod memory;
pub mod palace;
pub mod read;
pub mod safety;
pub mod think;
pub mod todo;
pub mod web_fetch;
pub mod web_search;
pub mod write;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};
use hermes_memory::MemoryStore;

const BASIC_TOOLS: &[&str] = &["read", "write", "edit", "bash", "glob", "grep", "git"];

pub struct BuiltinToolHost {
    workspace: PathBuf,
    memory_store: Option<Arc<dyn MemoryStore>>,
}

impl BuiltinToolHost {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            memory_store: None,
        }
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn handles(&self, name: &str) -> bool {
        BASIC_TOOLS.contains(&name)
            || todo::handles(name)
            || matches!(name, "think" | "web_fetch" | "web_search" | "memory_search" | "memory_save" | "memory_delete" | "palace_zones" | "palace_read_zone" | "palace_recall")
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
            git::spec(),
            web_fetch::spec(),
            web_search::spec(),
            think::spec(),
        ];
        tools.extend(todo::specs());
        if self.memory_store.is_some() {
            tools.push(memory::spec());
            tools.push(memory::save_spec());
            tools.push(memory::delete_spec());
            tools.push(palace::zones_spec());
            tools.push(palace::read_zone_spec());
            tools.push(palace::recall_spec());
        }
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
            "git" => git::run(&self.workspace, args).await,
            "web_fetch" => web_fetch::run(&self.workspace, args).await,
            "web_search" => web_search::run(&self.workspace, args).await,
            "think" => think::run(args).await,
            "memory_search" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("memory_search: no memory store configured".into())
                })?;
                memory::run(store.as_ref(), args).await
            }
            "memory_save" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("memory_save: no memory store configured".into())
                })?;
                memory::save_run(store.as_ref(), args).await
            }
            "memory_delete" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("memory_delete: no memory store configured".into())
                })?;
                memory::delete_run(store.as_ref(), args).await
            }
            "palace_zones" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("palace_zones: no memory store configured".into())
                })?;
                palace::zones_run(store.as_ref()).await
            }
            "palace_read_zone" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("palace_read_zone: no memory store configured".into())
                })?;
                palace::read_zone_run(store.as_ref(), args).await
            }
            "palace_recall" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("palace_recall: no memory store configured".into())
                })?;
                palace::recall_run(store.as_ref(), args).await
            }
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
        if name.contains("__") {
            if let Some(mcp) = &self.mcp {
                return mcp.call(name, args).await;
            }
        }
        Err(Error::ToolHost(format!(
            "unknown tool: {name}. Use one of the tools listed in your tool definitions."
        )))
    }
}
