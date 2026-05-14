//! Skill relevance matcher.
//!
//! v1: deterministic token overlap. No LLM, no embedding. Cheap enough to
//! re-run on every user turn.
//!
//! v2 (hybrid): combines token overlap (weight 0.4) with optional embedding
//! similarity (weight 0.6). Falls back to pure token overlap when embeddings
//! are unavailable.
//!
//! Scoring rules:
//! - Tokenise the query on non-alphanumeric boundaries, lowercase, drop
//!   tokens shorter than 3 chars.
//! - For each skill build a token bag from `triggers ∪ name ∪ description`,
//!   same normalisation.
//! - Token score = number of distinct query tokens that appear in the skill
//!   bag, multiplied by the skill's effectiveness factor (from usage stats).
//! - Hybrid score = 0.4 * normalised_token_score + 0.6 * embedding_score
//! - Tie-break: more triggers > fewer triggers (trigger-rich skills are
//!   more intentional).
//! - Floor: skills with score 0 are dropped.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::skill::LoadedSkill;
use crate::stats::SkillEffectiveness;

const MIN_TOKEN_LEN: usize = 3;
const TOKEN_WEIGHT: f64 = 0.4;
const EMBED_WEIGHT: f64 = 0.6;

/// Return the top-`k` skills relevant to `query`, highest score first.
/// An empty `query` or no matches yields an empty Vec.
///
/// When `effectiveness` is provided, each skill's token-overlap score is
/// multiplied by its effectiveness factor so that skills with low used/match
/// ratios are deprioritized.
pub fn match_for_query<'a>(
    skills: &'a [LoadedSkill],
    query: &str,
    k: usize,
) -> Vec<&'a LoadedSkill> {
    match_for_query_with_effectiveness(skills, query, k, None)
}

/// Like `match_for_query` but accepts optional per-skill effectiveness data.
pub fn match_for_query_with_effectiveness<'a>(
    skills: &'a [LoadedSkill],
    query: &str,
    k: usize,
    effectiveness: Option<&HashMap<String, SkillEffectiveness>>,
) -> Vec<&'a LoadedSkill> {
    match_for_query_hybrid(skills, query, k, effectiveness, None)
}

/// Hybrid matching: combines token overlap with optional embedding similarity.
///
/// When `embed_scores` is provided, the final score is:
/// `0.4 * normalised_token_score + 0.6 * embedding_score`
///
/// When `embed_scores` is `None`, falls back to pure token overlap.
pub fn match_for_query_hybrid<'a>(
    skills: &'a [LoadedSkill],
    query: &str,
    k: usize,
    effectiveness: Option<&HashMap<String, SkillEffectiveness>>,
    embed_scores: Option<&HashMap<String, f64>>,
) -> Vec<&'a LoadedSkill> {
    if k == 0 {
        return Vec::new();
    }
    let q = tokenise(query);
    if q.is_empty() {
        return Vec::new();
    }

    let has_embed = embed_scores.is_some_and(|m| !m.is_empty());

    let mut scored: Vec<(f64, usize, &LoadedSkill)> = skills
        .iter()
        .filter_map(|s| {
            let bag = skill_bag(s);
            let raw_token = q.iter().filter(|t| bag.contains(t.as_str())).count();
            if raw_token == 0 && !has_embed {
                return None;
            }

            let factor = effectiveness
                .and_then(|e| e.get(&s.frontmatter.name))
                .map(|e| e.factor())
                .unwrap_or(1.0);

            let token_score = raw_token as f64 * factor;

            let score = if has_embed {
                let emb = embed_scores
                    .and_then(|m| m.get(&s.frontmatter.name))
                    .copied()
                    .unwrap_or(0.0);
                // Normalise token score to [0, 1] range (cap at 5 tokens = 1.0)
                let norm_token = (token_score / 5.0).min(1.0);
                TOKEN_WEIGHT * norm_token + EMBED_WEIGHT * emb
            } else {
                token_score
            };

            if score <= 0.0 {
                None
            } else {
                Some((score, s.frontmatter.triggers.len(), s))
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.1.cmp(&a.1))
    });
    scored.into_iter().take(k).map(|(_, _, s)| s).collect()
}

fn tokenise(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= MIN_TOKEN_LEN)
        .map(|t| t.to_string())
        .collect()
}

