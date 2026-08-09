//! `hermes reflect-stats` — show meta-reflection signal: acceptance rate
//! and recent decisions, to help the user decide whether to refine the
//! reflection prompt itself.

use anyhow::Result;
use hermes_reflect::{log_read_all, log_stats, ActionTaken, CandidateKind};

const LOW_ACCEPTANCE_THRESHOLD: f64 = 0.30;

pub fn run(last: usize) -> Result<()> {
    let stats = log_stats(Some(last)).map_err(|e| anyhow::anyhow!("{e}"))?;
    if stats.total == 0 {
        println!("(no reflection log yet — run `hermes chat` and reflect a few times)");
        return Ok(());
    }
    let rate = stats.acceptance_rate().unwrap_or(0.0);
    println!(
        "last {} candidates · accept {} · reject {} · defer {} · other {}",
        stats.total, stats.accepted, stats.rejected, stats.deferred, stats.other
    );
    println!("acceptance rate: {:.0}%", rate * 100.0);
    if rate < LOW_ACCEPTANCE_THRESHOLD && stats.total >= 10 {
        println!();
        println!(
            "⚠ acceptance rate below {:.0}% — the reflection prompt may be \
             over-proposing. Consider editing crates/hermes-reflect/src/prompt.rs to be \
             more conservative, or adjust [reflect].min_turns.",
            LOW_ACCEPTANCE_THRESHOLD * 100.0
        );
    }

    // Per-kind breakdown.
    let all = log_read_all().map_err(|e| anyhow::anyhow!("{e}"))?;
    let recent: Vec<_> = if all.len() > last {
        all.iter().skip(all.len() - last).collect()
    } else {
        all.iter().collect()
    };
    println!();
    println!("Per-kind:");
    for kind in [
        CandidateKind::Skill,
        CandidateKind::Memory,
        CandidateKind::ConflictMemory,
        CandidateKind::OrphanConflict,
    ] {
        let total = recent.iter().filter(|e| e.kind == kind).count();
        if total == 0 {
            continue;
        }
        let accepted = recent
            .iter()
            .filter(|e| e.kind == kind && matches!(e.action, ActionTaken::Accept))
            .count();
        println!(
            "  {:?}: {}/{} accepted ({:.0}%)",
            kind,
            accepted,
            total,
            (accepted as f64 / total as f64) * 100.0
        );
    }
    Ok(())
}
