//! `edit` — exact string replacement in a file.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;

#[derive(Deserialize)]
struct Args {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "edit".into(),
        description: "Replace an exact string in a file. old_string must be unique unless replace_all is true.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path relative to workspace"},
                "old_string": {"type": "string", "description": "Exact text to find"},
                "new_string": {"type": "string", "description": "Replacement text"},
                "replace_all": {"type": "boolean", "description": "Replace all occurrences (default false)"}
            },
            "required": ["path", "old_string", "new_string"]
        }),
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("edit: bad args: {e}")))?;
    let path = safety::resolve(workspace, &a.path)?;
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("edit read {}: {e}", path.display())))?;

    let count = content.matches(&a.old_string).count();
    if count == 0 {
        return Ok(ToolCallOutcome {
            content: format!("edit failed: old_string not found in {}", a.path),
            is_error: true,
        });
    }
    if !a.replace_all && count > 1 {
        return Ok(ToolCallOutcome {
            content: format!(
                "edit failed: old_string found {} times in {} (must be unique or set replace_all=true)",
                count, a.path
            ),
            is_error: true,
        });
    }

    let new_content = if a.replace_all {
        content.replace(&a.old_string, &a.new_string)
    } else {
        content.replacen(&a.old_string, &a.new_string, 1)
    };

    tokio::fs::write(&path, &new_content)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("edit write {}: {e}", path.display())))?;

    Ok(ToolCallOutcome {
        content: format!("edited {}: {} replacement(s)", a.path, if a.replace_all { count } else { 1 }),
        is_error: false,
    })
}
