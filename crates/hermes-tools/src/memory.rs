//! Memory tools: search, save, and delete episodic memories.

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use hermes_memory::{Confidence, MemoryFrontmatter, MemoryStore, Scope, Source};
use serde::Deserialize;

// --- memory_search ---

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    5
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "memory_search".into(),
        description: "Search your episodic memories for information relevant to a query. Returns the most relevant memory bodies.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What to search for"},
                "limit": {"type": "integer", "description": "Max results (default 5)"}
            },
            "required": ["query"]
        }),
    }
}

pub async fn run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: SearchArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_search: bad args: {e}")))?;

    let hits = store
        .search(&a.query, a.limit)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_search: {e}")))?;

    if hits.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no memories matching: {}", a.query),
            is_error: false,
        });
    }

    let out = hits
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let tags = if m.frontmatter.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", m.frontmatter.tags.join(", "))
            };
            format!(
                "{}. (id={} scope={:?} conf={:?}{tags}) {}",
                i + 1,
                m.frontmatter.id,
                m.scope,
                m.frontmatter.confidence,
                m.body.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

// --- memory_save ---

#[derive(Deserialize)]
struct SaveArgs {
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_zone")]
    zone: String,
}

fn default_zone() -> String {
    "general".to_string()
}

pub fn save_spec() -> ToolSpec {
    ToolSpec {
        name: "memory_save".into(),
        description: "Save a piece of knowledge or insight for future reference. \
            Use this when you discover something worth remembering: a useful approach, \
            a user preference, a working solution, or a lesson learned from a mistake. \
            Memories persist across conversations."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "The insight or knowledge to remember"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags for retrieval (e.g. ['weather', 'api'])"},
                "zone": {"type": "string", "description": "Memory zone: core (identity/preferences), work (current focus), project:<name> (per-project), episode (session summaries), general (default)", "default": "general"}
            },
            "required": ["content"]
        }),
    }
}

pub async fn save_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: SaveArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_save: bad args: {e}")))?;

    if a.content.trim().is_empty() {
        return Ok(ToolCallOutcome {
            content: "memory_save: content must not be empty".into(),
            is_error: true,
        });
    }

    let fm = MemoryFrontmatter::new(Source::User, Confidence::High, a.tags, a.zone);
    let id = fm.id.clone();

    match store.put(Scope::User, fm, &a.content) {
        Ok(path) => Ok(ToolCallOutcome {
            content: format!("Saved memory {id} → {}", path.display()),
            is_error: false,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("memory_save failed: {e}"),
            is_error: true,
        }),
    }
}

// --- memory_delete ---

#[derive(Deserialize)]
struct DeleteArgs {
    id: String,
}

pub fn delete_spec() -> ToolSpec {
    ToolSpec {
        name: "memory_delete".into(),
        description: "Delete an outdated or incorrect memory by its ID. \
            Use memory_search first to find the ID."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "The memory ID to delete (e.g. mem_abc123)"}
            },
            "required": ["id"]
        }),
    }
}

pub async fn delete_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: DeleteArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_delete: bad args: {e}")))?;

    match store.delete(Scope::User, &a.id) {
        Ok(true) => Ok(ToolCallOutcome {
            content: format!("Deleted memory: {}", a.id),
            is_error: false,
        }),
        Ok(false) => Ok(ToolCallOutcome {
            content: format!("Memory not found: {}", a.id),
            is_error: true,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("memory_delete failed: {e}"),
            is_error: true,
        }),
    }
}
