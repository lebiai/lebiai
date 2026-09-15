//! Memory Palace: zone-based organization for retrieval.

use std::collections::BTreeMap;

use hermes_core::companion::zones;

use crate::memory::LoadedMemory;

pub fn group_by_zone<'a>(memories: &'a [LoadedMemory]) -> BTreeMap<String, Vec<&'a LoadedMemory>> {
    let mut map: BTreeMap<String, Vec<&'a LoadedMemory>> = BTreeMap::new();
    for m in memories {
        map.entry(zones::normalize(&m.frontmatter.zone).to_string())
            .or_default()
            .push(m);
    }
    map
}

pub fn get_zone<'a>(memories: &'a [LoadedMemory], zone: &str) -> Vec<&'a LoadedMemory> {
    memories
        .iter()
        .filter(|m| zones::same(&m.frontmatter.zone, zone))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, zone: &str, body: &str) -> LoadedMemory {
        let mut fm =
            MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], zone.to_string());
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
            mem("m1", "core", "user prefers vim"),
            mem("m2", "preferences", "user is architect"),
            mem("m3", "work", "working on palace"),
            mem("m4", "general", "misc fact"),
        ];
        let grouped = group_by_zone(&mems);
        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[zones::PREFERENCES].len(), 2);
        assert_eq!(grouped[zones::WORK].len(), 1);
        assert_eq!(grouped[zones::GENERAL].len(), 1);
    }

    #[test]
    fn get_zone_folds_legacy_core() {
        let mems = vec![
            mem("m1", "core", "core fact"),
            mem("m2", "work", "work fact"),
        ];
        let prefs = get_zone(&mems, zones::PREFERENCES);
        assert_eq!(prefs.len(), 1);
        assert_eq!(prefs[0].frontmatter.id, "m1");
        assert_eq!(get_zone(&mems, "core").len(), 1);
    }
}
