//! `glob` — find files by pattern inside the workspace.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

#[derive(Deserialize)]
struct Args {
    pattern: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "glob".into(),
        description:
            "Find files matching a glob pattern inside the workspace (e.g. **/*.rs, src/**/*.ts)."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern relative to workspace"}
            },
            "required": ["pattern"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("glob: bad args: {e}")))?;

    let full_pattern = workspace.join(&a.pattern).to_string_lossy().to_string();
    let entries: Vec<String> = glob::glob(&full_pattern)
        .map_err(|e| hermes_core::Error::ToolHost(format!("glob pattern error: {e}")))?
        .filter_map(|r| r.ok())
        .filter_map(|p| {
            p.strip_prefix(workspace)
                .ok()
                .map(|rel| rel.to_string_lossy().to_string())
        })
        .collect();

    if entries.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no files matching pattern: {}", a.pattern),
            is_error: false,
        });
    }
    let out = format!("{} file(s):\n{}", entries.len(), entries.join("\n"));
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}
