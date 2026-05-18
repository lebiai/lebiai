//! Filesystem-backed memory store.
//!
//! Layout under each scope root (flat directory of markdown files):
//! ```text
//! <root>/
//!   2026-05-05-mem_<short>.md
//!   2026-05-05-mem_<short>.md
//!   ...
//! ```
//!
//! Filename convention is `<YYYY-MM-DD>-<id-prefix>.md` for human browsing
//! ordering; the authoritative id lives in YAML frontmatter, so reading
//! tolerates any filename ending in `.md`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use thiserror::Error;
use walkdir::WalkDir;

use crate::memory::{LoadedMemory, MemoryFrontmatter, Scope};
use hermes_store::{FrontmatterDoc, FrontmatterError};

#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("memory id already exists: {0:?}")]
    DuplicateId(String),

    #[error("invalid memory id {0:?}")]
    InvalidId(String),

    #[error("potential conflict with existing memory {existing_id:?} (similarity {similarity:.2})")]
    Conflict {
        existing_id: String,
        similarity: f64,
    },

    #[error(transparent)]
    Frontmatter(#[from] FrontmatterError),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, MemoryStoreError>;

pub trait MemoryStore: Send + Sync {
    /// Every memory on disk, both scopes, in id order. Includes superseded
    /// records for audit / curation flows.
    fn list(&self) -> Result<Vec<LoadedMemory>>;

    /// Memories not transitively superseded.
    fn list_active(&self) -> Result<Vec<LoadedMemory>>;

    /// Active and `pinned == true`.
    fn list_pinned(&self) -> Result<Vec<LoadedMemory>>;

    fn get(&self, id: &str) -> Result<Option<LoadedMemory>>;

    /// Persist a new memory. Refuses if the id already exists in either
    /// scope.
    fn put(&self, scope: Scope, frontmatter: MemoryFrontmatter, body: &str) -> Result<PathBuf>;

    fn delete(&self, scope: Scope, id: &str) -> Result<bool>;

    /// Return the top-`k` active memories most relevant to `query`.
    fn search(&self, query: &str, k: usize) -> Result<Vec<LoadedMemory>>;
}

pub struct FsMemoryStore {
    user_root: PathBuf,
    project_root: Option<PathBuf>,
    #[cfg(feature = "embed")]
    embed_index: Option<std::sync::Mutex<crate::embed::EmbedIndex>>,
}

impl FsMemoryStore {
    pub fn new(user_root: PathBuf, project_root: Option<PathBuf>) -> Self {
        Self {
            user_root,
            project_root,
            #[cfg(feature = "embed")]
            embed_index: None,
        }
    }

    /// Enable semantic search via local embeddings. Must be called before
    /// any `put()` or `search()` calls for embeddings to take effect.
    #[cfg(feature = "embed")]
    pub fn enable_embeddings(&mut self) -> anyhow::Result<()> {
        let mut index = crate::embed::EmbedIndex::new()?;
        // Index all existing active memories.
        let active = self.list_active()?;
        if !active.is_empty() {
            let texts: Vec<String> = active.iter().map(|m| m.body.clone()).collect();
            let ids: Vec<String> = active.iter().map(|m| m.id().to_string()).collect();
            let embeddings = index.embed_batch(&texts)?;
            for (id, emb) in ids.into_iter().zip(embeddings) {
                index.insert(id, emb);
            }
        }
        self.embed_index = Some(std::sync::Mutex::new(index));
        Ok(())
    }

    /// Compute and store the embedding for a memory. Call after `put()`.
    #[cfg(feature = "embed")]
    pub fn index_memory(&self, id: &str, body: &str) -> anyhow::Result<()> {
        if let Some(ref index) = self.embed_index {
            let mut idx = index.lock().unwrap();
            let emb = idx.embed(body)?;
            idx.insert(id.to_string(), emb);
        }
        Ok(())
    }

    /// Remove a memory from the embedding index. Call after `delete()`.
    #[cfg(feature = "embed")]
    pub fn remove_from_index(&self, id: &str) {
        if let Some(ref index) = self.embed_index {
            let mut idx = index.lock().unwrap();
            idx.remove(id);
        }
    }

    /// Check if a new memory body conflicts with an existing one.
    /// Returns `Err(MemoryStoreError::Conflict)` if similarity > threshold.
    /// When embeddings are unavailable, falls back to TF-IDF similarity.
    #[cfg(feature = "embed")]
    pub fn check_conflict(&self, body: &str, threshold: f64) -> Result<()> {
        if let Some(ref index) = self.embed_index {
            let mut idx = index.lock().unwrap();
            let hits = idx.search(body, 1).map_err(|e| {
                MemoryStoreError::Config(format!("embedding search failed: {e}"))
            })?;
            if let Some((id, sim)) = hits.first() {
                if *sim > threshold {
                    return Err(MemoryStoreError::Conflict {
                        existing_id: id.clone(),
                        similarity: *sim,
                    });
                }
            }
        }
        Ok(())
    }

    /// TF-IDF based conflict check (used when embeddings are unavailable).
    pub fn check_conflict_tfidf(&self, body: &str, threshold: f64) -> Result<()> {
        let active = self.list_active()?;
        let refs = crate::relevance::search_memories(&active, body, 1);
        if let Some(m) = refs.first() {
            // Re-run TF-IDF to get the actual score.
            let score = crate::relevance::search_memories_scored(&active, body, 1)
                .first()
                .map(|(_, s)| *s)
                .unwrap_or(0.0);
            if score > threshold {
                return Err(MemoryStoreError::Conflict {
                    existing_id: m.id().to_string(),
                    similarity: score,
                });
            }
        }
        Ok(())
    }

    /// `~/.small-rust-hermes/memories` + (optional) `./.small-rust-hermes/memories`.
    pub fn standard() -> Result<Self> {
        let user = standard_user_root()?;
        let project = standard_project_root();
        Ok(Self::new(user, project))
    }

    fn root_for(&self, scope: Scope) -> Option<&Path> {
        match scope {
            Scope::User => Some(&self.user_root),
            Scope::Project => self.project_root.as_deref(),
        }
    }

    fn list_scope(root: &Path, scope: Scope) -> Result<Vec<LoadedMemory>> {
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in WalkDir::new(root).max_depth(1).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            match Self::load_one(path, scope) {
                Ok(m) => out.push(m),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping malformed memory");
                }
            }
        }
        Ok(out)
    }

    fn load_one(path: &Path, scope: Scope) -> Result<LoadedMemory> {
        let doc: FrontmatterDoc<MemoryFrontmatter> = hermes_store::read_doc(path)?;
        Ok(LoadedMemory {
            frontmatter: doc.frontmatter,
            body: doc.body,
            source_path: path.to_path_buf(),
            scope,
        })
    }

    fn filename_for(fm: &MemoryFrontmatter) -> String {
        let date = fm.created.format("%Y-%m-%d");
        let short = id_prefix(&fm.id, 12);
        format!("{date}-{short}.md")
    }
}

