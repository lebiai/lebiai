//! Memory Palace: zone-based organization, index building, and file I/O.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::memory::LoadedMemory;

pub const ZONE_CORE: &str = "core";
pub const ZONE_WORK: &str = "work";
pub const ZONE_EPISODE: &str = "episode";
pub const ZONE_GENERAL: &str = "general";

pub fn group_by_zone<'a>(memories: &'a [LoadedMemory]) -> BTreeMap<String, Vec<&'a LoadedMemory>> {
    let mut map: BTreeMap<String, Vec<&'a LoadedMemory>> = BTreeMap::new();
    for m in memories {
        map.entry(m.frontmatter.zone.clone())
            .or_default()
            .push(m);
    }
    map
}

pub fn get_zone<'a>(memories: &'a [LoadedMemory], zone: &str) -> Vec<&'a LoadedMemory> {
    memories
        .iter()
        .filter(|m| m.frontmatter.zone == zone)
        .collect()
}

/// Code-generated palace index (no LLM needed). Groups by zone, shows counts
/// and first-line previews. ~200 tokens.
pub fn build_palace_index_simple(memories: &[LoadedMemory]) -> String {
    let zones = group_by_zone(memories);
    if zones.is_empty() {
        return "## Memory Palace\nEmpty — no memories yet.".to_string();
    }
    let total: usize = zones.values().map(|v| v.len()).sum();
    let mut buf = format!(
        "## Memory Palace\n{} memories across {} zones. Use palace_read_zone to load details.\n",
        total,
        zones.len()
    );
    for (zone, mems) in &zones {
        buf.push_str(&format!("\n### {} ({})\n", zone, mems.len()));
        for m in mems.iter().take(5) {
            let line = m.body.lines().next().unwrap_or("").trim();
            let preview: String = line.chars().take(80).collect();
            buf.push_str(&format!("- {preview}\n"));
        }
        if mems.len() > 5 {
            buf.push_str(&format!("- ... ({} more)\n", mems.len() - 5));
        }
    }
    buf
}

fn base_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolving $HOME")?;
    Ok(home.join(".small-rust-hermes"))
}

pub fn palace_index_path() -> Result<PathBuf> {
    Ok(base_dir()?.join("palace-index.md"))
}

pub fn load_palace_index() -> Result<Option<String>> {
    let path = palace_index_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(content))
}

pub fn save_palace_index(content: &str) -> Result<PathBuf> {
    let path = palace_index_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(".palace-index.md.tmp");
    std::fs::write(&tmp, content)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(path)
}

fn sanitize_zone_filename(zone: &str) -> String {
    zone.replace(':', "_")
}

pub fn zone_cache_dir() -> Result<PathBuf> {
    let dir = base_dir()?.join("zone-cache");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

pub fn load_zone_summary(zone: &str) -> Result<Option<String>> {
    let dir = zone_cache_dir()?;
    let path = dir.join(format!("{}.md", sanitize_zone_filename(zone)));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(content))
}

pub fn save_zone_summary(zone: &str, content: &str) -> Result<PathBuf> {
    let dir = zone_cache_dir()?;
    let filename = format!("{}.md", sanitize_zone_filename(zone));
    let path = dir.join(&filename);
    let tmp = dir.join(format!(".{filename}.tmp"));
    std::fs::write(&tmp, content)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, zone: &str, body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::User,
            Confidence::Medium,
            vec![],
            zone.to_string(),
        );
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn group_by_zone_separates_zones() {
        let mems = vec![
            mem("m1", ZONE_CORE, "user prefers vim"),
            mem("m2", ZONE_CORE, "user is architect"),
            mem("m3", ZONE_WORK, "working on palace"),
            mem("m4", ZONE_GENERAL, "misc fact"),
        ];
        let grouped = group_by_zone(&mems);
        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[ZONE_CORE].len(), 2);
        assert_eq!(grouped[ZONE_WORK].len(), 1);
        assert_eq!(grouped[ZONE_GENERAL].len(), 1);
    }

    #[test]
    fn get_zone_filters() {
        let mems = vec![
            mem("m1", ZONE_CORE, "core fact"),
            mem("m2", ZONE_WORK, "work fact"),
        ];
        let core = get_zone(&mems, ZONE_CORE);
        assert_eq!(core.len(), 1);
        assert_eq!(core[0].frontmatter.id, "m1");
    }

    #[test]
    fn build_palace_index_simple_output() {
        let mems = vec![
            mem("m1", ZONE_CORE, "user prefers vim"),
            mem("m2", ZONE_CORE, "user is architect"),
            mem("m3", ZONE_WORK, "working on palace"),
        ];
        let index = build_palace_index_simple(&mems);
        assert!(index.contains("Memory Palace"));
        assert!(index.contains("3 memories across 2 zones"));
        assert!(index.contains("### core (2)"));
        assert!(index.contains("### work (1)"));
        assert!(index.contains("user prefers vim"));
    }

    #[test]
    fn build_palace_index_empty() {
        let index = build_palace_index_simple(&[]);
        assert!(index.contains("Empty"));
    }

    #[test]
    fn sanitize_zone_filename_replaces_colon() {
        assert_eq!(sanitize_zone_filename("project:hermes"), "project_hermes");
        assert_eq!(sanitize_zone_filename("core"), "core");
    }

    #[test]
    fn zone_cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let zone = "test_zone";
        let filename = format!("{}.md", sanitize_zone_filename(zone));
        let path = dir.path().join(&filename);
        let content = "## Test Zone Summary\n- fact 1\n- fact 2\n";
        std::fs::write(&path, content).unwrap();
        let read = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read, content);
    }
}
