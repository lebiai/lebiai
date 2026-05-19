//! `read` — read a file with line numbers.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;

#[derive(Deserialize)]
struct Args {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "read".into(),
        description: "Read a file and return its contents with line numbers. \
            Use `offset` and `limit` to read specific sections of large files. \
            Always read a file before editing it to understand its structure and \
            find the exact text to replace.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path relative to workspace"},
                "offset": {"type": "integer", "description": "Start line (0-based, default 0)"},
                "limit": {"type": "integer", "description": "Max lines to return (default 2000)"}
            },
            "required": ["path"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("read: bad args: {e}")))?;
    let path = safety::resolve(workspace, &a.path)?;
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("read {}: {e}", path.display())))?;

    let offset = a.offset.unwrap_or(0);
    let limit = a.limit.unwrap_or(2000);
    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("{:>5}\t{}\n", offset + i + 1, line));
    }
    let total = content.lines().count();
    if offset + limit < total {
        out.push_str(&format!("... ({} more lines)\n", total - offset - limit));
    }
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}