impl MemoryStore for FsMemoryStore {
    fn list(&self) -> Result<Vec<LoadedMemory>> {
        let mut out = Self::list_scope(&self.user_root, Scope::User)?;
        if let Some(root) = &self.project_root {
            out.extend(Self::list_scope(root, Scope::Project)?);
        }
        out.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(out)
    }

    fn list_active(&self) -> Result<Vec<LoadedMemory>> {
        let all = self.list()?;
        let mut superseded: HashSet<&str> = HashSet::new();
        for m in &all {
            for old in &m.frontmatter.supersedes {
                superseded.insert(old.as_str());
            }
        }
        let superseded_owned: HashSet<String> =
            superseded.iter().map(|s| s.to_string()).collect();
        Ok(all
            .into_iter()
            .filter(|m| !superseded_owned.contains(m.id()))
            .collect())
    }

    fn list_pinned(&self) -> Result<Vec<LoadedMemory>> {
        Ok(self
            .list_active()?
            .into_iter()
            .filter(|m| m.frontmatter.pinned)
            .collect())
    }

    fn get(&self, id: &str) -> Result<Option<LoadedMemory>> {
        for m in self.list()? {
            if m.id() == id {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }

    fn put(&self, scope: Scope, frontmatter: MemoryFrontmatter, body: &str) -> Result<PathBuf> {
        validate_id(&frontmatter.id)?;
        // Refuse if the id already exists anywhere on disk.
        if self.get(&frontmatter.id)?.is_some() {
            return Err(MemoryStoreError::DuplicateId(frontmatter.id.clone()));
        }
        let root = self.root_for(scope).ok_or_else(|| {
            MemoryStoreError::Config(
                "project scope requested but no project root configured".into(),
            )
        })?;
        let path = root.join(Self::filename_for(&frontmatter));
        let doc = FrontmatterDoc {
            frontmatter,
            body: body.to_string(),
        };
        hermes_store::write_doc_atomic(&path, &doc)?;
        // Note: embedding computation for the new memory must be done by the
        // caller after put() returns, since put() is sync and embedding is
        // async. Use FsMemoryStore::index_memory() for this.
        Ok(path)
    }

    fn delete(&self, scope: Scope, id: &str) -> Result<bool> {
        validate_id(id)?;
        let root = self.root_for(scope).ok_or_else(|| {
            MemoryStoreError::Config(
                "project scope requested but no project root configured".into(),
            )
        })?;
        // Walk the scope directory once and delete by frontmatter-id match.
        for entry in WalkDir::new(root).max_depth(1).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Ok(m) = Self::load_one(path, scope) {
                if m.id() == id {
                    std::fs::remove_file(path).map_err(|source| MemoryStoreError::Io {
                        path: path.to_path_buf(),
                        source,
                    })?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn search(&self, query: &str, k: usize) -> Result<Vec<LoadedMemory>> {
        let active = self.list_active()?;
        #[cfg(feature = "embed")]
        if let Some(ref index) = self.embed_index {
            let mut idx = index.lock().unwrap();
            match idx.search(query, k) {
                Ok(scored) => {
                    let id_set: std::collections::HashSet<String> =
                        scored.into_iter().map(|(id, _)| id).collect();
                    let results: Vec<LoadedMemory> = active
                        .into_iter()
                        .filter(|m| id_set.contains(m.id()))
                        .collect();
                    return Ok(results);
                }
                Err(e) => {
                    tracing::warn!(error=%e, "semantic search failed, falling back to TF-IDF");
                    // Fall through to TF-IDF.
                }
            }
        }
        let refs = crate::relevance::search_memories(&active, query, k);
        Ok(refs.into_iter().cloned().collect())
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(MemoryStoreError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn id_prefix(id: &str, n: usize) -> &str {
    &id[..n.min(id.len())]
}

pub fn standard_user_root() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| MemoryStoreError::Config("could not resolve $HOME".into()))?;
    Ok(home.join(".small-rust-hermes").join("memories"))
}

pub fn standard_project_root() -> Option<PathBuf> {
    let p = PathBuf::from(".small-rust-hermes").join("memories");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, Source};

    fn fm(tags: Vec<&str>) -> MemoryFrontmatter {
        MemoryFrontmatter::new(
            Source::User,
            Confidence::Medium,
            tags.into_iter().map(String::from).collect(),
            "general".to_string(),
        )
    }

    #[test]
    fn put_get_list_delete_user_scope() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let m1 = fm(vec!["rust", "convention"]);
        let id1 = m1.id.clone();
        let path = store.put(Scope::User, m1, "use anyhow not thiserror at app layer\n").unwrap();
        assert!(path.to_string_lossy().ends_with(".md"));

        let loaded = store.get(&id1).unwrap().unwrap();
        assert_eq!(loaded.id(), id1);
        assert!(loaded.body.contains("anyhow"));
        assert_eq!(loaded.scope, Scope::User);

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);

        assert!(store.delete(Scope::User, &id1).unwrap());
        assert!(store.get(&id1).unwrap().is_none());
        assert!(!store.delete(Scope::User, &id1).unwrap());
    }

    #[test]
    fn duplicate_id_rejected() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let m1 = fm(vec!["x"]);
        let mut m2 = fm(vec!["y"]);
        m2.id = m1.id.clone();

        store.put(Scope::User, m1, "body 1").unwrap();
        let err = store.put(Scope::User, m2, "body 2").unwrap_err();
        assert!(matches!(err, MemoryStoreError::DuplicateId(_)));
    }

    #[test]
    fn supersedes_filters_active() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let m_old = fm(vec!["rust"]);
        let id_old = m_old.id.clone();
        store.put(Scope::User, m_old, "use thiserror everywhere\n").unwrap();

        let mut m_new = fm(vec!["rust"]);
        m_new.supersedes = vec![id_old.clone()];
        let id_new = m_new.id.clone();
        store.put(Scope::User, m_new, "use anyhow at app layer, thiserror in libs\n").unwrap();

        let active = store.list_active().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id(), id_new);

        let all = store.list().unwrap();
        assert_eq!(all.len(), 2, "list() must include superseded for audit");
    }

