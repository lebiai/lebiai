//! `hermes distill` — cross-session distillation of the memory store.
//!
//! This is the one place where the knowledge base *actively converges* rather
//! than only growing. It scans the active memories, clusters near-duplicates
//! (TF-IDF cosine, no model), and collapses each cluster into one survivor
//! whose `supersedes` lists the other members. The writing reuses the same
//! `persist_memory` path as reflection, so no new store semantics are added.
//!
//! Modes, in order of cost:
//!   `hermes distill`                     dry-run, prints clusters only
//!   `hermes distill --apply`             per-cluster prompt; survivor body is
//!                                        the winner's verbatim body
//!   `hermes distill --apply --llm-merge` per-cluster prompt; one LLM call
//!                                        per accepted cluster synthesises a
//!                                        merged body (the quality uplift)
//!
//! Protected memories (`zone == "core"` or `pinned`) are reported but never
//! applied automatically.

use std::io::Write;

use anyhow::{Context, Result};
use hermes_core::{CompletionRequest, LlmProvider, Message};
use hermes_memory::{
    distill::{find_clusters, Cluster},
    load_effectiveness, Confidence, FsMemoryStore, LoadedMemory, MemoryStore,
};
use hermes_reflect::MemoryCandidate;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::reflect::persist_memory;

/// CLI options mirroring the `Distill` clap variant in `main.rs`.
pub struct DistillOpts {
    pub apply: bool,
    pub llm_merge: bool,
    pub threshold: f64,
}

pub async fn run(opts: &DistillOpts) -> Result<()> {
    if opts.llm_merge && !opts.apply {
        anyhow::bail!("--llm-merge requires --apply (it only fires for accepted clusters)");
    }

    let store = FsMemoryStore::standard()?;
    let active = store.list_active().context("loading active memories")?;
    if active.len() < 2 {
        println!("(only {} active memory — nothing to distill)", active.len());
        return Ok(());
    }

    let eff = load_effectiveness().unwrap_or_default();
    let clusters = find_clusters(&active, opts.threshold, Some(&eff));
    if clusters.is_empty() {
        println!(
            "no near-duplicate clusters at threshold {:.2} ({} active memories scanned)",
            opts.threshold,
            active.len()
        );
        return Ok(());
    }

    let total_superseded: usize = clusters
        .iter()
        .map(|c| c.superseded_ids(&active).len())
        .sum();
    let protected_count = clusters.iter().filter(|c| c.protected).count();

    println!(
        "found {} cluster(s); {} memory(ies) would be superseded{}",
        clusters.len(),
        total_superseded,
        if protected_count > 0 {
            format!(" ({protected_count} protected — review only)")
        } else {
            String::new()
        }
    );
    println!();

    if !opts.apply {
        for (n, c) in clusters.iter().enumerate() {
            print_cluster(n, c, &active);
        }
        println!();
        println!("dry-run — no changes. Re-run with --apply to merge.");
        return Ok(());
    }

    // --apply path: build provider up-front only if we'll need it.
    let provider = if opts.llm_merge {
        let cfg = super::util::load_config_or_hint()?;
        Some(super::util::build_active_provider(&cfg)?)
    } else {
        None
    };

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut applied = 0usize;

    for (n, c) in clusters.iter().enumerate() {
        print_cluster(n, c, &active);
        if c.protected {
            eprintln!("  (protected — skipping automatic merge)");
            println!();
            continue;
        }
        if !prompt_accept(&mut reader).await? {
            println!("  (skipped)\n");
            continue;
        }

        let body = if let Some(p) = provider.as_deref() {
            llm_merge_body(p, c, &active).await.unwrap_or_else(|e| {
                eprintln!("  ⚠ llm-merge failed ({e}); falling back to survivor body");
                active[c.survivor_idx].body.clone()
            })
        } else {
            active[c.survivor_idx].body.clone()
        };

        let survivor = &active[c.survivor_idx];
        // The merged file supersedes EVERY original member — including the
        // one whose body/tags we copied. Otherwise that member stays active
        // alongside the new file, leaving the duplication we set out to fix.
        let superseded: Vec<String> = c
            .members
            .iter()
            .filter_map(|&i| active.get(i).map(|m| m.id().to_string()))
            .collect();
        let candidate = MemoryCandidate {
            fact: body,
            tags: survivor.frontmatter.tags.clone(),
            zone: survivor.frontmatter.zone.clone(),
            scope: survivor.scope,
            confidence: Confidence::High, // merged from multiple corroborated sources
            rationale: format!("distilled from {} overlapping memories", c.members.len()),
            supersedes: superseded.clone(),
        };
        match persist_memory(&store, &candidate) {
            Ok(path) => {
                applied += 1;
                println!("  ✓ wrote {}", path.display());
                log_distill(&survivor.frontmatter.id, &superseded, "merge");
            }
            Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
        }
        println!();
    }

    println!("distill complete: {applied} cluster(s) merged");
    Ok(())
}

