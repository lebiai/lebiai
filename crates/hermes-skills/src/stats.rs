//! Skill effectiveness tracking: record when a skill is matched vs actually
//! used (a turn counts as "used" when it has real output — tool calls or
//! >=40 chars of text). Skills with low used/match ratio are factored into relevance scoring.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStatEntry {
    pub at: chrono::DateTime<chrono::Utc>,
    pub skill_name: String,
    pub event: SkillEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillEvent {
    Matched,
    Used,
}

fn default_path() -> Result<PathBuf> {
    Ok(hermes_core::data_path("skill-stats.jsonl"))
}

/// Append a stat entry. Best-effort write.
pub fn record(entry: SkillStatEntry) {
    if let Err(e) = try_record(entry) {
        tracing::warn!(error=%e, "failed to record skill stat");
    }
}

fn try_record(entry: SkillStatEntry) -> Result<()> {
    let path = default_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&entry)?;
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Per-skill effectiveness summary.
#[derive(Debug, Clone, Default)]
pub struct SkillEffectiveness {
    pub matched: usize,
    pub used: usize,
}

impl SkillEffectiveness {
    /// Effectiveness factor in [0.5, 1.0]. Skills with no data default to 1.0.
    /// Low used/match ratio pulls the factor down toward 0.5.
    pub fn factor(&self) -> f64 {
        if self.matched == 0 {
            return 1.0;
        }
        let ratio = self.used as f64 / self.matched as f64;
        // Map [0, 1] → [0.5, 1.0] so even never-used skills still have a
        // chance to match (just deprioritized).
        0.5 + 0.5 * ratio
    }
}

/// Load effectiveness stats for all skills. Returns a map from skill name to
/// its effectiveness summary.
pub fn load_effectiveness() -> Result<HashMap<String, SkillEffectiveness>> {
    let path = default_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let f = std::fs::File::open(&path)?;
    let reader = BufReader::new(f);
    let mut map: HashMap<String, SkillEffectiveness> = HashMap::new();
    for line in reader.lines().map_while(|r| r.ok()) {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<SkillStatEntry>(&line) {
            Ok(e) => {
                let entry = map.entry(e.skill_name).or_default();
                match e.event {
                    SkillEvent::Matched => entry.matched += 1,
                    SkillEvent::Used => entry.used += 1,
                }
            }
            Err(e) => tracing::debug!(error=%e, "skipping bad skill-stats line"),
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effectiveness_factor_defaults_to_one() {
        let e = SkillEffectiveness::default();
        assert!((e.factor() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn effectiveness_factor_scales() {
        let e = SkillEffectiveness {
            matched: 10,
            used: 5,
        };
        // ratio = 0.5, factor = 0.5 + 0.5 * 0.5 = 0.75
        assert!((e.factor() - 0.75).abs() < 1e-6);

        let e = SkillEffectiveness {
            matched: 10,
            used: 0,
        };
        // ratio = 0.0, factor = 0.5
        assert!((e.factor() - 0.5).abs() < 1e-6);

        let e = SkillEffectiveness {
            matched: 10,
            used: 10,
        };
        // ratio = 1.0, factor = 1.0
        assert!((e.factor() - 1.0).abs() < 1e-6);
    }
}