fn skill_bag(s: &LoadedSkill) -> HashSet<String> {
    let mut bag = HashSet::new();
    for t in &s.frontmatter.triggers {
        bag.extend(tokenise(t));
    }
    bag.extend(tokenise(&s.frontmatter.name));
    bag.extend(tokenise(&s.frontmatter.description));
    bag
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::{LoadedSkill, Scope, SkillFrontmatter};
    use serde_yaml::Mapping;
    use std::path::PathBuf;

    fn skill(name: &str, desc: &str, triggers: &[&str]) -> LoadedSkill {
        LoadedSkill {
            frontmatter: SkillFrontmatter {
                name: name.to_string(),
                description: desc.to_string(),
                triggers: triggers.iter().map(|s| s.to_string()).collect(),
                version: None,
                license: None,
                extra: Mapping::new(),
            },
            body: String::new(),
            source: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn ranks_by_token_overlap() {
        let skills = vec![
            skill("rust-error-handling", "Switch unwrap to anyhow", &["rust", "anyhow", "unwrap"]),
            skill("python-type-hints", "Add type hints to functions", &["python", "typing"]),
        ];
        let hits = match_for_query(&skills, "please refactor my Rust unwrap to use anyhow", 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frontmatter.name, "rust-error-handling");
    }

    #[test]
    fn empty_query_returns_empty() {
        let skills = vec![skill("rust-error-handling", "x", &["rust"])];
        assert!(match_for_query(&skills, "", 3).is_empty());
        assert!(match_for_query(&skills, "  !! ", 3).is_empty());
    }

    #[test]
    fn no_matches_returns_empty() {
        let skills = vec![skill("rust-x", "x", &["rust"])];
        let hits = match_for_query(&skills, "javascript debugging", 3);
        assert!(hits.is_empty());
    }

    #[test]
    fn k_zero_returns_empty() {
        let skills = vec![skill("rust-x", "rust", &["rust"])];
        let hits = match_for_query(&skills, "rust", 0);
        assert!(hits.is_empty());
    }

    #[test]
    fn top_k_caps_results() {
        let skills = vec![
            skill("a", "rust", &["rust"]),
            skill("b", "rust", &["rust"]),
            skill("c", "rust", &["rust"]),
            skill("d", "rust", &["rust"]),
        ];
        let hits = match_for_query(&skills, "rust", 2);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn drops_short_tokens() {
        // "go" is 2 chars, below MIN_TOKEN_LEN=3, so a query of just "go"
        // produces no tokens and matches nothing.
        let skills = vec![skill("go-formatting", "go fmt", &["go"])];
        let hits = match_for_query(&skills, "go", 3);
        assert!(hits.is_empty());
    }

    #[test]
    fn tie_breaks_on_trigger_count() {
        // Both score 1 on "rust"; sk2 has more triggers and should win.
        let sk1 = skill("a", "rust thing", &["rust"]);
        let sk2 = skill("b", "rust", &["rust", "ownership", "borrow"]);
        let skills = vec![sk1, sk2];
        let hits = match_for_query(&skills, "rust", 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frontmatter.name, "b");
    }

    #[test]
    fn hybrid_with_embed_scores() {
        let sk1 = skill("rust-error", "Switch unwrap to anyhow", &["rust", "anyhow"]);
        let sk2 = skill("python-hints", "Add type hints", &["python", "typing"]);
        let skills = vec![sk1, sk2];

        // Embed scores strongly favour python-hints despite token overlap
        // favouring rust-error.
        let mut embed = HashMap::new();
        embed.insert("rust-error".into(), 0.2);
        embed.insert("python-hints".into(), 0.9);

        let hits = match_for_query_hybrid(&skills, "rust unwrap", 2, None, Some(&embed));
        assert_eq!(hits.len(), 2);
        // python-hints should win because 0.6*0.9 >> 0.4*norm_token
        assert_eq!(hits[0].frontmatter.name, "python-hints");
    }

    #[test]
    fn hybrid_without_embed_falls_back_to_token() {
        let sk1 = skill("rust-error", "Switch unwrap to anyhow", &["rust", "anyhow"]);
        let skills = vec![sk1];
        let hits = match_for_query_hybrid(&skills, "rust unwrap", 3, None, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frontmatter.name, "rust-error");
    }

    #[test]
    fn hybrid_empty_embed_map_falls_back() {
        let sk1 = skill("rust-error", "Switch unwrap to anyhow", &["rust"]);
        let skills = vec![sk1];
        let embed = HashMap::new();
        let hits = match_for_query_hybrid(&skills, "rust", 3, None, Some(&embed));
        assert_eq!(hits.len(), 1);
    }
}
