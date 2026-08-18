//! Engine tools for 在办 (open work). Not a skill.

use chrono::NaiveDate;
use hermes_commitments::{
    parse_due, Commitment, CommitmentStore, SaveMode, SaveOutcome, Source, Status, OPEN_CROWD,
};
use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

pub fn list_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_list".into(),
        description: "List open work (在办): debts still owed after a conversation. \
            Returns titles and ids. Use before creating a new row. \
            Steps of the current turn belong in todo_write, not here."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        requires_confirmation: false,
    }
}

pub fn save_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_save".into(),
        description: "Capture or fold a piece of open work (在办) — something they \
            will still owe after this conversation. Prefer fewer items. \
            A due day is required (今天 / 这周 / 下周 / 周五 / YYYY-MM-DD). \
            If they did not name a day, ask 哪天前 — do not save 尽快 or invent a date. \
            If a live row is the same debt, pass mergeInto with that id. \
            Title must use their verb + object."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "User's wording: verb + object"},
                "doneWhen": {"type": "string", "description": "What done looks like"},
                "softDue": {"type": "string", "description": "Required due phrase: 今天 / 这周 / 下周 / 周五 / 8/20"},
                "softDueDate": {"type": "string", "description": "YYYY-MM-DD if already known"},
                "note": {"type": "string"},
                "sessionId": {"type": "string"},
                "mergeInto": {"type": "string", "description": "Existing id to fold into"},
                "forceNew": {"type": "boolean", "description": "Create even if similar"}
            },
            "required": ["title"]
        }),
        requires_confirmation: false,
    }
}

pub fn close_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_close".into(),
        description: "Mark a piece of open work done. It leaves the open list. \
            Do not turn the instance into a memory unless a reusable method remains."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            },
            "required": ["id"]
        }),
        requires_confirmation: false,
    }
}

pub fn split_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_split".into(),
        description: "Split one open-work row into two or more independent deliverables. \
            First title stays on the original id. Only use when there are truly separate done-pictures."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "titles": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["id", "titles"]
        }),
        requires_confirmation: false,
    }
}

pub fn update_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_update".into(),
        description: "Update an open-work row: retitle, doneWhen, softDue, waiting note, or reopen."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "title": {"type": "string"},
                "doneWhen": {"type": "string"},
                "softDue": {"type": "string"},
                "note": {"type": "string"},
                "waiting": {"type": "boolean"}
            },
            "required": ["id"]
        }),
        requires_confirmation: false,
    }
}

pub fn drop_spec() -> ToolSpec {
    ToolSpec {
        name: "commitment_drop".into(),
        description: "Drop a piece of open work they decided not to do."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            },
            "required": ["id"]
        }),
        requires_confirmation: true,
    }
}

pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "commitment_list"
            | "commitment_save"
            | "commitment_close"
            | "commitment_drop"
            | "commitment_split"
            | "commitment_update"
    )
}

pub async fn run(
    store: &CommitmentStore,
    name: &str,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    match name {
        "commitment_list" => list_run(store).await,
        "commitment_save" => save_run(store, args).await,
        "commitment_close" => close_run(store, args, Status::Done).await,
        "commitment_drop" => close_run(store, args, Status::Dropped).await,
        "commitment_split" => split_run(store, args).await,
        "commitment_update" => update_run(store, args).await,
        _ => Err(hermes_core::Error::ToolHost(format!(
            "unknown commitment tool: {name}"
        ))),
    }
}

