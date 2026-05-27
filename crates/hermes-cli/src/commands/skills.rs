//! `hermes skills ...` — inspect / install / delete skills.

use anyhow::Result;
use hermes_skills::{FsSkillStore, Scope, SkillStore};

pub fn list(scope_filter: Option<Scope>) -> Result<()> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut skills = store.list().map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(sc) = scope_filter {
        skills.retain(|s| s.scope == sc);
    }
    if skills.is_empty() {
        println!("(no skills)");
        return Ok(());
    }
    for s in skills {
        println!(
            "{}  [{:?}]  — {}",
            s.frontmatter.name, s.scope, s.frontmatter.description
        );
    }
    Ok(())
}

pub fn show(name: &str) -> Result<()> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let s = store
        .get(name)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .ok_or_else(|| anyhow::anyhow!("no skill named {name:?}"))?;
    println!("# {}  [{:?}]", s.frontmatter.name, s.scope);
    println!("source: {}", s.source.display());
    if !s.frontmatter.triggers.is_empty() {
        println!("triggers: {}", s.frontmatter.triggers.join(", "));
    }
    println!();
    println!("{}", s.body.trim());
    Ok(())
}

/// Install a skill from a slug (`owner/repo@slug`) or raw URL. Delegates
/// to the shared sync logic in `hermes_skills::install_from_source`
/// (wrapped in `spawn_blocking` so we don't stall the runtime).
pub async fn install(source: &str, overwrite: bool, git_ref: Option<&str>) -> Result<()> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("Fetching {source}…");
    let source_owned = source.to_string();
    let git_ref_owned = git_ref.map(|s| s.to_string());
    let outcome = tokio::task::spawn_blocking(move || {
        hermes_skills::install_from_source(
            &store,
            &source_owned,
            overwrite,
            git_ref_owned.as_deref(),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("join error: {e}"))??;

    println!("✓ Installed {} (ref={})", outcome.name, outcome.resolved_ref);
    println!("  Description: {}", outcome.description);
    println!(
        "  {} file{} ({} bytes total):",
        outcome.files_written.len(),
        if outcome.files_written.len() == 1 {
            ""
        } else {
            "s"
        },
        outcome.total_bytes
    );
    for f in &outcome.files_written {
        println!("    - {f}");
    }
    Ok(())
}

/// Delete a locally-installed skill. Routes through `delete_skill` so
/// bundled meta-skills are protected consistently with the agent-side
/// `skill_delete` tool.
pub fn delete(name: &str) -> Result<()> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let outcome = hermes_skills::delete_skill(&store, name)?;
    println!(
        "✓ Deleted {} ({} file{} removed)",
        outcome.name,
        outcome.files_removed,
        if outcome.files_removed == 1 { "" } else { "s" }
    );
    Ok(())
}
