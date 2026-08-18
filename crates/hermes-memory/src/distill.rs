//! Cross-session distillation: cluster near-duplicate memories so a caller
//! can collapse N overlapping facts into one survivor.
//!
//! This is the only place where the knowledge base *actively converges*
//! instead of only growing. It is deliberately model-free and deterministic:
//! TF-IDF cosine similarity (reusing [`crate::relevance`]) feeds a union-find
//! clustering; the survivor within a cluster is picked by effectiveness
//! (the first consumer that *acts on* the stats, not just down-ranks them).
//!
//! This module produces [`Cluster`]s only. Writing the survivor back to disk
//! (by setting `supersedes` on a new memory) lives in the CLI command layer,
//! which reuses the existing reflection persistence path.

use std::collections::HashMap;

use hermes_core::companion::zones;

use crate::memory::LoadedMemory;
use crate::relevance::search_memories_scored;
use crate::stats::MemoryEffectiveness;

/// Default cosine-similarity threshold above which two memories are
/// considered near-duplicates of each other.
///
/// Tuned for short factual statements under TF-IDF: in practice two genuine
/// rewordings of the same fact score around 0.55–0.65, while unrelated facts
/// score below ~0.1. 0.55 sits in the gap. The CLI exposes `--threshold`
/// so the user can tighten or loosen it.
pub const DEFAULT_THRESHOLD: f64 = 0.55;

/// One cluster of near-duplicate memories plus the chosen survivor.
///
/// `survivor_idx` / `superseded` index into the same `active` slice the
/// caller passed to [`find_clusters`]. `protected` memories (preferences,
/// including legacy `core`, or `pinned`) are never auto-merged — the cluster
/// is still reported so the user can review it, but the caller must not apply it.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Indices into the input slice that form this cluster (>= 2 members).
    pub members: Vec<usize>,
    /// Index of the member that should be kept. The rest are superseded.
    pub survivor_idx: usize,
    /// Highest pairwise similarity observed inside the cluster.
    pub max_score: f64,
    /// `true` if any member is preferences (incl. legacy `core`) or `pinned`.
    pub protected: bool,
}

impl Cluster {
    /// Ids of the members that would be superseded (everyone except the
    /// survivor). Empty for a protected cluster the caller chose to skip.
    pub fn superseded_ids<'a>(&self, active: &'a [LoadedMemory]) -> Vec<&'a str> {
        self.members
            .iter()
            .filter(|&&i| i != self.survivor_idx)
            .filter_map(|&i| active.get(i).map(|m| m.id()))
            .collect()
    }
}

/// Find clusters of near-duplicate memories across the whole active set.
///
/// Each memory's body is used as a query against the rest of the corpus via
/// [`search_memories_scored`]; any pair scoring above `threshold` is unioned.
/// Singletons (no neighbour above threshold) produce no cluster. Returns at
/// most one cluster per memory (the connected component it belongs to).
pub fn find_clusters(
    active: &[LoadedMemory],
    threshold: f64,
    eff: Option<&HashMap<String, MemoryEffectiveness>>,
) -> Vec<Cluster> {
    if active.len() < 2 || threshold <= 0.0 {
        return Vec::new();
    }

    // Build the pairwise "is similar" relation as edges, then union-find.
    let mut uf = UnionFind::new(active.len());
    // Track the strongest edge per eventual component, for reporting.
    let mut best_pair_score = vec![0.0_f64; active.len()];
    for (i, m) in active.iter().enumerate() {
        // `k` large enough to consider every other memory.
        let hits = search_memories_scored(active, &m.body, active.len());
        for (other, score) in hits {
            // `other` is a &LoadedMemory; resolve back to its index.
            let Some(j) = index_of(active, other.id()) else {
                continue;
            };
            if j == i || score < threshold {
                continue;
            }
            uf.union(i, j);
            if score > best_pair_score[i] {
                best_pair_score[i] = score;
            }
        }
    }

    // Group indices by their union-find root, preserving input order.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..active.len() {
        let root = uf.find(i);
        groups.entry(root).or_default().push(i);
    }

    let mut out: Vec<Cluster> = groups
        .into_values()
        .filter(|members| members.len() >= 2)
        .map(|mut members| {
            members.sort_unstable();
            let protected = members.iter().any(|&i| is_protected(&active[i]));
            let survivor_idx = pick_survivor(&members, active, eff);
            let max_score = members
                .iter()
                .map(|&i| best_pair_score[i])
                .fold(0.0_f64, f64::max);
            Cluster {
                members,
                survivor_idx,
                max_score,
                protected,
            }
        })
        .collect();
    // Deterministic output: by descending max_score, then by first member idx.
    out.sort_by(|a, b| {
        b.max_score
            .partial_cmp(&a.max_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.members.first().cmp(&b.members.first()))
    });
    out
}

/// Pick the member to keep: highest effectiveness factor, tie-break on the
/// longer body (more information retained), final tie-break on lowest index
/// for determinism. Returns an index into `members`.
fn pick_survivor(
    members: &[usize],
    active: &[LoadedMemory],
    eff: Option<&HashMap<String, MemoryEffectiveness>>,
) -> usize {
    members
        .iter()
        .copied()
        .max_by(|&a, &b| {
            let fa = factor_of(&active[a], eff);
            let fb = factor_of(&active[b], eff);
            // Higher factor first; then longer body; then lower index.
            fa.partial_cmp(&fb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| active[a].body.len().cmp(&active[b].body.len()).reverse())
                .then_with(|| b.cmp(&a))
        })
        .unwrap_or_else(|| members[0])
}

