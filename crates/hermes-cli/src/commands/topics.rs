//! `hermes topics` — topic-card distillation of the memory store.
//!
//! Cards group memories by **subject** and are a derived view: no memory is
//! ever merged, superseded or deleted here. Rebuilding is always safe.
//!
//! Modes:
//!   `hermes topics`                 list the cards (no model call)
//!   `hermes topics --build`         fold memories that no card covers into the cards
//!   `hermes topics --build --rebuild`  re-cut every theme from the full active set

use anyhow::{Context, Result};
use hermes_memory::{FsMemoryStore, MemoryStore};

pub async fn run(build: bool, rebuild: bool) -> Result<()> {
    let store = FsMemoryStore::standard()?;
    let active = store.list_active().context("loading active memories")?;
    if active.is_empty() {
        println!("(no memories yet — nothing to organise)");
        return Ok(());
    }

    // 命令行今天没有工位语境：看/整理的都是「全局那份」，所以只算全局可见的记忆。
    let view = hermes_memory::visible_owned(&active, None);
    let stored = hermes_memory::topics::load(None).unwrap_or_default();
    let cards = hermes_memory::topics::prune(&stored, &view);

    if !build {
        if cards.is_empty() {
            println!(
                "no topic cards yet ({} active memories). Run `hermes topics --build`.",
                active.len()
            );
            return Ok(());
        }
        print_cards(&cards, &view);
        return Ok(());
    }

    let cfg = super::util::load_config_or_hint()?;
    let provider = super::util::build_active_provider(&cfg)?;
    println!(
        "{} {} active memories...",
        if rebuild {
            "re-cutting themes from"
        } else {
            "folding new memories into"
        },
        active.len()
    );
    let fresh = hermes_reflect::build_topic_cards(provider.as_ref(), &view, &stored, rebuild)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let path = hermes_memory::topics::save(None, &fresh)?;
    println!("✓ {} cards → {}", fresh.cards.len(), path.display());
    println!();
    print_cards(&fresh, &view);
    Ok(())
}

fn print_cards(cards: &hermes_memory::TopicCards, active: &[hermes_memory::LoadedMemory]) {
    for card in &cards.cards {
        println!("● {} ({})", card.title, card.members.len());
        for line in card.summary.lines() {
            println!("    {line}");
        }
        println!("    members: {}", card.members.join(", "));
        println!();
    }
    let left = hermes_memory::topics::unassigned(cards, active);
    if left.is_empty() {
        println!("all {} active memories are covered.", active.len());
    } else {
        println!(
            "{} active memories are not on any card — run `hermes topics --build`.",
            left.len()
        );
    }
}