async fn list_run(store: &CommitmentStore) -> Result<ToolCallOutcome> {
    let live = store
        .list_live()
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    if live.is_empty() {
        return Ok(ToolCallOutcome {
            content: "(no open work)".into(),
            is_error: false,
        });
    }
    let owed: Vec<_> = live.iter().filter(|c| c.status.is_owed()).collect();
    let suggested: Vec<_> = live
        .iter()
        .filter(|c| c.status == Status::Suggested)
        .collect();
    let mut buf = String::new();
    if !suggested.is_empty() {
        buf.push_str("Suggested (not yet accepted):\n");
        for c in &suggested {
            buf.push_str(&format!("- [suggested {}] {}\n", c.id, c.title));
        }
    }
    if owed.is_empty() {
        buf.push_str("Open: (none)\n");
    } else {
        buf.push_str("Open:\n");
        for c in &owed {
            let wait = if c.status == Status::Waiting {
                " waiting"
            } else {
                ""
            };
            let due = c
                .soft_due
                .as_deref()
                .map(|d| format!(" due={d}"))
                .unwrap_or_default();
            buf.push_str(&format!("- [{}{wait}{due}] {}\n", c.id, c.title));
        }
    }
    if owed.len() >= OPEN_CROWD {
        buf.push_str("\n(crowded — prefer cutting before adding)\n");
    }
    Ok(ToolCallOutcome {
        content: buf,
        is_error: false,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    title: String,
    #[serde(default)]
    done_when: Option<String>,
    #[serde(default)]
    soft_due: Option<String>,
    #[serde(default)]
    soft_due_date: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    merge_into: Option<String>,
    #[serde(default)]
    force_new: bool,
}

async fn save_run(store: &CommitmentStore, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: SaveArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("commitment_save: {e}")))?;
    if let Some(id) = a.merge_into.as_deref() {
        let mut incoming = Commitment::new(&a.title, Source::Dialogue)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
        fill(&mut incoming, &a);
        match store
            .fold_into(id, &incoming)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?
        {
            SaveOutcome::Folded { into } => {
                return Ok(ToolCallOutcome {
                    content: format!("Folded into existing open work [{}]: {}", into.id, into.title),
                    is_error: false,
                });
            }
            _ => {
                return Ok(ToolCallOutcome {
                    content: "mergeInto failed".into(),
                    is_error: true,
                });
            }
        }
    }
    let mut item = Commitment::new(&a.title, Source::Dialogue)
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    fill(&mut item, &a);
    if item.soft_due_date.is_none() {
        return Ok(ToolCallOutcome {
            content: "Need a due day (哪天前). Ask 今天 / 这周 / 下周 / a date. \
                 Do not save 尽快. Then call again with softDue."
                .into(),
            is_error: false,
        });
    }
    let mode = if a.force_new {
        SaveMode::ForceNew
    } else {
        SaveMode::Ask
    };
    match store
        .save(item, mode)
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?
    {
        SaveOutcome::Created(c) => Ok(ToolCallOutcome {
            content: format!("Recorded open work [{}]: {}", c.id, c.title),
            is_error: false,
        }),
        SaveOutcome::Folded { into } => Ok(ToolCallOutcome {
            content: format!("Folded into [{}]: {}", into.id, into.title),
            is_error: false,
        }),
        SaveOutcome::Near { existing, score } => Ok(ToolCallOutcome {
            content: format!(
                "Near existing open work [{}] 「{}」 (score {score:.2}). \
                 Same debt? Call again with mergeInto=\"{}\" to fold, or forceNew=true if it is a new deliverable.",
                existing.id, existing.title, existing.id
            ),
            is_error: false,
        }),
    }
}

fn fill(item: &mut Commitment, a: &SaveArgs) {
    item.done_when = empty_to_none(&a.done_when);
    let today = chrono::Local::now().date_naive();
    if let Some(iso) = a
        .soft_due_date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
    {
        item.soft_due_date = Some(iso);
        item.soft_due = empty_to_none(&a.soft_due).or_else(|| Some(iso.to_string()));
    } else if let Some(phrase) = empty_to_none(&a.soft_due) {
        if let Ok((kept, date)) = parse_due(&phrase, today) {
            item.soft_due = Some(kept);
            item.soft_due_date = Some(date);
        }
    }
    item.note = empty_to_none(&a.note);
    item.session_id = empty_to_none(&a.session_id);
}

fn empty_to_none(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

async fn close_run(
    store: &CommitmentStore,
    args: serde_json::Value,
    status: Status,
) -> Result<ToolCallOutcome> {
    let a: IdArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("commitment close: {e}")))?;
    let c = store
        .close(&a.id, status)
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    let verb = match status {
        Status::Done => "Closed",
        _ => "Dropped",
    };
    Ok(ToolCallOutcome {
        content: format!("{verb} [{}]: {}", c.id, c.title),
        is_error: false,
    })
}

#[derive(Deserialize)]
struct SplitArgs {
    id: String,
    titles: Vec<String>,
}

async fn split_run(store: &CommitmentStore, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: SplitArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("commitment_split: {e}")))?;
    let out = store
        .split(&a.id, &a.titles)
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    let lines: Vec<String> = out
        .iter()
        .map(|c| format!("- [{}] {}", c.id, c.title))
        .collect();
    Ok(ToolCallOutcome {
        content: format!("Split into {}:\n{}", out.len(), lines.join("\n")),
        is_error: false,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArgs {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    done_when: Option<String>,
    #[serde(default)]
    soft_due: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    waiting: Option<bool>,
}

async fn update_run(store: &CommitmentStore, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: UpdateArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("commitment_update: {e}")))?;
    if let Some(title) = a.title.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        store
            .retitle(&a.id, title.to_string())
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    }
    if a.done_when.is_some() {
        store
            .set_done_when(&a.id, a.done_when)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    }
    if let Some(true) = a.waiting {
        store
            .set_waiting(&a.id, a.note.clone())
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    } else if let Some(false) = a.waiting {
        store
            .reopen(&a.id)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
        if a.note.is_some() {
            store
                .patch_note(&a.id, a.note)
                .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
        }
    } else if a.note.is_some() {
        store
            .patch_note(&a.id, a.note)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    }
    if let Some(due) = a.soft_due {
        store
            .patch_soft_due(&a.id, due)
            .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?;
    }
    let c = store
        .get(&a.id)
        .map_err(|e| hermes_core::Error::ToolHost(e.to_string()))?
        .ok_or_else(|| hermes_core::Error::ToolHost("not found".into()))?;
    Ok(ToolCallOutcome {
        content: format!("Updated [{}]: {}", c.id, c.title),
        is_error: false,
    })
}
