//! Built-in engine tools (files, shell, web, memory, skills).
//!
//! These run in-process (no MCP subprocess), are always available, and
//! enforce workspace-root boundaries for all file operations.

pub mod bash;
pub mod bash_sandbox;
pub mod commitment;
pub mod document_import;
pub mod edit;
pub mod git;
pub mod glob;
pub mod grep;
pub mod http_defaults;
pub mod memory;
pub mod office_export;
pub mod open;
pub mod palace;
pub mod read;
pub mod safety;
pub mod skill;
pub mod skill_propose;
pub mod source;
pub mod subagent;
pub mod think;
pub mod todo;
pub mod url_safety;
pub mod web;
pub mod web_cache;
pub mod web_fetch;
pub mod web_search;
pub mod write;

pub use document_import::{
    check_converter, decode_bytes_base64, import_document, ConverterPathConfig, ConverterStatus,
    ImportError, ImportRequest, ImportResult,
};

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use hermes_commitments::CommitmentStore;
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};
use hermes_memory::MemoryStore;
use hermes_skills::SkillStore;
use hermes_sources::SourceStore;

pub use skill_propose::{ProposeContext, SessionMessages, SkillProposeQueue};
pub use subagent::SubagentContext;
pub use web::{SearchBackend, WebToolsContext};

const BASIC_TOOLS: &[&str] = &[
    "read", "write", "edit", "bash", "glob", "grep", "git", "open",
];

pub struct BuiltinToolHost {
    workspace: PathBuf,
    memory_store: Option<Arc<dyn MemoryStore>>,
    commitment_store: Option<Arc<CommitmentStore>>,
    source_store: Option<Arc<SourceStore>>,
    skill_store: Option<Arc<dyn SkillStore>>,
    propose_ctx: Option<Arc<ProposeContext>>,
    subagent_ctx: Option<Arc<SubagentContext>>,
    web_ctx: Option<Arc<WebToolsContext>>,
    /// Per-session todo list (Claude Code-style single-write). Owned here so
    /// todos don't leak across sessions the way a process global would.
    todos: todo::TodoStore,
}

