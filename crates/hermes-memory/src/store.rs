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

/// Cosine-similarity threshold for write-time near-duplicate rejection.
/// Aligned with [`crate::distill::DEFAULT_THRESHOLD`]: genuine rewordings of
/// the same fact typically score ~0.55–0.65 under TF-IDF.
pub const DEFAULT_DEDUP_THRESHOLD: f64 = 0.55;

#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("memory id already exists: {0:?}")]
    DuplicateId(String),

    #[error("invalid memory id {0:?}")]
    InvalidId(String),

    #[error(
        "potential conflict with existing memory {existing_id:?} (similarity {similarity:.2})"
    )]
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

    /// Memories not transitively superseded, minus the empty shells that must
    /// never enter a prompt. A `pinned` memory is exempt from that second
    /// filter: pinning is the user saying "this one stays", so a derived
    /// filter must not silently overrule it.
    fn list_active(&self) -> Result<Vec<LoadedMemory>>;

    /// Active and `pinned == true`.
    fn list_pinned(&self) -> Result<Vec<LoadedMemory>>;

    fn get(&self, id: &str) -> Result<Option<LoadedMemory>>;

    /// Persist a new memory.
    ///
    /// Refuses if the id already exists in either scope, and refuses
    /// near-duplicate *bodies* of active memories — **unless** the caller
    /// says `supersedes` (that is an intentional replace/merge).
    ///
    /// 闸门在实现里，不在调用方：任何落盘路径都从这一处过（P1-4）。
    /// [`check_near_duplicate`](Self::check_near_duplicate) 仍然可用，但只作为
    /// 「写之前先问一声」的预检（用于给模型/用户一句更好的话），不是安全边界。
    fn put(&self, scope: Scope, frontmatter: MemoryFrontmatter, body: &str) -> Result<PathBuf>;

    fn delete(&self, scope: Scope, id: &str) -> Result<bool>;

    /// Return the top-`k` active memories most relevant to `query`.
    fn search(&self, query: &str, k: usize) -> Result<Vec<LoadedMemory>>;

    /// Reject bodies that are too similar to an **active** memory
    /// (`Err(Conflict)` when cosine similarity exceeds `threshold`).
    /// Used as a write-time dedup gate; does not modify the store.
    fn check_near_duplicate(&self, body: &str, threshold: f64) -> Result<()>;
}

pub struct FsMemoryStore {
    user_root: PathBuf,
    project_root: Option<PathBuf>,
    /// 写入时的查重阈值。**闸门长在 `put` 上**，所以阈值必须跟着 store 走，
    /// 而不是跟着某一个调用方走 —— 否则「谁忘了检查」就又是一条绕过路径（P1-4）。
    /// `<= 0.0` = 显式关掉查重（测试与「用户坚持要记」的场景）。
    dedup_threshold: f64,
    #[cfg(feature = "embed")]
    embed_index: Option<std::sync::Mutex<crate::embed::EmbedIndex>>,
}

impl Clone for FsMemoryStore {
    /// Clone paths only. Embedding index is not shared (callers that need
    /// embeddings re-enable on the new instance).
    fn clone(&self) -> Self {
        Self {
            user_root: self.user_root.clone(),
            project_root: self.project_root.clone(),
            dedup_threshold: self.dedup_threshold,
            #[cfg(feature = "embed")]
            embed_index: None,
        }
    }
}

impl FsMemoryStore {
    pub fn new(user_root: PathBuf, project_root: Option<PathBuf>) -> Self {
        Self {
            user_root,
            project_root,
            dedup_threshold: DEFAULT_DEDUP_THRESHOLD,
            #[cfg(feature = "embed")]
            embed_index: None,
        }
    }

