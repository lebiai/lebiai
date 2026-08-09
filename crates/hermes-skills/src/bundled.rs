//! Bundled skills that ship inside the binary and auto-install at startup.
//!
//! Three meta-skills live in `crates/hermes-skills/bundled/` and are shared
//! by every entry surface (CLI / GUI / channels) so the engine behaves the
//! same everywhere: memory-palace, skill-creator, find-skills. They are
//! user-scoped and idempotent to install.

use crate::{parse_skill_doc, FsSkillStore, Scope, SkillStore};

/// Auto-install the bundled skills into the user skill store (idempotent).
/// Failures are logged and swallowed — a broken bundle must never block startup.
pub fn auto_install_bundled(skill_store: &FsSkillStore) {
    auto_install_palace_skill(skill_store);
    auto_install_skill_creator_skill(skill_store);
    auto_install_find_skills_skill(skill_store);
}

fn auto_install_palace_skill(skill_store: &FsSkillStore) {
    if let Ok(Some(_)) = skill_store.get("memory-palace") {
        return;
    }
    let raw = include_str!("../bundled/memory-palace/SKILL.md");
    install_bundled_skill(skill_store, "memory-palace", raw);
}

/// Upgrade-aware: an older install that only has SKILL.md (no
/// `agents/grader.md`) is considered stale and re-installed in full, so
/// users never need to remove anything by hand.
fn auto_install_skill_creator_skill(skill_store: &FsSkillStore) {
    let already_full = matches!(skill_store.get("skill-creator"), Ok(Some(_)))
        && matches!(
            skill_store.skill_dir(Scope::User, "skill-creator"),
            Ok(d) if d.join("agents").join("grader.md").is_file()
        );
    if already_full {
        return;
    }

    let raw = include_str!("../bundled/skill-creator/SKILL.md");
    install_bundled_skill(skill_store, "skill-creator", raw);

    // SKILL.md is in place — now lay down the bundled subfiles next to it.
    // Paths are compile-time constants (`include_str!`), so no path
    // validation is needed here; the trust boundary is the source tree.
    let extra_files: &[(&str, &str)] = &[
        (
            "agents/grader.md",
            include_str!("../bundled/skill-creator/agents/grader.md"),
        ),
        (
            "agents/comparator.md",
            include_str!("../bundled/skill-creator/agents/comparator.md"),
        ),
        (
            "agents/analyzer.md",
            include_str!("../bundled/skill-creator/agents/analyzer.md"),
        ),
        (
            "references/schemas.md",
            include_str!("../bundled/skill-creator/references/schemas.md"),
        ),
    ];
    write_bundled_subfiles(skill_store, "skill-creator", extra_files);
}

fn auto_install_find_skills_skill(skill_store: &FsSkillStore) {
    if let Ok(Some(_)) = skill_store.get("find-skills") {
        return;
    }
    let raw = include_str!("../bundled/find-skills/SKILL.md");
    install_bundled_skill(skill_store, "find-skills", raw);
}

/// Shared inner: parse a bundled SKILL.md and write it through the store.
fn install_bundled_skill(skill_store: &FsSkillStore, name: &str, raw: &str) {
    let (fm, body) = match parse_skill_doc(raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(skill = %name, error = %e, "bundled SKILL.md failed to parse");
            return;
        }
    };
    match skill_store.put(Scope::User, fm, &body) {
        Ok(p) => tracing::info!(skill = %name, path = %p.display(), "auto-installed bundled skill"),
        Err(e) => tracing::warn!(skill = %name, error = %e, "failed to auto-install bundled skill"),
    }
}

/// Write extra files alongside a bundled skill's SKILL.md (the level-3
/// Progressive Disclosure payload — `agents/` / `references/`). Pass
/// `subfiles` as `(rel_path, content)` pairs. Failures are logged and
/// swallowed for the same reason as [`install_bundled_skill`].
fn write_bundled_subfiles(skill_store: &FsSkillStore, name: &str, subfiles: &[(&str, &str)]) {
    let base = match skill_store.skill_dir(Scope::User, name) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(skill = %name, error = %e, "resolving bundled skill dir failed");
            return;
        }
    };
    for (rel, content) in subfiles {
        let target = base.join(rel);
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(skill = %name, rel = %rel, error = %e, "create subfile dir failed");
                continue;
            }
        }
        if let Err(e) = std::fs::write(&target, content) {
            tracing::warn!(skill = %name, rel = %rel, error = %e, "write bundled subfile failed");
        }
    }
}

