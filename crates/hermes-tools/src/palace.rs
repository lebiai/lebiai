//! Palace tools: zone navigation for the Memory Palace.

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use hermes_memory::{get_zone, group_by_zone, MemoryEvent, MemoryStatEntry, MemoryStore};
use serde::Deserialize;

// --- palace_zones ---

pub fn zones_spec() -> ToolSpec {
    ToolSpec {
        name: "palace_zones".into(),
        description: "List all Memory Palace zones with memory counts. Use this to discover what zones exist before reading details.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        requires_confirmation: false,
    }
}

pub async fn zones_run(store: &dyn MemoryStore) -> Result<ToolCallOutcome> {
    let active = store
        .list_active()
        .map_err(|e| hermes_core::Error::ToolHost(format!("palace_zones: {e}")))?;
    let zones = group_by_zone(&active);
    if zones.is_empty() {
        return Ok(ToolCallOutcome {
            content: "Memory Palace is empty — no memories yet.".into(),
            is_error: false,
        });
    }
    let mut buf = String::new();
    for (zone, mems) in &zones {
        buf.push_str(&format!("- {} ({} memories)\n", zone, mems.len()));
    }
    Ok(ToolCallOutcome {
        content: buf,
        is_error: false,
    })
}

// --- palace_read_zone ---

#[derive(Deserialize)]
struct ReadZoneArgs {
    zone: String,
}

pub fn read_zone_spec() -> ToolSpec {
    ToolSpec {
        name: "palace_read_zone".into(),
        description: "Load all memories from a specific zone. Returns cached zone summary if available, otherwise raw memory bodies. Records access for forgetting/decay tracking.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "zone": {"type": "string", "description": "Zone name (e.g. 'core', 'work', 'project:hermes', 'episode', 'general')"}
            },
            "required": ["zone"]
        }),
        requires_confirmation: false,
    }
}

pub async fn read_zone_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: ReadZoneArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("palace_read_zone: bad args: {e}")))?;

    let active = store
        .list_active()
        .map_err(|e| hermes_core::Error::ToolHost(format!("palace_read_zone: {e}")))?;
    let zone_mems = get_zone(&active, &a.zone);

    if zone_mems.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("Zone '{}' is empty or does not exist.", a.zone),
            is_error: false,
        });
    }

    let now = chrono::Utc::now();
    for m in &zone_mems {
        hermes_memory::record_memory_stat(MemoryStatEntry {
            at: now,
            memory_id: m.frontmatter.id.clone(),
            event: MemoryEvent::Accessed,
        });
    }

    let mut buf = format!("[zone '{}' — {} memories]\n\n", a.zone, zone_mems.len());
    for m in &zone_mems {
        let tags = if m.frontmatter.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", m.frontmatter.tags.join(", "))
        };
        buf.push_str(&format!(
            "- ({} conf={:?}{tags}) {}\n",
            m.frontmatter.id,
            m.frontmatter.confidence,
            m.body.trim()
        ));
    }

    Ok(ToolCallOutcome {
        content: buf,
        is_error: false,
    })
}

// --- palace_recall ---

#[derive(Deserialize)]
struct RecallArgs {
    topic: String,
    #[serde(default = "default_recall_limit")]
    limit: usize,
    #[serde(default)]
    zone: Option<String>,
}

fn default_recall_limit() -> usize {
    5
}

pub fn recall_spec() -> ToolSpec {
    ToolSpec {
        name: "palace_recall".into(),
        description: "Search memories by topic, optionally scoped to a zone. More focused than memory_search — use this for palace navigation.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "What to recall (e.g. 'editor preference', 'error handling convention')"},
                "limit": {"type": "integer", "description": "Max results (default 5)"},
                "zone": {"type": "string", "description": "Optional: restrict to a specific zone"}
            },
            "required": ["topic"]
        }),
        requires_confirmation: false,
    }
}

pub async fn recall_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: RecallArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("palace_recall: bad args: {e}")))?;

    let hits = store
        .search(&a.topic, a.limit * 3)
        .map_err(|e| hermes_core::Error::ToolHost(format!("palace_recall: {e}")))?;

    let filtered: Vec<_> = if let Some(ref zone) = a.zone {
        hits.into_iter()
            .filter(|m| m.frontmatter.zone == *zone)
            .take(a.limit)
            .collect()
    } else {
        hits.into_iter().take(a.limit).collect()
    };

    if filtered.is_empty() {
        let scope = a
            .zone
            .as_deref()
            .map(|z| format!(" in zone '{z}'"))
            .unwrap_or_default();
        return Ok(ToolCallOutcome {
            content: format!("No memories matching '{}'{scope}", a.topic),
            is_error: false,
        });
    }

    let now = chrono::Utc::now();
    for m in &filtered {
        hermes_memory::record_memory_stat(MemoryStatEntry {
            at: now,
            memory_id: m.frontmatter.id.clone(),
            event: MemoryEvent::Accessed,
        });
    }

    let mut buf = String::new();
    for (i, m) in filtered.iter().enumerate() {
        let tags = if m.frontmatter.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", m.frontmatter.tags.join(", "))
        };
        buf.push_str(&format!(
            "{}. ({} zone={} conf={:?}{tags}) {}\n",
            i + 1,
            m.frontmatter.id,
            m.frontmatter.zone,
            m.frontmatter.confidence,
            m.body.trim()
        ));
    }

    Ok(ToolCallOutcome {
        content: buf,
        is_error: false,
    })
}