    /// 改写入查重阈值。`<= 0.0` 显式关掉查重（只有明确的调用方才该这么做）。
    pub fn with_dedup_threshold(mut self, threshold: f64) -> Self {
        self.dedup_threshold = threshold;
        self
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

    /// Embedding-based conflict check when the embed index is enabled.
    /// Returns `Ok(())` without checking if embeddings are not active —
    /// prefer [`MemoryStore::check_near_duplicate`] which always falls back
    /// to TF-IDF.
    #[cfg(feature = "embed")]
    pub fn check_conflict(&self, body: &str, threshold: f64) -> Result<()> {
        if let Some(ref index) = self.embed_index {
            let mut idx = index.lock().unwrap();
            let hits = idx
                .search(body, 1)
                .map_err(|e| MemoryStoreError::Config(format!("embedding search failed: {e}")))?;
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

    /// TF-IDF based conflict check against active memories.
    pub fn check_conflict_tfidf(&self, body: &str, threshold: f64) -> Result<()> {
        if threshold <= 0.0 || body.trim().is_empty() {
            return Ok(());
        }
        let active = self.list_active()?;
        if active.is_empty() {
            return Ok(());
        }
        // Single scored query is enough (top-1).
        if let Some((m, score)) = crate::relevance::search_memories_scored(&active, body, 1)
            .into_iter()
            .next()
        {
            if score > threshold {
                return Err(MemoryStoreError::Conflict {
                    existing_id: m.id().to_string(),
                    similarity: score,
                });
            }
        }
        Ok(())
    }

    fn near_duplicate_impl(&self, body: &str, threshold: f64) -> Result<()> {
        // 首行判据先跑：它是**精确**的（同一件事的再次誊写），余弦只是兜底。
        // 余弦会被长度稀释——三条「新闻选材标准」同日入库（2026-09-18 实测），
        // 每条 1890–3668 字、措辞不同，TF-IDF 全落在阈值以下，于是同一套标准存了三份。
        // `threshold <= 0.0` 是调用方**显式关掉查重**的信号，照旧尊重。
        if threshold > 0.0 {
            if let Some(existing_id) = self.same_leading_line(body)? {
                return Err(MemoryStoreError::Conflict {
                    existing_id,
                    similarity: 1.0,
                });
            }
        }
        #[cfg(feature = "embed")]
        if self.embed_index.is_some() {
            match self.check_conflict(body, threshold) {
                // Embed path found a hit or ran cleanly.
                Ok(()) => return Ok(()),
                Err(e @ MemoryStoreError::Conflict { .. }) => return Err(e),
                // Search/config failure → fall through to TF-IDF.
                Err(e) => {
                    tracing::warn!(error=%e, "embed near-duplicate check failed; using TF-IDF");
                }
            }
        }
        self.check_conflict_tfidf(body, threshold)
    }

    /// 同一「首行」= 同一件事的又一次誊写，返回已在库的那条 id。
    ///
    /// 首行是这条记忆的**标题**；标题相同就该合并（用 `supersedes`），而不是再存一份。
    /// 判据只此一处——工具写入、反思自动入库、CLI 都走同一个 `check_near_duplicate`。
    fn same_leading_line(&self, body: &str) -> Result<Option<String>> {
        let Some(lead) = leading_line(body) else {
            return Ok(None);
        };
        for m in self.list_active()? {
            if leading_line(&m.body).as_deref() == Some(lead.as_str()) {
                return Ok(Some(m.id().to_string()));
            }
        }
        Ok(None)
    }

    /// `~/.lebi-ai/memories` + (optional) `./.lebi-ai/memories`.
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
        for entry in WalkDir::new(root)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
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
        let superseded_owned: HashSet<String> = superseded.iter().map(|s| s.to_string()).collect();
        Ok(all
            .into_iter()
            .filter(|m| !superseded_owned.contains(m.id()))
            .filter(|m| m.frontmatter.pinned || !crate::slot::is_worthless_for_living(&m.body))
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
        // 查重闸门就长在落盘这一处：inbox 批准、GUI/server 反思、CLI 反思、
        // memory_save 工具、micro 自动入库 —— 全都从这里过，谁也绕不过去（P1-4）。
        // 两种「有意写入」放行：带 `supersedes`（我知道我在替换谁），或调用方
        // 显式声明了 `intentional`（视图把看不见的 supersedes id 丢掉之后，
        // 意图不该跟着一起消失）。那正是**该**写下去的时候。
        if frontmatter.supersedes.is_empty() && !frontmatter.intentional {
            self.near_duplicate_impl(body, self.dedup_threshold)?;
        }
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
        for entry in WalkDir::new(root)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
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

    fn check_near_duplicate(&self, body: &str, threshold: f64) -> Result<()> {
        self.near_duplicate_impl(body, threshold)
    }
}

/// 记忆的「标题」= 正文第一行非空内容，折叠空白后**至少 8 个字**才算数。
///
/// 太短的一行（`好的`、`注意`）不足以判定「同一件事」，那些交给余弦那条判据；
/// 返回 `None` = 这条记忆没有可比对的标题。
fn leading_line(body: &str) -> Option<String> {
    let line = body.lines().map(str::trim).find(|l| !l.is_empty())?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    (collapsed.chars().count() >= 8).then_some(collapsed)
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
    Ok(hermes_core::data_path("memories"))
}

pub fn standard_project_root() -> Option<PathBuf> {
    let p = PathBuf::from(hermes_core::project_data_dirname()).join("memories");
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
        let path = store
            .put(Scope::User, m1, "use anyhow not thiserror at app layer\n")
            .unwrap();
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
        store
            .put(Scope::User, m_old, "use thiserror everywhere\n")
            .unwrap();

        let mut m_new = fm(vec!["rust"]);
        m_new.supersedes = vec![id_old.clone()];
        let id_new = m_new.id.clone();
        store
            .put(
                Scope::User,
                m_new,
                "use anyhow at app layer, thiserror in libs\n",
            )
            .unwrap();

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
        let scopes: std::collections::HashSet<Scope> = listed.iter().map(|m| m.scope).collect();
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

    #[test]
    fn check_near_duplicate_rejects_reworded_active_memory() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        let m = fm(vec!["editor"]);
        store
            .put(
                Scope::User,
                m,
                "The user prefers vim as their primary editor",
            )
            .unwrap();

        let err = store
            .check_near_duplicate(
                "User prefers vim as the primary editor",
                DEFAULT_DEDUP_THRESHOLD,
            )
            .unwrap_err();
        assert!(
            matches!(err, MemoryStoreError::Conflict { similarity, .. } if similarity > DEFAULT_DEDUP_THRESHOLD),
            "got {err:?}"
        );
    }

    #[test]
    fn check_near_duplicate_allows_unrelated_body() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        store
            .put(
                Scope::User,
                fm(vec![]),
                "The user prefers vim as their primary editor",
            )
            .unwrap();
        store
            .check_near_duplicate(
                "The build server is named ci-prod-07",
                DEFAULT_DEDUP_THRESHOLD,
            )
            .unwrap();
    }

    #[test]
    fn check_near_duplicate_ignores_superseded_memories() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);

        let old = fm(vec![]);
        let old_id = old.id.clone();
        store
            .put(
                Scope::User,
                old,
                "The user prefers vim as their primary editor",
            )
            .unwrap();

        // Replace with an unrelated active fact so the old body is only on disk
        // as a superseded audit record.
        let mut newer = fm(vec![]);
        newer.supersedes = vec![old_id];
        store
            .put(Scope::User, newer, "The build server is named ci-prod-07")
            .unwrap();

        // Same wording as the superseded file must not trip the gate.
        store
            .check_near_duplicate(
                "The user prefers vim as their primary editor",
                DEFAULT_DEDUP_THRESHOLD,
            )
            .unwrap();
    }

