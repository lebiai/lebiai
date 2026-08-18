//! Filesystem-backed materials + catalog. Index lives in RAM after load.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::index::{
    rebuild_postings, search_with_postings, split_chunks, Chunk, Posting, MAX_HITS,
};
use crate::tokenize::overlap;

/// Word / PDF — auto-keep when dropped into dialogue.
pub fn is_auto_keep_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "pdf" | "doc" | "docx")
}

const NEAR_VERSION: f64 = 0.72;
const MIN_SEARCHABLE_CHARS: usize = 16;
const MAX_SOURCES: usize = 200;
const MAX_BODY_CHARS: usize = 2_000_000;
const CATALOG_NAME: &str = "catalog.json";

#[derive(Debug, Error)]
pub enum SourceStoreError {
    #[error("material not found")]
    NotFound,
    #[error("too many materials (max {MAX_SOURCES})")]
    Quota,
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, SourceStoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readable {
    Ok,
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMeta {
    pub id: String,
    pub title: String,
    pub original_name: String,
    pub ext: String,
    pub hash: String,
    pub created_at: DateTime<Utc>,
    pub readable: Readable,
    #[serde(default)]
    pub superseded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    pub chars: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: String,
    pub title: String,
    pub original_name: String,
    pub ext: String,
    pub created_at: String,
    pub readable: bool,
    pub chars: usize,
    pub superseded: bool,
    #[serde(default)]
    pub original_missing: bool,
}

impl From<&SourceMeta> for SourceItem {
    fn from(m: &SourceMeta) -> Self {
        Self {
            id: m.id.clone(),
            title: m.title.clone(),
            original_name: m.original_name.clone(),
            ext: m.ext.clone(),
            created_at: m.created_at.to_rfc3339(),
            readable: m.readable == Readable::Ok,
            chars: m.chars,
            superseded: m.superseded,
            original_missing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHit {
    pub id: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestOutcome {
    pub item: SourceItem,
    /// created | duplicate | new_version
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Catalog {
    version: u32,
    sources: Vec<SourceMeta>,
    chunks: Vec<Chunk>,
    #[serde(default)]
    postings: HashMap<String, Vec<Posting>>,
}

struct Inner {
    catalog: Catalog,
}

pub struct SourceStore {
    root: Mutex<PathBuf>,
    inner: Mutex<Inner>,
    /// None = any active file; Some = only these ids/titles this turn.
    read_allow: Mutex<Option<HashSet<String>>>,
}

impl SourceStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let mut catalog = load_catalog(&root)?;
        if catalog.postings.is_empty() && !catalog.chunks.is_empty() {
            catalog.postings = rebuild_postings(&catalog.chunks);
        }
        Ok(Self {
            root: Mutex::new(root),
            inner: Mutex::new(Inner { catalog }),
            read_allow: Mutex::new(None),
        })
    }

    fn root_path(&self) -> PathBuf {
        self.root.lock().expect("sources root").clone()
    }

    /// After Settings moves the data folder, point at the new copy.
    pub fn replace_root(&self, root: PathBuf) -> Result<()> {
        fs::create_dir_all(&root)?;
        let mut catalog = load_catalog(&root)?;
        if catalog.postings.is_empty() && !catalog.chunks.is_empty() {
            catalog.postings = rebuild_postings(&catalog.chunks);
        }
        *self.root.lock().expect("sources root") = root;
        *self.inner.lock().expect("sources lock") = Inner { catalog };
        *self.read_allow.lock().expect("sources allow") = None;
        Ok(())
    }

    /// Restrict `source_read` to ids/titles the engine already showed this turn.
    /// `None` = any kept file (user asked what is on hand).
    pub fn set_read_allowlist(&self, ids: Option<Vec<String>>) {
        *self.read_allow.lock().expect("sources allow") =
            ids.map(|v| v.into_iter().filter(|s| !s.is_empty()).collect());
    }

    pub fn standard() -> Result<Self> {
        Self::open(hermes_core::data_path("sources"))
    }

    pub fn list_active(&self) -> Vec<SourceItem> {
        self.list_active_with_prev()
            .into_iter()
            .map(|(item, _)| item)
            .collect()
    }

    /// Active materials plus the immediate previous version (if any).
    pub fn list_active_with_prev(&self) -> Vec<(SourceItem, Option<SourceItem>)> {
        let g = self.inner.lock().expect("sources lock");
        let mut rows: Vec<(SourceItem, Option<SourceItem>)> = g
            .catalog
            .sources
            .iter()
            .filter(|s| !s.superseded)
            .map(|s| {
                let prev = s
                    .supersedes
                    .as_ref()
                    .and_then(|pid| g.catalog.sources.iter().find(|p| p.id == *pid))
                    .map(SourceItem::from);
                let mut item = SourceItem::from(s);
                let orig = self
                    .root_path()
                    .join(&s.id)
                    .join(format!("original.{}", s.ext));
                item.original_missing = !orig.is_file();
                (item, prev)
            })
            .collect();
        rows.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));
        rows
    }

    /// Title match or body retrieval. Empty query = all active.
    pub fn list_matching(&self, query: &str) -> Vec<(SourceItem, Option<SourceItem>)> {
        let all = self.list_active_with_prev();
        let q = query.trim();
        if q.is_empty() {
            return all;
        }
        let hit_ids: std::collections::HashSet<String> =
            self.search(q, &[]).into_iter().map(|h| h.id).collect();
        let qlow = q.to_lowercase();
        all.into_iter()
            .filter(|(item, prev)| {
                hit_ids.contains(&item.id)
                    || item.title.to_lowercase().contains(&qlow)
                    || item.original_name.to_lowercase().contains(&qlow)
                    || prev
                        .as_ref()
                        .is_some_and(|p| p.title.to_lowercase().contains(&qlow))
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<SourceMeta> {
        let g = self.inner.lock().expect("sources lock");
        g.catalog.sources.iter().find(|s| s.id == id).cloned()
    }

    pub fn original_path(&self, id: &str) -> Option<PathBuf> {
        let meta = self.get(id)?;
        let p = self
            .root_path()
            .join(id)
            .join(format!("original.{}", meta.ext));
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }

    pub fn search(&self, query: &str, focus: &[String]) -> Vec<SourceHit> {
        let g = self.inner.lock().expect("sources lock");
        let active: std::collections::HashSet<&str> = g
            .catalog
            .sources
            .iter()
            .filter(|s| !s.superseded && s.readable == Readable::Ok)
            .map(|s| s.id.as_str())
            .collect();
        let post = &g.catalog.postings;
        let q_tokens = crate::tokenize::tokenise(query);
        let mut cand: std::collections::HashSet<&str> = std::collections::HashSet::new();
        if !post.is_empty() {
            for t in &q_tokens {
                if let Some(ps) = post.get(t) {
                    for p in ps {
                        cand.insert(p.chunk_id.as_str());
                    }
                }
            }
            if !focus.is_empty() {
                for c in &g.catalog.chunks {
                    if focus.iter().any(|id| id == &c.source_id) {
                        cand.insert(c.id.as_str());
                    }
                }
            }
            if cand.is_empty() {
                return Vec::new();
            }
        }
        let owned: Vec<Chunk> = g
            .catalog
            .chunks
            .iter()
            .filter(|c| active.contains(c.source_id.as_str()))
            .filter(|c| post.is_empty() || cand.contains(c.id.as_str()))
            .cloned()
            .collect();
        let scored = search_with_postings(
            &owned,
            if post.is_empty() { None } else { Some(post) },
            query,
            focus,
            MAX_HITS,
        );
        let mut hits: Vec<SourceHit> = scored
            .iter()
            .map(|s| SourceHit {
                id: s.chunk.source_id.clone(),
                title: s.chunk.title.clone(),
                excerpt: excerpt(&s.chunk.text),
            })
            .collect();
        if !focus.is_empty() {
            let mut seen: HashSet<String> = hits.iter().map(|h| h.excerpt.clone()).collect();
            for s in &scored {
                let ord = s.chunk.ordinal;
                for ch in &owned {
                    if ch.source_id != s.chunk.source_id || ch.ordinal.abs_diff(ord) != 1 {
                        continue;
                    }
                    let ex = excerpt(&ch.text);
                    if !seen.insert(ex.clone()) {
                        continue;
                    }
                    hits.push(SourceHit {
                        id: ch.source_id.clone(),
                        title: ch.title.clone(),
                        excerpt: ex,
                    });
                    if hits.len() >= MAX_HITS + 2 {
                        break;
                    }
                }
            }
        }
        hits
    }

    /// First paragraphs for the materials page. Empty if unread.
    pub fn preview(&self, id: &str, max_chars: usize) -> Option<String> {
        let body = self.root_path().join(id).join("body.md");
        let text = fs::read_to_string(body).ok()?;
        let t = text.trim();
        if t.is_empty() {
            return None;
        }
        let n = t.chars().count();
        if n <= max_chars {
            Some(t.to_string())
        } else {
            Some(format!(
                "{}…",
                t.chars().take(max_chars).collect::<String>()
            ))
        }
    }

    /// Keep a file. `body_md` is the already-converted text (from markitdown).
    pub fn ingest(
        &self,
        original_name: &str,
        original_bytes: &[u8],
        body_md: Option<&str>,
        ext: &str,
    ) -> Result<IngestOutcome> {
        let hash = sha256_hex(original_bytes);
        let title = human_title(original_name);
        let body = body_md.unwrap_or("").trim();
        if body.chars().count() > MAX_BODY_CHARS {
            return Err(SourceStoreError::Quota);
        }
        let readable = if body.chars().count() >= MIN_SEARCHABLE_CHARS && looks_like_text(body) {
            Readable::Ok
        } else {
            Readable::Unreadable
        };

        let mut g = self.inner.lock().expect("sources lock");

        if let Some(existing) = g
            .catalog
            .sources
            .iter()
            .find(|s| !s.superseded && s.hash == hash)
            .cloned()
        {
            return Ok(IngestOutcome {
                item: SourceItem::from(&existing),
                kind: "duplicate".into(),
            });
        }

        let near = g
            .catalog
            .sources
            .iter()
            .filter(|s| !s.superseded)
            .filter(|s| {
                same_stem(&s.original_name, original_name)
                    || (readable == Readable::Ok
                        && s.readable == Readable::Ok
                        && overlap_bodies(&g.catalog, &s.id, body) >= NEAR_VERSION)
            })
            .cloned()
            .max_by(|a, b| {
                overlap_bodies(&g.catalog, &a.id, body)
                    .partial_cmp(&overlap_bodies(&g.catalog, &b.id, body))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let active_n = g.catalog.sources.iter().filter(|s| !s.superseded).count();
        if near.is_none() && active_n >= MAX_SOURCES {
            return Err(SourceStoreError::Quota);
        }

        let id = new_id();
        let dir = self.root_path().join(&id);
        let staging = self.root_path().join(format!(".{id}.tmp"));
        if staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        fs::create_dir_all(&staging)?;
        fs::write(
            staging.join(format!("original.{}", ext.to_ascii_lowercase())),
            original_bytes,
        )?;
        if !body.is_empty() {
            fs::write(staging.join("body.md"), body)?;
        }
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        fs::rename(&staging, &dir)?;

        let mut meta = SourceMeta {
            id: id.clone(),
            title,
            original_name: original_name.to_string(),
            ext: ext.to_ascii_lowercase(),
            hash,
            created_at: Utc::now(),
            readable,
            superseded: false,
            supersedes: None,
            chars: body.chars().count(),
        };

        let mut kind = "created";
        if let Some(old) = near {
            if let Some(o) = g.catalog.sources.iter_mut().find(|s| s.id == old.id) {
                o.superseded = true;
            }
            g.catalog.chunks.retain(|c| c.source_id != old.id);
            meta.supersedes = Some(old.id.clone());
            kind = "new_version";
        }

        if readable == Readable::Ok {
            g.catalog
                .chunks
                .extend(split_chunks(&id, &meta.title, body));
        }
        g.catalog.sources.push(meta.clone());
        reindex(&mut g.catalog);
        save_catalog(&self.root_path(), &g.catalog)?;
        Ok(IngestOutcome {
            item: SourceItem::from(&meta),
            kind: kind.into(),
        })
    }

    /// Drop this version. If it replaced an older one, the older version
    /// becomes current again (chunks rebuilt from its `body.md`).
    pub fn undo_keep(&self, id: &str) -> Result<Option<SourceItem>> {
        let pred = {
            let g = self.inner.lock().expect("sources lock");
            g.catalog
                .sources
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.supersedes.clone())
        };
        self.remove_id(id)?;
        if let Some(pid) = pred {
            self.reactivate(&pid)?;
            Ok(self.get(&pid).as_ref().map(SourceItem::from))
        } else {
            Ok(None)
        }
    }

    /// Remove this version and every older version it replaced. The title
    /// is gone from 我的材料.
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut walk = vec![id.to_string()];
        {
            let g = self.inner.lock().expect("sources lock");
            let mut cur = g.catalog.sources.iter().find(|s| s.id == id).cloned();
            while let Some(s) = cur {
                if let Some(prev) = s.supersedes.clone() {
                    walk.push(prev.clone());
                    cur = g.catalog.sources.iter().find(|x| x.id == prev).cloned();
                } else {
                    break;
                }
            }
        }
        for one in walk {
            self.remove_id(&one)?;
        }
        Ok(())
    }

    pub fn read_text(&self, id_or_title: &str) -> Option<(SourceMeta, String)> {
        let g = self.inner.lock().expect("sources lock");
        let q = id_or_title.trim();
        let meta = g
            .catalog
            .sources
            .iter()
            .filter(|s| !s.superseded)
            .find(|s| s.id == q || s.title == q || s.original_name == q)
            .cloned()?;
        drop(g);
        if let Some(allow) = self.read_allow.lock().expect("sources allow").as_ref() {
            if !allow.contains(&meta.id) && !allow.contains(&meta.title) {
                return None;
            }
        }
        let body_path = self.root_path().join(&meta.id).join("body.md");
        let text = fs::read_to_string(body_path).unwrap_or_default();
        Some((meta, text))
    }

    fn remove_id(&self, id: &str) -> Result<()> {
        let mut g = self.inner.lock().expect("sources lock");
        let exists = g.catalog.sources.iter().any(|s| s.id == id);
        if !exists {
            return Err(SourceStoreError::NotFound);
        }
        g.catalog.sources.retain(|s| s.id != id);
        for s in &mut g.catalog.sources {
            if s.supersedes.as_deref() == Some(id) {
                s.supersedes = None;
            }
        }
        g.catalog.chunks.retain(|c| c.source_id != id);
        reindex(&mut g.catalog);
        save_catalog(&self.root_path(), &g.catalog)?;
        drop(g);
        let dir = self.root_path().join(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    fn reactivate(&self, id: &str) -> Result<()> {
        let mut g = self.inner.lock().expect("sources lock");
        let Some(meta) = g.catalog.sources.iter_mut().find(|s| s.id == id) else {
            return Err(SourceStoreError::NotFound);
        };
        meta.superseded = false;
        let title = meta.title.clone();
        let readable = meta.readable;
        g.catalog.chunks.retain(|c| c.source_id != id);
        if readable == Readable::Ok {
            let body =
                fs::read_to_string(self.root_path().join(id).join("body.md")).unwrap_or_default();
            if !body.trim().is_empty() {
                g.catalog.chunks.extend(split_chunks(id, &title, &body));
            }
        }
        reindex(&mut g.catalog);
        save_catalog(&self.root_path(), &g.catalog)?;
        Ok(())
    }
}

fn overlap_bodies(cat: &Catalog, id: &str, new_body: &str) -> f64 {
    let text: String = cat
        .chunks
        .iter()
        .filter(|c| c.source_id == id)
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() || new_body.is_empty() {
        return 0.0;
    }
    overlap(
        &text.chars().take(2500).collect::<String>(),
        &new_body.chars().take(2500).collect::<String>(),
    )
}

fn excerpt(text: &str) -> String {
    let t = text.trim();
    let n = t.chars().count();
    if n <= 420 {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(400).collect::<String>())
    }
}

fn new_id() -> String {
    format!("src_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn human_title(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let t = stem.trim();
    if t.is_empty() {
        name.to_string()
    } else {
        t.to_string()
    }
}

fn same_stem(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        Path::new(s)
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or(s)
            .to_lowercase()
            .replace([' ', '_', '-'], "")
            .replace("最终", "")
            .replace("副本", "")
            .replace("copy", "")
    }
    let na = norm(a);
    let nb = norm(b);
    !na.is_empty() && na == nb
}

fn looks_like_text(s: &str) -> bool {
    let mut good = 0usize;
    let mut n = 0usize;
    for c in s.chars().take(800) {
        n += 1;
        if c.is_alphanumeric()
            || crate::tokenize::is_cjk(c)
            || c.is_whitespace()
            || "，。；：、“”《》".contains(c)
        {
            good += 1;
        }
    }
    n == 0 || (good as f64 / n as f64) >= 0.45
}

fn reindex(cat: &mut Catalog) {
    cat.postings = rebuild_postings(&cat.chunks);
}

fn catalog_path(root: &Path) -> PathBuf {
    root.join(CATALOG_NAME)
}

fn load_catalog(root: &Path) -> Result<Catalog> {
    let p = catalog_path(root);
    if !p.exists() {
        return Ok(Catalog::default());
    }
    let raw = fs::read_to_string(&p)?;
    if raw.trim().is_empty() {
        return Ok(Catalog::default());
    }
    serde_json::from_str(&raw).map_err(|e| SourceStoreError::Parse(e.to_string()))
}

fn save_catalog(root: &Path, cat: &Catalog) -> Result<()> {
    let _lock = WriteLock::acquire(root)?;
    let p = catalog_path(root);
    let tmp = root.join(".catalog.json.tmp");
    let json =
        serde_json::to_string_pretty(cat).map_err(|e| SourceStoreError::Parse(e.to_string()))?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, p)?;
    Ok(())
}

/// Two windows leaving files at once: the later one waits, never half-writes.
struct WriteLock {
    path: PathBuf,
}

impl WriteLock {
    fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".write.lock");
        for _ in 0..80 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e.into()),
            }
        }
        // Stale lock (crash). Take over.
        let _ = fs::remove_file(&path);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self { path })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_keep_word_pdf_only() {
        assert!(is_auto_keep_ext("pdf"));
        assert!(is_auto_keep_ext("DOCX"));
        assert!(is_auto_keep_ext("doc"));
        assert!(!is_auto_keep_ext("xlsx"));
        assert!(!is_auto_keep_ext("png"));
    }

    #[test]
    fn ingest_search_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let body = "第七条 违约金。一方违约应向对方支付合同总额百分之二十的违约金。本条适用于全部服务期间。";
        let out = store
            .ingest("服务合同.pdf", b"%PDF-fake", Some(body), "pdf")
            .unwrap();
        assert_eq!(out.kind, "created");
        assert!(out.item.readable, "body should be searchable");
        let hits = store.search("违约金怎么写", &[]);
        assert!(!hits.is_empty(), "hits={hits:?}");
        assert!(hits[0].title.contains("服务合同"));
        store.delete(&out.item.id).unwrap();
        assert!(store.list_active().is_empty());
        assert!(store.search("违约金怎么计算", &[]).is_empty());
    }

