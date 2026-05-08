//! `hermes skills ...` — inspect / delete skills.

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

pub fn delete(name: &str, scope: Scope) -> Result<()> {
    let store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let removed = store
        .delete(scope, name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if removed {
        println!("removed {name} from {scope:?}");
    } else {
        println!("{name} not found in {scope:?}");
    }
    Ok(())
}