impl BuiltinToolHost {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            memory_store: None,
            commitment_store: None,
            source_store: None,
            skill_store: None,
            propose_ctx: None,
            subagent_ctx: None,
            web_ctx: None,
            todos: todo::TodoStore::default(),
        }
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_commitment_store(mut self, store: Arc<CommitmentStore>) -> Self {
        self.commitment_store = Some(store);
        self
    }

    pub fn with_source_store(mut self, store: Arc<SourceStore>) -> Self {
        self.source_store = Some(store);
        self
    }

    pub fn with_skill_store(mut self, store: Arc<dyn SkillStore>) -> Self {
        self.skill_store = Some(store);
        self
    }

    pub fn with_propose_ctx(mut self, ctx: Arc<ProposeContext>) -> Self {
        self.propose_ctx = Some(ctx);
        self
    }

    pub fn with_subagent_ctx(mut self, ctx: Arc<SubagentContext>) -> Self {
        self.subagent_ctx = Some(ctx);
        self
    }

    pub fn with_web_ctx(mut self, ctx: Arc<WebToolsContext>) -> Self {
        self.web_ctx = Some(ctx);
        self
    }

    pub fn handles(&self, name: &str) -> bool {
        BASIC_TOOLS.contains(&name)
            || todo::handles(name)
            || matches!(
                name,
                "think"
                    | "web_fetch"
                    | "web_search"
                    | "memory_search"
                    | "memory_save"
                    | "memory_delete"
                    | "memory_distill"
                    | "palace_zones"
                    | "palace_read_zone"
                    | "palace_recall"
                    | "skill_list"
                    | "skill_read"
                    | "skill_read_file"
                    | "skill_create"
                    | "skill_install"
                    | "skill_delete"
                    | "propose_skill"
                    | "subagent"
            )
            || (self.commitment_store.is_some() && commitment::handles(name))
            || (self.source_store.is_some() && source::handles(name))
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
            open::spec(),
            web_fetch::spec(),
            web_search::spec(),
            think::spec(),
        ];
        tools.extend(todo::specs());
        if self.memory_store.is_some() {
            tools.push(memory::spec());
            tools.push(memory::save_spec());
            tools.push(memory::delete_spec());
            tools.push(memory::distill_spec());
            tools.push(palace::zones_spec());
            tools.push(palace::read_zone_spec());
            tools.push(palace::recall_spec());
        }
        if self.skill_store.is_some() {
            tools.push(skill::list_spec());
            tools.push(skill::read_spec());
            tools.push(skill::read_file_spec());
            tools.push(skill::create_spec());
            tools.push(skill::install_spec());
            tools.push(skill::delete_spec());
        }
        if self.commitment_store.is_some() {
            tools.push(commitment::list_spec());
            tools.push(commitment::save_spec());
            tools.push(commitment::close_spec());
            tools.push(commitment::drop_spec());
            tools.push(commitment::split_spec());
            tools.push(commitment::update_spec());
        }
        if self.source_store.is_some() {
            tools.push(source::list_spec());
            tools.push(source::read_spec());
        }
        if self.propose_ctx.is_some() {
            tools.push(skill_propose::spec());
        }
        if self.subagent_ctx.is_some() {
            tools.push(subagent::spec());
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
            "open" => open::run(&self.workspace, args).await,
            "web_fetch" => web_fetch::run(&self.workspace, args, self.web_ctx.as_deref()).await,
            "web_search" => web_search::run(&self.workspace, args, self.web_ctx.as_deref()).await,
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
            "memory_distill" => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("memory_distill: no memory store configured".into())
                })?;
                memory::distill_run(store.as_ref(), args).await
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
            "skill_list" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_list: no skill store configured".into())
                })?;
                skill::list_run(store.as_ref()).await
            }
            "skill_read" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_read: no skill store configured".into())
                })?;
                skill::read_run(store.as_ref(), args).await
            }
            "skill_read_file" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_read_file: no skill store configured".into())
                })?;
                skill::read_file_run(store.as_ref(), args).await
            }
            "skill_create" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_create: no skill store configured".into())
                })?;
                skill::create_run(store.as_ref(), args).await
            }
            "skill_install" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_install: no skill store configured".into())
                })?;
                skill::install_run(store.clone(), args).await
            }
            "skill_delete" => {
                let store = self.skill_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("skill_delete: no skill store configured".into())
                })?;
                skill::delete_run(store.clone(), args).await
            }
            "propose_skill" => {
                let ctx = self.propose_ctx.as_ref().ok_or_else(|| {
                    Error::ToolHost("propose_skill: not wired up (provider/session missing)".into())
                })?;
                skill_propose::run(ctx, args).await
            }
            "subagent" => {
                let ctx = self.subagent_ctx.as_ref().ok_or_else(|| {
                    Error::ToolHost("subagent: not wired up (provider/turn config missing)".into())
                })?;
                subagent::run(ctx, args).await
            }
            n if commitment::handles(n) => {
                let store = self.commitment_store.as_ref().ok_or_else(|| {
                    Error::ToolHost(format!("{n}: no commitment store configured"))
                })?;
                commitment::run(store.as_ref(), n, args).await
            }
            "source_list" => {
                let store = self.source_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("source_list: no source store configured".into())
                })?;
                source::list_run(store.as_ref()).await
            }
            "source_read" => {
                let store = self.source_store.as_ref().ok_or_else(|| {
                    Error::ToolHost("source_read: no source store configured".into())
                })?;
                source::read_run(store.as_ref(), args).await
            }
            n if todo::handles(n) => todo::run(&self.todos, &self.workspace, n, args).await,
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

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::FsMemoryStore;
    use tempfile::tempdir;

    /// Regression guard: every tool that `list_tools` advertises must be
    /// recognised by `handles()`. `CompositeToolHost::call` routes via
    /// `handles` — a tool listed but not handled surfaces as "unknown tool"
    /// at call time (the exact bug that shipped `memory_distill` listed but
    /// uncallable). This test fails the moment a new tool is added to
    /// `list_tools` without being added to the `handles` whitelist.
    #[tokio::test]
    async fn every_listed_tool_is_handled() {
        let dir = tempdir().unwrap();
        let store: Arc<dyn MemoryStore> =
            Arc::new(FsMemoryStore::new(dir.path().to_path_buf(), None));
        let commitments = Arc::new(hermes_commitments::CommitmentStore::new(
            dir.path().join("commitments.json"),
        ));
        let sources =
            Arc::new(hermes_sources::SourceStore::open(dir.path().join("sources")).unwrap());
        let host = BuiltinToolHost::new(dir.path().to_path_buf())
            .with_memory_store(store)
            .with_commitment_store(commitments)
            .with_source_store(sources);

        let tools = host.list_tools().await.unwrap();
        assert!(!tools.is_empty(), "memory store wired → tools expected");
        for spec in &tools {
            assert!(
                host.handles(&spec.name),
                "tool {:?} is advertised by list_tools but NOT recognised by handles() \
                 — CompositeToolHost::call will reject it as 'unknown tool'. \
                 Add it to the handles() whitelist in lib.rs.",
                spec.name
            );
        }
    }
}
