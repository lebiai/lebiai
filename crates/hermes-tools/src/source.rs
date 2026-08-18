//! Read-only tools for kept work materials. Not a skill.

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use hermes_sources::SourceStore;
use serde::Deserialize;

pub fn list_spec() -> ToolSpec {
    ToolSpec {
        name: "source_list".into(),
        description: "List work files the user kept (我的材料). Titles only. \
Use when they ask what materials you have on hand. Do not invent files."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        requires_confirmation: false,
    }
}

pub fn read_spec() -> ToolSpec {
    ToolSpec {
        name: "source_read".into(),
        description: "Read the text of one kept material by id or title. \
Use after excerpts in this turn are not enough (adjacent clause, full section). \
If the file cannot be read, say so — do not invent clauses."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "src_… id from excerpts or source_list"},
                "title": {"type": "string", "description": "Exact title if id unknown"}
            }
        }),
        requires_confirmation: false,
    }
}

pub fn handles(name: &str) -> bool {
    matches!(name, "source_list" | "source_read")
}

pub async fn list_run(store: &SourceStore) -> Result<ToolCallOutcome> {
    let rows = store.list_active();
    let allow: Vec<String> = rows
        .iter()
        .flat_map(|s| [s.id.clone(), s.title.clone()])
        .collect();
    store.set_read_allowlist(Some(allow));
    if rows.is_empty() {
        return Ok(ToolCallOutcome {
            content: "no kept materials".into(),
            is_error: false,
        });
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|s| {
            let flag = if s.readable { "" } else { " (text unread)" };
            format!("- [{}] 《{}》{flag}", s.id, s.title)
        })
        .collect();
    Ok(ToolCallOutcome {
        content: lines.join("\n"),
        is_error: false,
    })
}

#[derive(Deserialize)]
struct ReadArgs {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

pub async fn read_run(store: &SourceStore, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: ReadArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("source_read: bad args: {e}")))?;
    let key =
        a.id.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or(a.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
            .unwrap_or("");
    if key.is_empty() {
        return Ok(ToolCallOutcome {
            content: "source_read: need id or title".into(),
            is_error: true,
        });
    }
    match store.read_text(key) {
        None => Ok(ToolCallOutcome {
            content: format!("no kept material matching {key}"),
            is_error: false,
        }),
        Some((meta, text)) if text.trim().is_empty() => Ok(ToolCallOutcome {
            content: format!(
                "《{}》 is kept but the text could not be read. Open the original.",
                meta.title
            ),
            is_error: false,
        }),
        Some((meta, text)) => {
            let clipped: String = text.chars().take(12_000).collect();
            Ok(ToolCallOutcome {
                content: format!("[{}|《{}》]\n{clipped}", meta.id, meta.title),
                is_error: false,
            })
        }
    }
}
