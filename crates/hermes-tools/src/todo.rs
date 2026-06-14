//! `todo` — in-session task management for the agent.
//!
//! The agent breaks a complex task into steps, then keeps the list current
//! with a single `todo_write` call that **replaces the whole list** — Claude
//! Code's TodoWrite model. There are no fragile per-item ids to drift out of
//! sync; each write is the complete desired state.
//!
//! State is per-session: it lives in a [`TodoStore`] owned by the tool host,
//! not a process global, so todos never leak across sessions or GUI windows.
//! The list is also mirrored to `<workspace>/TODOS.md` so the user can watch
//! progress.

use std::path::Path;
use std::sync::{Arc, Mutex};

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

#[derive(Debug, Clone)]
struct Todo {
    content: String,
    status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Pending,
    InProgress,
    Completed,
}

/// Per-session todo list. Cheap to clone (shared `Arc`). One per tool host,
/// so each session/window has its own list.
#[derive(Default, Clone)]
pub struct TodoStore(Arc<Mutex<Vec<Todo>>>);

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "todo_write".into(),
            description: "Write the full todo list for the current task, replacing the previous \
                list entirely. Use this for any multi-step task (~3+ steps): lay out the plan up \
                front, then call again to update status as you go. Keep exactly ONE item \
                in_progress at a time, and mark an item completed the moment it is done — do not \
                batch completions. Each item has `content` (imperative, e.g. \"Add cache \
                breakpoints\"), `status` (pending/in_progress/completed), and optional \
                `activeForm` (present-continuous, e.g. \"Adding cache breakpoints\")."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "The complete todo list; replaces any prior list.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {"type": "string", "description": "Imperative task description"},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
                                "activeForm": {"type": "string", "description": "Present-continuous form shown while in progress"}
                            },
                            "required": ["content", "status"]
                        }
                    }
                },
                "required": ["items"]
            }),
            requires_confirmation: false,
        },
        ToolSpec {
            name: "todo_list".into(),
            description: "Show the current todo list with each item's status.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            requires_confirmation: false,
        },
    ]
}

pub fn handles(name: &str) -> bool {
    matches!(name, "todo_write" | "todo_list")
}

pub async fn run(
    store: &TodoStore,
    workspace: &Path,
    name: &str,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    match name {
        "todo_write" => write(store, workspace, args).await,
        "todo_list" => list(store).await,
        _ => Err(hermes_core::Error::ToolHost(format!(
            "unknown todo tool: {name}"
        ))),
    }
}

#[derive(Deserialize)]
struct WriteArgs {
    items: Vec<ItemArg>,
}

#[derive(Deserialize)]
struct ItemArg {
    content: String,
    status: String,
    // `activeForm` is accepted (Claude Code parity) but not displayed in the
    // compact list; kept optional so omitting it is fine.
    #[serde(default, rename = "activeForm", alias = "active_form")]
    #[allow(dead_code)]
    active_form: Option<String>,
}

async fn write(
    store: &TodoStore,
    workspace: &Path,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: WriteArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("todo_write: bad args: {e}")))?;

    let mut new_todos = Vec::with_capacity(a.items.len());
    for it in a.items {
        let status = match it.status.as_str() {
            "pending" => Status::Pending,
            "in_progress" => Status::InProgress,
            // accept "done" as a friendly alias for the old vocabulary
            "completed" | "done" => Status::Completed,
            other => {
                return Ok(ToolCallOutcome {
                    content: format!(
                        "invalid status {other:?} (use pending/in_progress/completed)"
                    ),
                    is_error: true,
                })
            }
        };
        new_todos.push(Todo {
            content: it.content,
            status,
        });
    }

    let rendered = {
        let mut guard = store.0.lock().unwrap();
        *guard = new_todos;
        render(&guard)
    };
    persist(workspace, &rendered);
    Ok(ToolCallOutcome {
        content: format!("Updated todo list:\n{rendered}"),
        is_error: false,
    })
}

async fn list(store: &TodoStore) -> Result<ToolCallOutcome> {
    let rendered = render(&store.0.lock().unwrap());
    Ok(ToolCallOutcome {
        content: rendered,
        is_error: false,
    })
}

/// Render the list as checkbox lines, reused for both the tool result and the
/// `TODOS.md` mirror.
fn render(todos: &[Todo]) -> String {
    if todos.is_empty() {
        return "(no todos)".to_string();
    }
    todos
        .iter()
        .map(|t| {
            let mark = match t.status {
                Status::Pending => "[ ]",
                Status::InProgress => "[~]",
                Status::Completed => "[x]",
            };
            format!("- {mark} {}", t.content)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn persist(workspace: &Path, rendered: &str) {
    let md = format!("# TODOs\n\n{rendered}\n");
    let _ = std::fs::write(workspace.join("TODOS.md"), md);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_replaces_whole_list() {
        let store = TodoStore::default();
        let ws = std::env::temp_dir();

        run(
            &store,
            &ws,
            "todo_write",
            serde_json::json!({"items": [
                {"content": "a", "status": "completed"},
                {"content": "b", "status": "in_progress"},
                {"content": "c", "status": "pending"}
            ]}),
        )
        .await
        .unwrap();
        assert_eq!(store.0.lock().unwrap().len(), 3);

        // A second write fully replaces — not appends.
        run(
            &store,
            &ws,
            "todo_write",
            serde_json::json!({"items": [{"content": "only", "status": "pending"}]}),
        )
        .await
        .unwrap();
        let todos = store.0.lock().unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "only");
    }

    #[tokio::test]
    async fn stores_are_isolated_per_session() {
        let ws = std::env::temp_dir();
        let a = TodoStore::default();
        let b = TodoStore::default();
        run(
            &a,
            &ws,
            "todo_write",
            serde_json::json!({"items": [{"content": "x", "status": "pending"}]}),
        )
        .await
        .unwrap();
        // b is a separate session: must not see a's todos.
        assert_eq!(a.0.lock().unwrap().len(), 1);
        assert_eq!(b.0.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn invalid_status_is_a_tool_error_not_a_panic() {
        let store = TodoStore::default();
        let ws = std::env::temp_dir();
        let out = run(
            &store,
            &ws,
            "todo_write",
            serde_json::json!({"items": [{"content": "x", "status": "bogus"}]}),
        )
        .await
        .unwrap();
        assert!(out.is_error);
        // The bad write left the list untouched.
        assert_eq!(store.0.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn accepts_active_form_and_done_alias() {
        let store = TodoStore::default();
        let ws = std::env::temp_dir();
        let out = run(
            &store,
            &ws,
            "todo_write",
            serde_json::json!({"items": [
                {"content": "ship", "status": "done", "activeForm": "Shipping"}
            ]}),
        )
        .await
        .unwrap();
        assert!(!out.is_error);
        assert_eq!(store.0.lock().unwrap()[0].status, Status::Completed);
    }
}
