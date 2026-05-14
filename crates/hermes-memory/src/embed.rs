//! Local embedding index for semantic memory search.
//!
//! Uses `fastembed` with `AllMiniLML6V2` (384-dim, ~22MB model). Runs on CPU
//! via ONNX runtime — no GPU needed. Feature-gated behind `embed`.

#[cfg(feature = "embed")]
use std::collections::HashMap;

#[cfg(feature = "embed")]
use anyhow::Result;

/// In-memory embedding index: maps memory IDs to their embedding vectors.
/// Supports cosine-similarity search against a query embedding.
#[cfg(feature = "embed")]
pub struct EmbedIndex {
    model: fastembed::TextEmbedding,
    entries: HashMap<String, Vec<f32>>,
}

#[cfg(feature = "embed")]
impl EmbedIndex {
    /// Create a new index with the default model (`AllMiniLML6V2`).
    pub fn new() -> Result<Self> {
        let model = fastembed::TextEmbedding::try_new(fastembed::InitOptions::new(
            fastembed::EmbeddingModel::AllMiniLML6V2,
        ))?;
        Ok(Self {
            model,
            entries: HashMap::new(),
        })
    }

    /// Compute embedding for a single text.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        let docs = vec![text.to_string()];
        let embeddings = self.model.embed(docs, None)?;
        Ok(embeddings
            .into_iter()
            .next()
            .unwrap_or_else(|| vec![0.0; 384]))
    }

    /// Compute embeddings for multiple texts in batch.
    pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let embeddings = self.model.embed(texts.to_vec(), None)?;
        Ok(embeddings)
    }

    /// Insert a memory ID with its pre-computed embedding.
    pub fn insert(&mut self, id: String, embedding: Vec<f32>) {
        self.entries.insert(id, embedding);
    }

    /// Remove a memory from the index.
    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }

    /// Search for the top-`k` memories most similar to `query`.
    /// Returns (id, score) pairs sorted by descending similarity.
    pub fn search(&mut self, query: &str, k: usize) -> Result<Vec<(String, f64)>> {
        if k == 0 || self.entries.is_empty() {
            return Ok(Vec::new());
        }
        let q_emb = self.embed(query)?;
        let mut scored: Vec<(String, f64)> = self
            .entries
            .iter()
            .map(|(id, emb)| {
                let sim = cosine_similarity(&q_emb, emb);
                (id.clone(), sim)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    /// Number of entries in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(feature = "embed")]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (va, vb) in a.iter().zip(b.iter()) {
        let va = *va as f64;
        let vb = *vb as f64;
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(feature = "embed")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_returns_384_dims() {
        let mut idx = EmbedIndex::new().unwrap();
        let emb = idx.embed("hello world").unwrap();
        assert_eq!(emb.len(), 384);
    }

    #[test]
    fn search_finds_similar() {
        let mut idx = EmbedIndex::new().unwrap();
        let emb_rust = idx.embed("Rust error handling with anyhow").unwrap();
        let emb_python = idx.embed("Python type hints and mypy").unwrap();
        idx.insert("mem_rust".into(), emb_rust);
        idx.insert("mem_python".into(), emb_python);

        let hits = idx.search("how to handle errors in Rust", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "mem_rust");
    }

    #[test]
    fn search_empty_index() {
        let mut idx = EmbedIndex::new().unwrap();
        let hits = idx.search("anything", 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn remove_works() {
        let mut idx = EmbedIndex::new().unwrap();
        let emb = idx.embed("test").unwrap();
        idx.insert("m1".into(), emb);
        assert_eq!(idx.len(), 1);
        idx.remove("m1");
        assert!(idx.is_empty());
    }
}
