//! `write` — create or overwrite a file.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;

#[derive(Deserialize)]
struct Args {
    path: String,
    content: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "write".into(),
        description: "Create a new file or completely overwrite an existing file. \
            Parent directories are created automatically. WARNING: this replaces \
            the entire file — use `edit` instead to modify parts of an existing file. \
            For large content (>150 lines), write a skeleton first, then use `edit` \
            to fill in sections incrementally.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path relative to workspace"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path", "content"]
        }),
        requires_confirmation: true,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("write: bad args: {e}")))?;

    // For new files, resolve checks the parent chain.
    let path = safety::resolve(workspace, &a.path).or_else(|_| {
        // Path doesn't exist yet — construct it and verify parent is inside ws.
        let candidate = if Path::new(&a.path).is_absolute() {
            std::path::PathBuf::from(&a.path)
        } else {
            workspace.join(&a.path)
        };
        safety::resolve(workspace, &candidate.to_string_lossy())
            .or_else(|_| {
                // Last resort: just join and check prefix manually.
                let ws_canon = std::fs::canonicalize(workspace)
                    .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
                let norm = crate::safety::normalize_join(workspace, &a.path);
                if !norm.starts_with(&ws_canon) && !norm.starts_with(workspace) {
                    return Err(hermes_core::Error::ToolHost(format!(
                        "write: path escapes workspace: {}",
                        a.path
                    )));
                }
                Ok(norm)
            })
    })?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| hermes_core::Error::ToolHost(format!("write mkdir: {e}")))?;
    }
    tokio::fs::write(&path, &a.content)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("write {}: {e}", path.display())))?;

    Ok(ToolCallOutcome {
        content: format!("wrote {} bytes to {}", a.content.len(), path.display()),
        is_error: false,
    })
}
