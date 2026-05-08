//! `hermes memory ...` — inspect, delete, pin, unpin.

use anyhow::Result;
use hermes_memory::{FsMemoryStore, MemoryStore, Scope};
use hermes_store::{read_doc, write_doc_atomic, FrontmatterDoc};

pub enum Filter {
    Active,
    All,
    Pinned,
}

pub fn list(filter: Filter) -> Result<()> {
    let store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let items = match filter {
        Filter::Active => store.list_active().map_err(|e| anyhow::anyhow!("{e}"))?,
        Filter::All => store.list().map_err(|e| anyhow::anyhow!("{e}"))?,
        Filter::Pinned => store.list_pinned().map_err(|e| anyhow::anyhow!("{e}"))?,
    };
    if items.is_empty() {
        println!("(no memories)");
        return Ok(());
    }
    for m in items {
        let pin = if m.frontmatter.pinned { "★ " } else { "  " };
        let fact = m.body.lines().next().unwrap_or("").trim();
        println!(
            "{pin}{}  [{:?}]  {}  {}",
            m.frontmatter.id, m.scope, m.frontmatter.created.format("%Y-%m-%d"), fact
        );
    }
    Ok(())
}

pub fn show(id: &str) -> Result<()> {
    let store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let m = store
        .get(id)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .ok_or_else(|| anyhow::anyhow!("no memory with id {id:?}"))?;
    println!("# {}  [{:?}]", m.frontmatter.id, m.scope);
    println!("source:      {}", m.source_path.display());
    println!("created:     {}", m.frontmatter.created);
    println!("confidence:  {:?}", m.frontmatter.confidence);
    println!("origin:      {:?}", m.frontmatter.source);
    println!("pinned:      {}", m.frontmatter.pinned);
    if !m.frontmatter.tags.is_empty() {
        println!("tags:        {}", m.frontmatter.tags.join(", "));
    }
    if !m.frontmatter.supersedes.is_empty() {
        println!("supersedes:  {}", m.frontmatter.supersedes.join(", "));
    }
    println!();
    println!("{}", m.body.trim());
    Ok(())
}

pub fn delete(id: &str, scope: Scope) -> Result<()> {
    let store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let removed = store
        .delete(scope, id)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if removed {
        println!("removed {id} from {scope:?}");
    } else {
        println!("{id} not found in {scope:?}");
    }
    Ok(())
}

pub fn set_pinned(id: &str, pinned: bool) -> Result<()> {
    let store = FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("{e}"))?;
    let m = store
        .get(id)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .ok_or_else(|| anyhow::anyhow!("no memory with id {id:?}"))?;

    if m.frontmatter.pinned == pinned {
        println!("{id} is already pinned={pinned}");
        return Ok(());
    }

    // Read the raw doc so we preserve fields the store trait doesn't know
    // about (frontmatter `extra`, arbitrary source-written keys).
    let doc: FrontmatterDoc<hermes_memory::MemoryFrontmatter> =
        read_doc(&m.source_path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut new_fm = doc.frontmatter;
    new_fm.pinned = pinned;
    let new_doc = FrontmatterDoc {
        frontmatter: new_fm,
        body: doc.body,
    };
    write_doc_atomic(&m.source_path, &new_doc).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "{} {id}",
        if pinned { "pinned" } else { "unpinned" }
    );
    Ok(())
}
