//! TF-IDF based memory search.
//!
//! v1: deterministic TF-IDF with cosine similarity. No LLM, no embedding.
//! Cheap enough to re-run on every user turn.
//!
//! Tokenisation mirrors `hermes_skills::relevance` but with a lower
//! `MIN_TOKEN_LEN` (2) to support CJK bigrams where single characters
//! carry meaning.

use std::collections::HashMap;

use crate::memory::LoadedMemory;
use crate::stats::MemoryEffectiveness;

const MIN_TOKEN_LEN: usize = 2;

/// Return the top-`k` memories relevant to `query`, highest score first.
/// An empty `query` or no matches yields an empty Vec.
pub fn search_memories<'a>(
    memories: &'a [LoadedMemory],
    query: &str,
    k: usize,
) -> Vec<&'a LoadedMemory> {
    search_memories_scored(memories, query, k)
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

/// Like `search_memories` but returns (memory, score) pairs.
pub fn search_memories_scored<'a>(
    memories: &'a [LoadedMemory],
    query: &str,
    k: usize,
) -> Vec<(&'a LoadedMemory, f64)> {
    if k == 0 || memories.is_empty() {
        return Vec::new();
    }
    let q_tokens = tokenise(query);
    if q_tokens.is_empty() {
        return Vec::new();
    }

    // Build document frequency map.
    let n = memories.len() as f64;
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut doc_tokens: Vec<Vec<String>> = Vec::with_capacity(memories.len());
    for m in memories {
        let tokens = memory_tokens(m);
        let unique: std::collections::HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();
        for t in &unique {
            *df.entry((*t).to_string()).or_insert(0) += 1;
        }
        doc_tokens.push(tokens);
    }

    // Compute query TF-IDF vector.
    let q_tf = term_freq(&q_tokens);
    let q_vec = tfidf(&q_tf, &df, n);

    // Score each memory by cosine similarity.
    let mut scored: Vec<(f64, &LoadedMemory)> = doc_tokens
        .into_iter()
        .zip(memories.iter())
        .map(|(tokens, m)| {
            let m_tf = term_freq(&tokens);
            let m_vec = tfidf(&m_tf, &df, n);
            let score = cosine_similarity(&q_vec, &m_vec);
            (score, m)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(s, m)| (m, s)).collect()
}

/// Like `search_memories_scored` but multiplies each score by the memory's
/// effectiveness factor before ranking.
pub fn search_memories_with_effectiveness<'a>(
    memories: &'a [LoadedMemory],
    query: &str,
    k: usize,
    effectiveness: Option<&HashMap<String, MemoryEffectiveness>>,
) -> Vec<(&'a LoadedMemory, f64)> {
    let raw = search_memories_scored(memories, query, k.max(memories.len()));
    if effectiveness.is_none() || raw.is_empty() {
        return raw.into_iter().take(k).collect();
    }
    let eff = effectiveness.unwrap();
    let mut adjusted: Vec<(&LoadedMemory, f64)> = raw
        .into_iter()
        .map(|(m, score)| {
            let factor = eff
                .get(&m.frontmatter.id)
                .map(|e| e.factor())
                .unwrap_or(1.0);
            (m, score * factor)
        })
        .collect();
    adjusted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    adjusted.into_iter().take(k).collect()
}

/// Convenience: like `search_memories_with_effectiveness` but returns only
/// the memories, not scores.
pub fn search_memories_effective<'a>(
    memories: &'a [LoadedMemory],
    query: &str,
    k: usize,
    effectiveness: Option<&HashMap<String, MemoryEffectiveness>>,
) -> Vec<&'a LoadedMemory> {
    search_memories_with_effectiveness(memories, query, k, effectiveness)
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

