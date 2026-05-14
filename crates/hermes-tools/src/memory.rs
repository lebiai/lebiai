//! `memory_search` — search episodic memories by relevance.

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use hermes_memory::MemoryStore;
use serde::Deserialize;

#[derive(Deserialize)]
struct Args {
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
    let a: Args = serde_json::from_value(args)
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