fn print_cluster(n: usize, c: &Cluster, active: &[LoadedMemory]) {
    let marker = if c.protected { " [protected]" } else { "" };
    println!(
        "cluster {n} · {} members · max similarity {:.2}{}",
        c.members.len(),
        c.max_score,
        marker
    );
    let survivor = &active[c.survivor_idx];
    println!("  ★ keep: {} — {}", survivor.id(), one_line(&survivor.body));
    for id in c.superseded_ids(active) {
        if let Some(m) = active.iter().find(|x| x.id() == id) {
            println!("    – {} — {}", m.id(), one_line(&m.body));
        }
    }
}

/// Read one yes/no from stdin. Blank = skip. Returns true for accept.
async fn prompt_accept(reader: &mut tokio::io::Lines<BufReader<tokio::io::Stdin>>) -> Result<bool> {
    let mut stderr = std::io::stderr();
    loop {
        write!(stderr, "  [a]ccept / [s]kip (default): ")?;
        stderr.flush().ok();
        let Some(line) = reader.next_line().await.context("reading stdin")? else {
            return Ok(false);
        };
        match line.trim().to_lowercase().as_str() {
            "a" | "accept" | "y" | "yes" => return Ok(true),
            "s" | "skip" | "" => return Ok(false),
            other => eprintln!("  (unknown {other:?} — try a / s)"),
        }
    }
}

/// One LLM call: ask the model to fuse N overlapping facts into a single,
/// denser statement. Only invoked when `--llm-merge` is set *and* the user
/// accepted this specific cluster — cost is bounded to accepted clusters.
async fn llm_merge_body(
    provider: &dyn LlmProvider,
    c: &Cluster,
    active: &[LoadedMemory],
) -> Result<String> {
    let mut facts = String::new();
    for (i, &idx) in c.members.iter().enumerate() {
        let m = &active[idx];
        facts.push_str(&format!("{}. {}\n", i + 1, m.body.trim()));
    }
    let system = "You merge overlapping factual memories into one concise statement. \
        Preserve every distinct detail, resolve wording differences, drop pure duplication. \
        Output ONLY the merged statement, no preamble, no quotes, no markdown.";
    let user = format!("Merge these into one statement:\n\n{facts}");
    let req = CompletionRequest {
        model: String::new(),
        system: Some(system.to_string()),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: 512,
        temperature: Some(0.1),
        enable_caching: false,
    };
    let resp = provider
        .complete(req)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let body = resp.text().trim().to_string();
    if body.is_empty() {
        anyhow::bail!("empty merge response");
    }
    Ok(body)
}

fn log_distill(survivor_id: &str, superseded: &[String], action: &str) {
    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: format!("distill/{survivor_id}"),
        kind: hermes_reflect::CandidateKind::Memory,
        action: hermes_reflect::ActionTaken::Merge,
        label: format!("{action}: superseded [{}]", superseded.join(", ")),
    });
}

fn one_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}