#[cfg(test)]
mod tests {
    //! Compile-time guards for the bundled meta-skills. If their SKILL.md
    //! frontmatter regresses (missing field, bad YAML, etc.) the test fails
    //! at `cargo test`, long before users see a broken cold start.

    use super::*;

    #[test]
    fn skill_creator_bundle_parses_and_is_not_always_active() {
        let raw = include_str!("../bundled/skill-creator/SKILL.md");
        let (fm, body) = parse_skill_doc(raw).expect("bundled skill-creator SKILL.md must parse");
        assert_eq!(fm.name, "skill-creator");
        assert!(!fm.description.is_empty());
        assert!(
            !fm.always_active,
            "skill-creator should not be always_active"
        );
        assert!(body.contains("Skill Creator"));
        assert!(
            body.contains("agents/grader.md"),
            "skill-creator body must link to its bundled grader prompt"
        );
    }

    #[test]
    fn find_skills_bundle_parses_and_is_not_always_active() {
        let raw = include_str!("../bundled/find-skills/SKILL.md");
        let (fm, body) = parse_skill_doc(raw).expect("bundled find-skills SKILL.md must parse");
        assert_eq!(fm.name, "find-skills");
        assert!(!fm.description.is_empty());
        assert!(!fm.always_active, "find-skills should not be always_active");
        assert!(body.contains("Finding and Installing Skills"));
    }

    #[test]
    fn memory_palace_bundle_parses_and_is_always_active() {
        let raw = include_str!("../bundled/memory-palace/SKILL.md");
        let (fm, body) = parse_skill_doc(raw).expect("bundled memory-palace SKILL.md must parse");
        assert_eq!(fm.name, "memory-palace");
        assert!(
            fm.always_active,
            "memory-palace protocol must be always_active"
        );
        assert!(body.contains("Memory Palace Protocol"));
    }

    #[test]
    fn auto_install_writes_all_bundled_skills_into_a_fresh_store() {
        use crate::{FsSkillStore, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        auto_install_bundled(&store);

        assert!(store.get("memory-palace").unwrap().is_some());
        assert!(store.get("skill-creator").unwrap().is_some());
        assert!(store.get("find-skills").unwrap().is_some());
    }

    #[test]
    fn auto_install_skill_creator_writes_full_multi_file_bundle() {
        use crate::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        auto_install_bundled(&store);

        // SKILL.md registered through the store.
        assert!(store.get("skill-creator").unwrap().is_some());

        // All bundled subfiles landed on disk next to SKILL.md.
        let dir = store
            .skill_dir(Scope::User, "skill-creator")
            .expect("skill_dir for skill-creator");
        for rel in [
            "agents/grader.md",
            "agents/comparator.md",
            "agents/analyzer.md",
            "references/schemas.md",
        ] {
            let p = dir.join(rel);
            assert!(
                p.is_file(),
                "expected bundled subfile {} to exist",
                p.display()
            );
        }
    }

    #[test]
    fn auto_install_skill_creator_is_upgrade_aware() {
        // Simulate an older install: SKILL.md present, but no
        // agents/grader.md. The upgrade-detection branch should re-run
        // the full installer and lay down all subfiles.
        use crate::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        auto_install_bundled(&store);
        let dir = store.skill_dir(Scope::User, "skill-creator").unwrap();
        let marker = dir.join("agents").join("grader.md");
        assert!(marker.is_file());
        std::fs::remove_file(&marker).unwrap();

        auto_install_bundled(&store);

        assert!(marker.is_file());
        assert!(dir.join("references").join("schemas.md").is_file());
    }

    #[test]
    fn auto_install_skill_creator_is_noop_when_bundle_already_complete() {
        // When the multi-file marker is already on disk, the function
        // must not overwrite (preserves any local edits a user made).
        use crate::{FsSkillStore, Scope, SkillStore};
        let tmp = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(tmp.path().to_path_buf(), None);

        auto_install_bundled(&store);

        // Overwrite SKILL.md with a sentinel and the marker file with a
        // sentinel; if the second call no-ops, both survive.
        let dir = store.skill_dir(Scope::User, "skill-creator").unwrap();
        let skill_md = dir.join("SKILL.md");
        let marker = dir.join("agents").join("grader.md");
        std::fs::write(&skill_md, "---\nname: skill-creator\ndescription: sentinel\nalways_active: false\n---\nsentinel-body\n").unwrap();
        std::fs::write(&marker, "sentinel-grader").unwrap();

        auto_install_bundled(&store);

        assert_eq!(std::fs::read_to_string(&skill_md).unwrap().trim_end(), "---\nname: skill-creator\ndescription: sentinel\nalways_active: false\n---\nsentinel-body");
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "sentinel-grader");
    }
}