fn tokenise(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let mut tokens = Vec::new();
    for segment in lower.split(|c: char| !c.is_alphanumeric()) {
        let char_count = segment.chars().count();
        if char_count == 0 {
            continue;
        }
        if char_count >= MIN_TOKEN_LEN {
            tokens.push(segment.to_string());
        }
        // For CJK-heavy segments, also emit character bigrams so that
        // single-character tokens (which carry meaning in CJK) contribute
        // to matching.
        if char_count >= 2 && segment.chars().any(is_cjk) {
            let chars: Vec<char> = segment.chars().collect();
            for window in chars.windows(2) {
                tokens.push(window.iter().collect::<String>());
            }
        }
    }
    tokens
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)   // CJK Unified Ideographs
        || (0x3400..=0x4DBF).contains(&cp) // CJK Unified Ideographs Extension A
        || (0x3000..=0x303F).contains(&cp) // CJK Symbols and Punctuation
        || (0x3040..=0x309F).contains(&cp) // Hiragana
        || (0x30A0..=0x30FF).contains(&cp) // Katakana
        || (0xAC00..=0xD7AF).contains(&cp) // Hangul Syllables
}

fn memory_tokens(m: &LoadedMemory) -> Vec<String> {
    let mut tokens = Vec::new();
    tokens.extend(tokenise(&m.body));
    for tag in &m.frontmatter.tags {
        tokens.extend(tokenise(tag));
    }
    tokens
}

fn term_freq(tokens: &[String]) -> HashMap<String, f64> {
    let mut tf: HashMap<String, f64> = HashMap::new();
    let len = tokens.len() as f64;
    for t in tokens {
        *tf.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    for v in tf.values_mut() {
        *v /= len;
    }
    tf
}

fn tfidf(
    tf: &HashMap<String, f64>,
    df: &HashMap<String, usize>,
    n: f64,
) -> HashMap<String, f64> {
    let mut vec = HashMap::new();
    for (term, &freq) in tf {
        let d = df.get(term).copied().unwrap_or(0) as f64;
        if d == 0.0 {
            continue;
        }
        let idf = (n / d).ln() + 1.0; // smoothed IDF
        vec.insert(term.clone(), freq * idf);
    }
    vec
}

fn cosine_similarity(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (k, &va) in a {
        norm_a += va * va;
        if let Some(&vb) = b.get(k) {
            dot += va * vb;
        }
    }
    for &vb in b.values() {
        norm_b += vb * vb;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn memory(id: &str, body: &str, tags: &[&str]) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(Source::User, Confidence::Medium, tags.iter().copied().map(String::from).collect(), "general".to_string());
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    #[test]
    fn ranks_by_relevance() {
        let mems = vec![
            memory("m1", "Always use anyhow for error handling in Rust applications", &["rust", "error"]),
            memory("m2", "Prefer dark theme in all editors and IDEs", &["theme"]),
        ];
        let hits = search_memories(&mems, "rust error handling", 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frontmatter.id, "m1");
    }

    #[test]
    fn empty_query_returns_empty() {
        let mems = vec![memory("m1", "some body", &[])];
        assert!(search_memories(&mems, "", 3).is_empty());
        assert!(search_memories(&mems, "  !! ", 3).is_empty());
    }

    #[test]
    fn no_matches_returns_empty() {
        let mems = vec![memory("m1", "rust error handling", &["rust"])];
        let hits = search_memories(&mems, "javascript debugging", 3);
        assert!(hits.is_empty());
    }

    #[test]
    fn k_zero_returns_empty() {
        let mems = vec![memory("m1", "rust", &["rust"])];
        assert!(search_memories(&mems, "rust", 0).is_empty());
    }

    #[test]
    fn top_k_caps_results() {
        let mems = vec![
            memory("m1", "rust error handling", &["rust"]),
            memory("m2", "rust ownership rules", &["rust"]),
            memory("m3", "rust borrowing explained", &["rust"]),
            memory("m4", "rust lifetime annotations", &["rust"]),
        ];
        let hits = search_memories(&mems, "rust", 2);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn tags_contribute_to_matching() {
        let mems = vec![
            memory("m1", "use anyhow", &["rust", "error"]),
            memory("m2", "use anyhow", &["python"]),
        ];
        let hits = search_memories(&mems, "rust error", 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frontmatter.id, "m1");
    }

    #[test]
    fn cjk_short_tokens_match() {
        // MIN_TOKEN_LEN=2 allows CJK bigrams
        let mems = vec![memory("m1", "记住总是使用anyhow", &["偏好"])];
        let hits = search_memories(&mems, "记住偏好", 3);
        assert!(!hits.is_empty());
    }
}
