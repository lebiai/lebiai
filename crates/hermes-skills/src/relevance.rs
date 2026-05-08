//! Skill relevance matcher.
//!
//! v1: deterministic token overlap. No LLM, no embedding. Cheap enough to
//! re-run on every user turn.
//!
//! Scoring rules:
//! - Tokenise the query on non-alphanumeric boundaries, lowercase, drop
//!   tokens shorter than 3 chars.
//! - For each skill build a token bag from `triggers ∪ name ∪ description`,
//!   same normalisation.
//! - Score = number of distinct query tokens that appear in the skill bag.
//! - Tie-break: more triggers > fewer triggers (trigger-rich skills are
//!   more intentional).
//! - Floor: skills with score 0 are dropped.

use std::collections::HashSet;

use crate::skill::LoadedSkill;

const MIN_TOKEN_LEN: usize = 3;

/// Return the top-`k` skills relevant to `query`, highest score first.
/// An empty `query` or no matches yields an empty Vec.
pub fn match_for_query<'a>(
    skills: &'a [LoadedSkill],
    query: &str,
    k: usize,
) -> Vec<&'a LoadedSkill> {
    if k == 0 {
        return Vec::new();
    }
    let q = tokenise(query);
    if q.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(usize, usize, &LoadedSkill)> = skills
        .iter()
        .filter_map(|s| {
            let bag = skill_bag(s);
            let score = q.iter().filter(|t| bag.contains(t.as_str())).count();
            if score == 0 {
                None
            } else {
                Some((score, s.frontmatter.triggers.len(), s))
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
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
}
