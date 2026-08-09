//! Human-friendly one-line renderings of tool calls for the terminal.
//!
//! These map a tool name + its JSON input into a compact, emoji-prefixed
//! line such as `📝 write src/main.rs (812 bytes)` or `💻 $ cargo build`.
//! Shared by the streaming render paths in `chat`, `ask`, and the autonomous
//! `run` agent so tool activity looks identical everywhere.

use std::path::Path;

/// Short label for a tool *before* its input is known (the
/// `ToolUseStart` phase, shown as a transient `… ` placeholder).
pub fn friendly_tool_desc(name: &str) -> String {
    match name {
        "read" => "📖 Reading file…".into(),
        "write" => "📝 Writing file…".into(),
        "edit" => "✏️  Editing file…".into(),
        "bash" => "💻 Running command…".into(),
        "glob" => "🔍 Searching files…".into(),
        "grep" => "🔎 Searching content…".into(),
        "web_fetch" => "🌐 Fetching web page…".into(),
        "web_search" => "🔍 Searching the web…".into(),
        "todo_write" => "📋 Updating plan…".into(),
        "todo_list" => "📋 Listing tasks…".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}…")
        }
    }
}

/// Full one-line rendering of a tool call once its input is known (the
/// `ToolExecStart` phase). `workspace` is used to expand relative paths so
/// the line shows an unambiguous location.
pub fn friendly_tool_result(name: &str, input: &serde_json::Value, workspace: &Path) -> String {
    let full_path = |rel: &str| -> String {
        if Path::new(rel).is_absolute() {
            rel.to_string()
        } else {
            workspace.join(rel).to_string_lossy().to_string()
        }
    };

    match name {
        "read" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let offset = input.get("offset").and_then(|o| o.as_u64());
            let limit = input.get("limit").and_then(|l| l.as_u64());
            let mut desc = format!("📖 read {}", full_path(path));
            if let Some(o) = offset {
                desc.push_str(&format!(" (from line {o}"));
                if let Some(l) = limit {
                    desc.push_str(&format!(", {l} lines"));
                }
                desc.push(')');
            }
            desc
        }
        "write" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let len = input
                .get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.len())
                .unwrap_or(0);
            format!("📝 write {} ({len} bytes)", full_path(path))
        }
        "edit" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("?");
            let old: String = input
                .get("old_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            let new: String = input
                .get("new_string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect();
            format!("✏️  edit {}: \"{old}\" → \"{new}\"", full_path(path))
        }
        "bash" => {
            let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("?");
            let short: String = cmd.chars().take(120).collect();
            if cmd.chars().count() > 120 {
                format!("💻 $ {short}…")
            } else {
                format!("💻 $ {short}")
            }
        }
        "glob" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            format!("🔍 glob {}", full_path(pat))
        }
        "grep" => {
            let pat = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("?");
            let path = input.get("path").and_then(|p| p.as_str());
            match path {
                Some(p) => format!("🔎 grep /{pat}/ in {}", full_path(p)),
                None => format!("🔎 grep /{pat}/ in {}", workspace.display()),
            }
        }
        "web_search" => {
            let q = input.get("query").and_then(|q| q.as_str()).unwrap_or("?");
            format!("🌐 search \"{q}\"")
        }
        "web_fetch" => {
            let url = input.get("url").and_then(|u| u.as_str()).unwrap_or("?");
            format!("🌐 fetch {url}")
        }
        "think" => "💭 thinking…".into(),
        "todo_write" => {
            let items = input.get("items").and_then(|i| i.as_array());
            let n = items.map(|a| a.len()).unwrap_or(0);
            let active = items
                .and_then(|a| {
                    a.iter()
                        .find(|it| it.get("status").and_then(|s| s.as_str()) == Some("in_progress"))
                })
                .and_then(|it| it.get("content").and_then(|c| c.as_str()));
            match active {
                Some(c) => format!("📋 plan ({n}) → {c}"),
                None => format!("📋 plan ({n} tasks)"),
            }
        }
        "todo_list" => "📋 listing todos".into(),
        other => {
            let display = other.split_once("__").map(|(_, t)| t).unwrap_or(other);
            format!("🔧 {display}({})", summarise_input(input))
        }
    }
}

fn summarise_input(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() <= 80 {
        s
    } else {
        let truncated: String = s.chars().take(80).collect();
        format!("{truncated}…")
    }
}
