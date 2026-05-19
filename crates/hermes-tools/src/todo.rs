//! `todo` — in-session task management for the agent.
//!
//! The agent can break complex tasks into steps, track progress, and
//! report what's done vs pending. Todos live in memory for the session
//! duration and are also persisted to `<workspace>/TODOS.md` so the user
//! can inspect them.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone)]
struct Todo {
    id: u32,
    title: String,
    status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Pending,
    InProgress,
    Done,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Pending => write!(f, "pending"),
            Status::InProgress => write!(f, "in_progress"),
            Status::Done => write!(f, "done"),
        }
    }
}

use std::sync::Mutex;
use once_cell::sync::Lazy;

static TODOS: Lazy<Mutex<Vec<Todo>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "todo_add".into(),
            description: "Add a new todo item. Returns the assigned ID.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Task description"}
                },
                "required": ["title"]
            }),
            requires_confirmation: false,
        },
        ToolSpec {
            name: "todo_update".into(),
            description: "Update a todo's status to pending, in_progress, or done.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Todo ID"},
                    "status": {"type": "string", "enum": ["pending", "in_progress", "done"]}
                },
                "required": ["id", "status"]
            }),
            requires_confirmation: false,
        },
        ToolSpec {
            name: "todo_list".into(),
            description: "List all todos with their status.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            requires_confirmation: false,
        },
    ]
}

pub fn handles(name: &str) -> bool {
    matches!(name, "todo_add" | "todo_update" | "todo_list")
}

pub async fn run(workspace: &Path, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
    match name {
        "todo_add" => add(workspace, args).await,
        "todo_update" => update(workspace, args).await,
        "todo_list" => list(workspace).await,
        _ => Err(hermes_core::Error::ToolHost(format!("unknown todo tool: {name}"))),
    }
}

#[derive(Deserialize)]
struct AddArgs {
    title: String,
}

async fn add(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: AddArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("todo_add: {e}")))?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let todo = Todo {
        id,
        title: a.title.clone(),
        status: Status::Pending,
    };
    TODOS.lock().unwrap().push(todo);
    persist(workspace);
    Ok(ToolCallOutcome {
        content: format!("added todo #{id}: {}", a.title),
        is_error: false,
    })
}

#[derive(Deserialize)]
struct UpdateArgs {
    id: u32,
    status: String,
}

async fn update(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: UpdateArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("todo_update: {e}")))?;
    let new_status = match a.status.as_str() {
        "pending" => Status::Pending,
        "in_progress" => Status::InProgress,
        "done" => Status::Done,
        other => {
            return Ok(ToolCallOutcome {
                content: format!("invalid status: {other} (use pending/in_progress/done)"),
                is_error: true,
            })
        }
    };
    let mut todos = TODOS.lock().unwrap();
    if let Some(t) = todos.iter_mut().find(|t| t.id == a.id) {
        t.status = new_status;
        persist(workspace);
        Ok(ToolCallOutcome {
            content: format!("todo #{} → {}", a.id, new_status),
            is_error: false,
        })
    } else {
        Ok(ToolCallOutcome {
            content: format!("todo #{} not found", a.id),
            is_error: true,
        })
    }
}

async fn list(_workspace: &Path) -> Result<ToolCallOutcome> {
    let todos = TODOS.lock().unwrap();
    if todos.is_empty() {
        return Ok(ToolCallOutcome {
            content: "(no todos)".into(),
            is_error: false,
        });
    }
    let lines: Vec<String> = todos
        .iter()
        .map(|t| {
            let mark = match t.status {
                Status::Pending => "[ ]",
                Status::InProgress => "[~]",
                Status::Done => "[x]",
            };
            format!("{mark} #{}: {}", t.id, t.title)
        })
        .collect();
    Ok(ToolCallOutcome {
        content: lines.join("\n"),
        is_error: false,
    })
}

fn persist(workspace: &Path) {
    let todos = TODOS.lock().unwrap();
    let mut md = String::from("# TODOs\n\n");
    for t in todos.iter() {
        let mark = match t.status {
            Status::Pending => "- [ ]",
            Status::InProgress => "- [~]",
            Status::Done => "- [x]",
        };
        md.push_str(&format!("{mark} #{}: {}\n", t.id, t.title));
    }
    let _ = std::fs::write(workspace.join("TODOS.md"), md);
}