    #[test]
    fn list_pinned_returns_only_pinned_active() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let m_a = fm(vec!["a"]);
        let mut m_b = fm(vec!["b"]);
        m_b.pinned = true;
        let id_b = m_b.id.clone();

        store.put(Scope::User, m_a, "ephemeral").unwrap();
        store.put(Scope::User, m_b, "always loaded").unwrap();

        let pinned = store.list_pinned().unwrap();
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].id(), id_b);
    }

    #[test]
    fn project_scope_overrides_default_path() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(
            user.path().to_path_buf(),
            Some(project.path().to_path_buf()),
        );

        let m_u = fm(vec!["user"]);
        let id_u = m_u.id.clone();
        let m_p = fm(vec!["project"]);
        let id_p = m_p.id.clone();
        store.put(Scope::User, m_u, "u body").unwrap();
        store.put(Scope::Project, m_p, "p body").unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 2);
        let scopes: std::collections::HashSet<Scope> =
            listed.iter().map(|m| m.scope).collect();
        assert!(scopes.contains(&Scope::User));
        assert!(scopes.contains(&Scope::Project));

        // Round-trip lookup.
        assert_eq!(store.get(&id_u).unwrap().unwrap().scope, Scope::User);
        assert_eq!(store.get(&id_p).unwrap().unwrap().scope, Scope::Project);
    }

    #[test]
    fn invalid_id_rejected() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let mut bad = fm(vec![]);
        bad.id = "../etc".into();
        let err = store.put(Scope::User, bad, "x").unwrap_err();
        assert!(matches!(err, MemoryStoreError::InvalidId(_)));
    }

    #[test]
    fn preserves_unknown_frontmatter_keys() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let mut m = fm(vec!["x"]);
        m.extra.insert(
            serde_yaml::Value::String("custom_field".into()),
            serde_yaml::Value::String("preserved".into()),
        );
        let id = m.id.clone();
        store.put(Scope::User, m, "body").unwrap();

        let loaded = store.get(&id).unwrap().unwrap();
        assert_eq!(
            loaded
                .frontmatter
                .extra
                .get(serde_yaml::Value::String("custom_field".into())),
            Some(&serde_yaml::Value::String("preserved".into()))
        );
    }

    #[test]
    fn empty_root_returns_empty_list() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().join("does-not-exist"), None);
        assert!(store.list().unwrap().is_empty());
        assert!(store.list_active().unwrap().is_empty());
        assert!(store.list_pinned().unwrap().is_empty());
    }
}