    #[test]
    fn duplicate_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let a = store
            .ingest(
                "a.pdf",
                b"bytes-1",
                Some("甲方应当支付违约金条款若干。"),
                "pdf",
            )
            .unwrap();
        let b = store
            .ingest(
                "a.pdf",
                b"bytes-1",
                Some("甲方应当支付违约金条款若干。"),
                "pdf",
            )
            .unwrap();
        assert_eq!(b.kind, "duplicate");
        assert_eq!(a.item.id, b.item.id);
        assert_eq!(store.list_active().len(), 1);
    }

    #[test]
    fn new_version_same_stem() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let a = store
            .ingest(
                "对外口径.pdf",
                b"v1",
                Some("对外只讲三句话：安全、合规、可执行。不要发明口径。"),
                "pdf",
            )
            .unwrap();
        let b = store
            .ingest(
                "对外口径-最终.pdf",
                b"v2",
                Some("对外只讲三句话：安全、合规、可执行。新增：先结论。不要发明口径。"),
                "pdf",
            )
            .unwrap();
        assert_eq!(b.kind, "new_version");
        assert_eq!(store.list_active().len(), 1);
        assert_eq!(store.list_active()[0].id, b.item.id);
        assert!(store.get(&a.item.id).unwrap().superseded);
        let restored = store.undo_keep(&b.item.id).unwrap();
        assert_eq!(restored.unwrap().id, a.item.id);
        assert_eq!(store.list_active().len(), 1);
        assert_eq!(store.list_active()[0].id, a.item.id);
        assert!(!store.search("三句话 合规", &[]).is_empty());
    }

    #[test]
    fn delete_drops_whole_chain() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let a = store
            .ingest(
                "口径.pdf",
                b"v1",
                Some("对外只讲安全合规可执行不要发明。"),
                "pdf",
            )
            .unwrap();
        let b = store
            .ingest(
                "口径-最终.pdf",
                b"v2",
                Some("对外只讲安全合规可执行不要发明。加上先结论。"),
                "pdf",
            )
            .unwrap();
        store.delete(&b.item.id).unwrap();
        assert!(store.list_active().is_empty());
        assert!(store.get(&a.item.id).is_none());
    }

    /// Story F / G / H / I — user language, no GUI.
    #[test]
    fn stories_f_g_h_i() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        // F: keep 口径, ask in a new "session"
        store
            .ingest(
                "对外口径.docx",
                b"koujing-v1",
                Some("对外口径第三节：只讲安全、合规、可执行。不要发明新说法。朋友圈也按这三条。"),
                "docx",
            )
            .unwrap();
        let f = store.search("按我们口径写一条朋友圈", &[]);
        assert!(!f.is_empty(), "F: should ground in 对外口径");
        assert!(f[0].title.contains("对外口径"));
        // G: unrelated — no hit
        let g = store.search("今天天气怎么样", &[]);
        assert!(g.is_empty(), "G: weather must not hit materials");
        // H: follow-up stays on same file via focus
        store
            .ingest(
                "服务合同.pdf",
                b"hetong",
                Some("第七条违约金百分之二十。第八条逾期付款按日万分之五计。"),
                "pdf",
            )
            .unwrap();
        let h1 = store.search("合同违约金怎么写", &[]);
        assert!(h1.iter().any(|h| h.title.contains("服务合同")));
        let focus: Vec<String> = h1.iter().map(|h| h.id.clone()).collect();
        let h2 = store.search("那逾期呢", &focus);
        assert!(
            h2.iter()
                .any(|h| h.title.contains("服务合同") && h.excerpt.contains("逾期")),
            "H: follow-up should stay on the contract"
        );
        // I: new version + undo
        let v2 = store
            .ingest(
                "对外口径-最终.docx",
                b"koujing-v2",
                Some("对外口径第三节：只讲安全、合规、可执行。新增先结论。不要发明新说法。"),
                "docx",
            )
            .unwrap();
        assert_eq!(v2.kind, "new_version");
        store.undo_keep(&v2.item.id).unwrap();
        let after = store.search("按我们口径写一条朋友圈", &[]);
        assert!(after[0].excerpt.contains("朋友圈也按这三条"));
        // Body search (panel): 违约金 is not in the filename
        let by_body = store.list_matching("逾期付款");
        assert!(
            by_body.iter().any(|(i, _)| i.title.contains("服务合同")),
            "panel search must hit body, not only title"
        );
    }

    #[test]
    fn real_docx_bytes_roundtrip_when_textutil_exists() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("对外口径.txt");
        std::fs::write(
            &txt,
            "对外口径第三节：只讲安全、合规、可执行。不要发明新说法。朋友圈也按这三条。",
        )
        .unwrap();
        let ok = std::process::Command::new("textutil")
            .args(["-convert", "docx", "-output"])
            .arg(dir.path().join("对外口径.docx"))
            .arg(&txt)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return;
        }
        let bytes = std::fs::read(dir.path().join("对外口径.docx")).unwrap();
        assert!(bytes.len() > 80, "docx should be a real zip package");
        let store = SourceStore::open(dir.path().join("s")).unwrap();
        let body = "对外口径第三节：只讲安全、合规、可执行。不要发明新说法。朋友圈也按这三条。";
        store
            .ingest("对外口径.docx", &bytes, Some(body), "docx")
            .unwrap();
        assert!(store
            .original_path(&store.list_active()[0].id)
            .unwrap()
            .exists());
        assert!(!store.search("按我们口径写一条朋友圈", &[]).is_empty());
        assert!(store.search("今天天气怎么样", &[]).is_empty());
    }

    #[test]
    fn quota_blocks_201st() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        for i in 0..MAX_SOURCES {
            let body = format!("body{i}alpha body{i}beta body{i}gamma extra{i}text");
            store
                .ingest(
                    &format!("f{i}.pdf"),
                    format!("b{i}").as_bytes(),
                    Some(&body),
                    "pdf",
                )
                .unwrap();
        }
        assert_eq!(store.list_active().len(), MAX_SOURCES);
        let err = store
            .ingest(
                "overflow.pdf",
                b"new",
                Some("zebra volcano igloo waffle tandem"),
                "pdf",
            )
            .unwrap_err();
        assert!(matches!(err, SourceStoreError::Quota));
    }

    #[test]
    fn missing_original_flagged_and_preview() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let body = "第七条 违约金。一方违约应向对方支付合同总额百分之二十的违约金。";
        let out = store
            .ingest("服务合同.pdf", b"%PDF-fake", Some(body), "pdf")
            .unwrap();
        let preview = store.preview(&out.item.id, 40).unwrap();
        assert!(preview.contains("违约金"));
        let orig = dir.path().join(&out.item.id).join("original.pdf");
        std::fs::remove_file(orig).unwrap();
        let row = store
            .list_active_with_prev()
            .into_iter()
            .find(|(i, _)| i.id == out.item.id)
            .unwrap();
        assert!(row.0.original_missing);
    }

    #[test]
    fn read_allowlist_and_replace_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let body = "第七条 违约金。一方违约应向对方支付合同总额百分之二十的违约金。";
        let out = store
            .ingest("服务合同.pdf", b"%PDF-fake", Some(body), "pdf")
            .unwrap();
        store.set_read_allowlist(Some(vec![]));
        assert!(store.read_text(&out.item.id).is_none());
        store.set_read_allowlist(Some(vec![out.item.id.clone()]));
        assert!(store.read_text(&out.item.id).is_some());
        let dest = dir.path().join("moved");
        std::fs::create_dir_all(&dest).unwrap();
        // Copy current sources tree into dest.
        let src_id = dir.path().join(&out.item.id);
        if src_id.exists() {
            let to = dest.join(&out.item.id);
            std::fs::create_dir_all(&to).unwrap();
            for name in ["original.pdf", "body.md"] {
                let a = src_id.join(name);
                if a.exists() {
                    std::fs::copy(&a, to.join(name)).unwrap();
                }
            }
        }
        std::fs::copy(dir.path().join("catalog.json"), dest.join("catalog.json")).unwrap();
        store.replace_root(dest).unwrap();
        store.set_read_allowlist(None);
        assert!(store.read_text(&out.item.id).is_some());
    }

    #[test]
    fn unreadable_keep_has_no_hits() {
        let dir = tempfile::tempdir().unwrap();
        let store = SourceStore::open(dir.path().to_path_buf()).unwrap();
        let out = store
            .ingest("扫描件.pdf", b"%PDF-scan", None, "pdf")
            .unwrap();
        assert!(!out.item.readable);
        assert!(store.search("扫描 合同", &[]).is_empty());
        assert_eq!(store.list_active().len(), 1);
    }
}
