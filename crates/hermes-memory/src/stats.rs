//! Memory effectiveness tracking: record when a memory is loaded into context
//! vs actually referenced in the assistant's response. Memories with low
//! referenced/loaded ratio get deprioritized in relevance ranking.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatEntry {
    pub at: chrono::DateTime<chrono::Utc>,
    pub memory_id: String,
    pub event: MemoryEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEvent {
    Loaded,
    Referenced,
    Accessed,
}

fn default_path() -> Result<PathBuf> {
    Ok(hermes_core::data_path("memory-stats.jsonl"))
}

/// Append a stat entry. Best-effort write.
pub fn record(entry: MemoryStatEntry) {
    if let Err(e) = try_record(entry) {
        tracing::warn!(error=%e, "failed to record memory stat");
    }
}

fn try_record(entry: MemoryStatEntry) -> Result<()> {
    let path = default_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&entry)?;
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Per-memory effectiveness summary.
#[derive(Debug, Clone, Default)]
pub struct MemoryEffectiveness {
    pub loaded: usize,
    pub referenced: usize,
    pub accessed: usize,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl MemoryEffectiveness {
    /// Effectiveness factor in [0.5, 1.0]. Memories with no data default to 1.0.
    /// Low referenced/loaded ratio pulls the factor down toward 0.5.
    pub fn factor(&self) -> f64 {
        if self.loaded == 0 {
            return 1.0;
        }
        let ratio = self.referenced as f64 / self.loaded as f64;
        0.5 + 0.5 * ratio
    }

    /// Decay factor based on time since last access. 30-day half-life, floor 0.3.
    pub fn decay_factor(&self, now: chrono::DateTime<chrono::Utc>) -> f64 {
        let Some(last) = self.last_event_at else {
            return 1.0;
        };
        let days = (now - last).num_days().max(0) as f64;
        (0.5_f64.powf(days / 30.0)).max(0.3)
    }
}

/// Load effectiveness stats for all memories.
pub fn load_effectiveness() -> Result<HashMap<String, MemoryEffectiveness>> {
    let path = default_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let f = std::fs::File::open(&path)?;
    let reader = BufReader::new(f);
    let mut map: HashMap<String, MemoryEffectiveness> = HashMap::new();
    for line in reader.lines().map_while(|r| r.ok()) {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<MemoryStatEntry>(&line) {
            Ok(e) => {
                let entry = map.entry(e.memory_id).or_default();
                match e.event {
                    MemoryEvent::Loaded => entry.loaded += 1,
                    MemoryEvent::Referenced => entry.referenced += 1,
                    MemoryEvent::Accessed => entry.accessed += 1,
                }
                let ts = Some(e.at);
                if ts > entry.last_event_at {
                    entry.last_event_at = ts;
                }
            }
            Err(e) => tracing::debug!(error=%e, "skipping bad memory-stats line"),
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effectiveness_factor_defaults_to_one() {
        let e = MemoryEffectiveness::default();
        assert!((e.factor() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn effectiveness_factor_scales() {
        let e = MemoryEffectiveness {
            loaded: 10,
            referenced: 5,
            ..Default::default()
        };
        assert!((e.factor() - 0.75).abs() < 1e-6);

        let e = MemoryEffectiveness {
            loaded: 10,
            referenced: 0,
            ..Default::default()
        };
        assert!((e.factor() - 0.5).abs() < 1e-6);

        let e = MemoryEffectiveness {
            loaded: 10,
            referenced: 10,
            ..Default::default()
        };
        assert!((e.factor() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn decay_factor_no_events() {
        let e = MemoryEffectiveness::default();
        assert!((e.decay_factor(chrono::Utc::now()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn decay_factor_recent() {
        let now = chrono::Utc::now();
        let e = MemoryEffectiveness {
            last_event_at: Some(now),
            ..Default::default()
        };
        assert!((e.decay_factor(now) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn decay_factor_30_days() {
        let now = chrono::Utc::now();
        let thirty_days_ago = now - chrono::Duration::days(30);
        let e = MemoryEffectiveness {
            last_event_at: Some(thirty_days_ago),
            ..Default::default()
        };
        assert!((e.decay_factor(now) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn decay_factor_floor() {
        let now = chrono::Utc::now();
        let long_ago = now - chrono::Duration::days(365);
        let e = MemoryEffectiveness {
            last_event_at: Some(long_ago),
            ..Default::default()
        };
        assert!((e.decay_factor(now) - 0.3).abs() < 1e-6);
    }
}