    /// 2026-09-18 实测：同一套「新闻选材标准」在同一天里被存了三份（1890 / 3099 /
    /// 3668 字，措辞不同），余弦被长度稀释、三条全部通过旧闸门。首行相同即同一件事。
    #[test]
    fn check_near_duplicate_rejects_the_same_leading_line_even_when_text_differs() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        let head = "新闻选材标准（2026-09-17 用户明确「这是我选择新闻的标准」）";
        store
            .put(
                Scope::User,
                fm(vec!["standard"]),
                &format!("{head}\n一、市值低于 30 亿的不看。\n"),
            )
            .unwrap();

        let restated = format!(
            "{head}\n换成完全不同的说法，把同一套标准逐条重述一遍，\n\
             并且补上更多前后文，让字数与用词都不一样，\n\
             好让余弦相似度被稀释到阈值以下。\n"
        );
        let err = store
            .check_near_duplicate(&restated, DEFAULT_DEDUP_THRESHOLD)
            .unwrap_err();
        assert!(
            matches!(err, MemoryStoreError::Conflict { similarity, .. } if similarity >= 1.0),
            "同首行必须被拦下，got {err:?}"
        );
    }

    #[test]
    fn check_near_duplicate_allows_different_leading_lines() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        store
            .put(
                Scope::User,
                fm(vec![]),
                "新闻选材标准（2026-09-17 用户明确）：市值低于 30 亿的不看。\n",
            )
            .unwrap();
        store
            .check_near_duplicate(
                "视频口播口径（2026-09-15 用户明确）：两分钟，先结论后依据。\n",
                DEFAULT_DEDUP_THRESHOLD,
            )
            .unwrap();
    }

    #[test]
    fn a_short_first_line_is_not_a_title() {
        assert_eq!(leading_line(""), None);
        assert_eq!(leading_line("   \n\n"), None);
        assert_eq!(leading_line("好的\n下面才是正文"), None);
        assert_eq!(
            leading_line("  新闻选材标准（2026-09-17）  \n正文"),
            Some("新闻选材标准（2026-09-17）".to_string())
        );
    }

    /// P1-4 的核心断言：闸门长在 `put` 上，**任何**落盘路径都绕不过去。
    #[test]
    fn put_itself_refuses_a_near_duplicate() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        store
            .put(
                Scope::User,
                fm(vec![]),
                "The user prefers vim as their primary editor\n",
            )
            .unwrap();
        // 同一个意思换一种说法 —— 以前只有 memory_save / micro 两条路会拦，
        // inbox 批准与 GUI/server 反思直接落盘就漏过去了。
        let err = store
            .put(
                Scope::User,
                fm(vec![]),
                "User prefers vim as the primary editor\n",
            )
            .unwrap_err();
        assert!(
            matches!(err, MemoryStoreError::Conflict { .. }),
            "expected Conflict, got {err:?}"
        );
        assert_eq!(store.list_active().unwrap().len(), 1);
    }

    /// `supersedes` 是「我知道我在替换谁」的显式意图（改稿 / 蒸馏合并 / 解冲突），
    /// 有它就该写得下去。
    #[test]
    fn put_with_supersedes_writes_even_when_the_body_repeats() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        store
            .put(
                Scope::User,
                fm(vec![]),
                "The user prefers vim as their primary editor\n",
            )
            .unwrap();
        let old_id = store.list_active().unwrap()[0].id().to_string();

        let mut replacing = fm(vec![]);
        replacing.supersedes = vec![old_id];
        store
            .put(
                Scope::User,
                replacing,
                "The user prefers vim as their primary editor\n",
            )
            .expect("supersedes 必须能写下去");

        // 旧的那条被标成 superseded，所以「在册」只有新的一条，盘上共两条。
        assert_eq!(store.list_active().unwrap().len(), 1);
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn a_zero_threshold_still_means_no_dedup_at_all() {
        let user = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(user.path().to_path_buf(), None);
        let head = "新闻选材标准（2026-09-17 用户明确「这是我选择新闻的标准」）";
        store
            .put(Scope::User, fm(vec![]), &format!("{head}\n第一版\n"))
            .unwrap();
        // 显式关掉查重的调用方（threshold = 0）不该被首行判据拦住。
        store
            .check_near_duplicate(&format!("{head}\n第二版\n"), 0.0)
            .unwrap();
    }
}
