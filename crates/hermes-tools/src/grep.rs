//! `grep` — search file contents with regex inside the workspace.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use regex::Regex;
use serde::Deserialize;
use walkdir::WalkDir;

#[derive(Deserialize)]
struct Args {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    include: Option<String>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "grep".into(),
        description: "Search file contents with a regex pattern. Returns file:line:match. Searches workspace by default.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Regex pattern to search for"},
                "path": {"type": "string", "description": "Subdirectory to search (default: entire workspace)"},
                "include": {"type": "string", "description": "Glob filter for filenames (e.g. *.rs)"}
            },
            "required": ["pattern"]
        }),
        requires_confirmation: false,
    }
}

const MAX_MATCHES: usize = 200;

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("grep: bad args: {e}")))?;

    let re = Regex::new(&a.pattern)
        .map_err(|e| hermes_core::Error::ToolHost(format!("grep: invalid regex: {e}")))?;

    let search_root = match &a.path {
        Some(p) => crate::safety::resolve(workspace, p)?,
        None => workspace.to_path_buf(),
    };

    let include_glob = a
        .include
        .as_deref()
        .and_then(|g| glob::Pattern::new(g).ok());

    let mut matches: Vec<String> = Vec::new();
    for entry in WalkDir::new(&search_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(ref pat) = include_glob {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !pat.matches(name) {
                continue;
            }
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = path
            .strip_prefix(workspace)
            .unwrap_or(path)
            .to_string_lossy();
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                matches.push(format!("{}:{}:{}", rel, i + 1, line.trim()));
                if matches.len() >= MAX_MATCHES {
                    matches.push(format!("... (truncated at {MAX_MATCHES} matches)"));
                    let out = matches.join("\n");
                    return Ok(ToolCallOutcome {
                        content: out,
                        is_error: false,
                    });
                }
            }
        }
    }

    if matches.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no matches for pattern: {}", a.pattern),
            is_error: false,
        });
    }
    let out = format!("{} match(es):\n{}", matches.len(), matches.join("\n"));
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}