fn factor_of(m: &LoadedMemory, eff: Option<&HashMap<String, MemoryEffectiveness>>) -> f64 {
    eff.and_then(|map| map.get(m.id()).map(|e| e.factor()))
        .unwrap_or(1.0)
}

fn is_protected(m: &LoadedMemory) -> bool {
    m.frontmatter.pinned || zones::is_preferences(&m.frontmatter.zone)
}

fn index_of(active: &[LoadedMemory], id: &str) -> Option<usize> {
    active.iter().position(|m| m.id() == id)
}

/// Minimal union-find over `0..n` with path compression + union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // Path compression.
        let mut cur = x;
        while self.parent[cur] != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, body: &str, zone: &str, pinned: bool) -> LoadedMemory {
        let mut fm =
            MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], zone.to_string());
        fm.id = id.to_string();
        fm.pinned = pinned;
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn near_duplicates_form_one_cluster() {
        let active = vec![
            mem(
                "m1",
                "The user prefers vim as their primary editor",
                "general",
                false,
            ),
            mem(
                "m2",
                "User prefers to use vim as the primary editor",
                "general",
                false,
            ),
        ];
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, None);
        assert_eq!(clusters.len(), 1, "got: {clusters:?}");
        let c = &clusters[0];
        assert_eq!(c.members.len(), 2);
        assert!(!c.protected);
        assert!(c.max_score >= DEFAULT_THRESHOLD);
        // Without effectiveness data both factor at 1.0 → tie-break on longer
        // body. m1 and m2 are near-identical length; just assert it picks one
        // of them and the other is superseded.
        let superseded = c.superseded_ids(&active);
        assert_eq!(superseded.len(), 1);
    }

    #[test]
    fn unrelated_memories_yield_no_clusters() {
        let active = vec![
            mem(
                "m1",
                "Always use anyhow for error handling in Rust",
                "general",
                false,
            ),
            mem(
                "m2",
                "The build server is named ci-prod-07",
                "general",
                false,
            ),
            mem("m3", "User likes dark mode in editors", "general", false),
        ];
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, None);
        assert!(clusters.is_empty(), "got: {clusters:?}");
    }

    #[test]
    fn protected_cluster_is_reported_but_flagged() {
        let active = vec![
            mem(
                "m1",
                "The user is a software architect who prefers vim",
                "core",
                false,
            ),
            mem(
                "m2",
                "The user is a software architect that prefers vim",
                "preferences",
                false,
            ),
        ];
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, None);
        // Still detected (so the user can review), but flagged protected.
        assert_eq!(clusters.len(), 1);
        assert!(clusters[0].protected, "core-zone cluster must be protected");
        // Superseded list still computable; the caller decides not to apply.
        assert_eq!(clusters[0].superseded_ids(&active).len(), 1);
    }

    #[test]
    fn pinned_memories_are_protected() {
        let active = vec![
            mem(
                "m1",
                "Deploy via the blue-green pipeline every release",
                "general",
                true,
            ),
            mem(
                "m2",
                "Deploy via the blue-green pipeline for every release",
                "general",
                false,
            ),
        ];
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, None);
        assert_eq!(clusters.len(), 1);
        assert!(
            clusters[0].protected,
            "a pinned member protects the cluster"
        );
    }

    #[test]
    fn effectiveness_picks_the_survivor() {
        // Two near-duplicates. m2 has been referenced more, so it wins.
        let active = vec![
            mem(
                "m1",
                "User prefers vim as the primary editor always",
                "general",
                false,
            ),
            mem(
                "m2",
                "User prefers vim as the primary editor",
                "general",
                false,
            ),
        ];
        let mut eff = HashMap::new();
        eff.insert(
            "m1".to_string(),
            MemoryEffectiveness {
                loaded: 10,
                referenced: 0,
                ..Default::default()
            },
        );
        eff.insert(
            "m2".to_string(),
            MemoryEffectiveness {
                loaded: 10,
                referenced: 10,
                ..Default::default()
            },
        );
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, Some(&eff));
        assert_eq!(clusters.len(), 1);
        let c = &clusters[0];
        assert_eq!(
            active[c.survivor_idx].id(),
            "m2",
            "higher-effectiveness member survives"
        );
        let superseded: Vec<&str> = c.superseded_ids(&active).into_iter().collect();
        assert_eq!(superseded, vec!["m1"]);
    }

    #[test]
    fn transitive_chain_merges_into_one_cluster() {
        // Each adjacent pair clears the threshold; m1 and m3 only weakly
        // overlap but are connected via m2. Union-find merges all three.
        let active = vec![
            mem(
                "m1",
                "the user prefers vim as their primary editor",
                "general",
                false,
            ),
            mem(
                "m2",
                "user prefers vim as their primary editor",
                "general",
                false,
            ),
            mem(
                "m3",
                "user prefers vim as their primary code editor",
                "general",
                false,
            ),
        ];
        let clusters = find_clusters(&active, DEFAULT_THRESHOLD, None);
        assert_eq!(clusters.len(), 1, "got: {clusters:?}");
        assert_eq!(clusters[0].members.len(), 3);
    }

    #[test]
    fn empty_and_single_inputs_return_empty() {
        assert!(find_clusters(&[], DEFAULT_THRESHOLD, None).is_empty());
        assert!(find_clusters(
            &[mem("m1", "solo", "general", false)],
            DEFAULT_THRESHOLD,
            None
        )
        .is_empty());
    }

    #[test]
    fn zero_threshold_is_a_noop_guard() {
        // threshold <= 0 is a guard against "cluster everything" footguns.
        let active = vec![
            mem("m1", "vim", "general", false),
            mem(
                "m2",
                "completely different topic about cooking",
                "general",
                false,
            ),
        ];
        assert!(find_clusters(&active, 0.0, None).is_empty());
    }
}
