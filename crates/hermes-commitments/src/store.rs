//! One plaintext file: `{data_root}/commitments.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::due::{parse_due, DueError};
use crate::near::{score_near, NearHit, NEAR_ASK, NEAR_FOLD};

pub const SUGGESTED_TTL_DAYS: i64 = 7;
pub const OPEN_CROWD: usize = 7;
pub const INDEX_CAP: usize = 7;

#[derive(Debug, Error)]
pub enum CommitmentError {
    #[error("commitment not found: {0}")]
    NotFound(String),
    #[error("title is empty")]
    EmptyTitle,
    #[error("due day required")]
    MissingDue,
    #[error("due phrase is not a day")]
    VagueDue,
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, CommitmentError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    Suggested,
    Waiting,
    Done,
    Dropped,
}

impl Status {
    pub fn is_owed(self) -> bool {
        matches!(self, Status::Open | Status::Waiting)
    }

    pub fn is_live(self) -> bool {
        matches!(self, Status::Open | Status::Waiting | Status::Suggested)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Dialogue,
    User,
    Residue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commitment {
    pub id: String,
    pub title: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_due_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_at: Option<DateTime<Utc>>,
    pub source: Source,
}

impl Commitment {
    pub fn new(title: impl Into<String>, source: Source) -> Result<Self> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err(CommitmentError::EmptyTitle);
        }
        let now = Utc::now();
        Ok(Self {
            id: format!("cmt_{}", uuid::Uuid::new_v4().simple()),
            title,
            status: Status::Open,
            done_when: None,
            soft_due: None,
            soft_due_date: None,
            session_id: None,
            note: None,
            aliases: Vec::new(),
            created_at: now,
            updated_at: now,
            suggested_at: None,
            source,
        })
    }

    pub fn phrases(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.title.as_str()).chain(self.aliases.iter().map(String::as_str))
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.status.is_owed() && self.soft_due_date.map(|d| d < today).unwrap_or(true)
    }

    pub fn is_due_today(&self, today: NaiveDate) -> bool {
        self.status.is_owed() && self.soft_due_date == Some(today)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Return [`SaveOutcome::Near`] when a live item looks like the same debt.
    Ask,
    /// Fold into the best live hit at or above [`NEAR_FOLD`]; else create.
    FoldStrong,
    /// Always insert a new row.
    ForceNew,
}

#[derive(Debug, Clone)]
pub enum SaveOutcome {
    Created(Commitment),
    Folded { into: Commitment },
    Near { existing: Commitment, score: f64 },
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileDoc {
    #[serde(default)]
    items: Vec<Commitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic_pair: Option<(String, String)>,
}

pub struct CommitmentStore {
    path: PathBuf,
}

impl CommitmentStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn standard() -> Self {
        Self::new(standard_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load_doc(&self) -> Result<FileDoc> {
        if !self.path.exists() {
            return Ok(FileDoc::default());
        }
        let raw = fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(FileDoc::default());
        }
        serde_json::from_str(&raw).map_err(|e| CommitmentError::Parse(e.to_string()))
    }

    pub fn load(&self) -> Result<Vec<Commitment>> {
        Ok(self.load_doc()?.items)
    }

    pub fn semantic_pair(&self) -> Result<Option<(String, String)>> {
        Ok(self.load_doc()?.semantic_pair)
    }

    pub fn set_semantic_pair(&self, pair: Option<(String, String)>) -> Result<()> {
        let mut doc = self.load_doc()?;
        doc.semantic_pair = pair;
        self.write_doc(&doc)
    }

    fn write_all(&self, items: &[Commitment]) -> Result<()> {
        let mut doc = self.load_doc().unwrap_or_default();
        doc.items = items.to_vec();
        self.write_doc(&doc)
    }

    fn write_doc(&self, doc: &FileDoc) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&doc)
            .map_err(|e| CommitmentError::Parse(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list_live(&self) -> Result<Vec<Commitment>> {
        let mut items = self.load()?;
        let before = items.len();
        items.retain(|c| {
            if c.status != Status::Suggested {
                return c.status.is_live();
            }
            let Some(at) = c.suggested_at else {
                return true;
            };
            Utc::now() - at < Duration::days(SUGGESTED_TTL_DAYS)
        });
        if items.len() != before {
            let keep_ids: std::collections::HashSet<_> =
                items.iter().map(|c| c.id.clone()).collect();
            let mut all = self.load()?;
            all.retain(|c| {
                if c.status != Status::Suggested {
                    return true;
                }
                keep_ids.contains(&c.id)
            });
            self.write_all(&all)?;
        }
        items.sort_by(|a, b| match (a.status, b.status) {
            (Status::Suggested, Status::Suggested) => b.updated_at.cmp(&a.updated_at),
            (Status::Suggested, _) => std::cmp::Ordering::Less,
            (_, Status::Suggested) => std::cmp::Ordering::Greater,
            _ => due_then_updated(a, b),
        });
        Ok(items)
    }

    pub fn list_owed(&self) -> Result<Vec<Commitment>> {
        Ok(self
            .list_live()?
            .into_iter()
            .filter(|c| c.status.is_owed())
            .collect())
    }

    /// Done rows updated in the last `days` days, newest first.
    pub fn list_recent_done(&self, days: i64) -> Result<Vec<Commitment>> {
        let cut = Utc::now() - Duration::days(days);
        let mut items: Vec<_> = self
            .load()?
            .into_iter()
            .filter(|c| c.status == Status::Done && c.updated_at >= cut)
            .collect();
        items.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(items)
    }

    pub fn get(&self, id: &str) -> Result<Option<Commitment>> {
        Ok(self.load()?.into_iter().find(|c| c.id == id))
    }

    pub fn find_near_live(&self, title: &str) -> Result<Option<(Commitment, f64)>> {
        let mut best: Option<(Commitment, f64)> = None;
        for c in self.list_live()? {
            let score = c
                .phrases()
                .map(|p| score_near(title, p))
                .fold(0.0_f64, f64::max);
            if score >= NEAR_ASK && best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
                best = Some((c, score));
            }
        }
        Ok(best)
    }

    pub fn save(&self, mut item: Commitment, mode: SaveMode) -> Result<SaveOutcome> {
        item.title = item.title.trim().to_string();
        if item.title.is_empty() {
            return Err(CommitmentError::EmptyTitle);
        }
        if item.status.is_owed() && item.soft_due_date.is_none() {
            return Err(CommitmentError::MissingDue);
        }
        if mode != SaveMode::ForceNew {
            if let Some((existing, score)) = self.find_near_live(&item.title)? {
                if mode == SaveMode::FoldStrong && score >= NEAR_FOLD {
                    return self.fold_into(&existing.id, &item);
                }
                return Ok(SaveOutcome::Near { existing, score });
            }
        }
        let mut all = self.load()?;
        item.updated_at = Utc::now();
        all.push(item.clone());
        self.write_all(&all)?;
        Ok(SaveOutcome::Created(item))
    }

    pub fn fold_into(&self, id: &str, incoming: &Commitment) -> Result<SaveOutcome> {
        let mut all = self.load()?;
        let target = all
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| CommitmentError::NotFound(id.to_string()))?;
        if incoming.title != target.title
            && !target.aliases.iter().any(|a| a == &incoming.title)
            && score_near(&incoming.title, &target.title) < 1.0
        {
            target.aliases.push(incoming.title.clone());
        }
        if target.done_when.is_none() {
            target.done_when = incoming.done_when.clone();
        }
        if target.soft_due.is_none() {
            target.soft_due = incoming.soft_due.clone();
            target.soft_due_date = incoming.soft_due_date;
        }
        if target.note.is_none() {
            target.note = incoming.note.clone();
        }
        if target.session_id.is_none() {
            target.session_id = incoming.session_id.clone();
        }
        if target.status == Status::Suggested && incoming.status.is_owed() {
            target.status = incoming.status;
            target.suggested_at = None;
        }
        target.updated_at = Utc::now();
        let out = target.clone();
        self.write_all(&all)?;
        Ok(SaveOutcome::Folded { into: out })
    }

    pub fn accept_suggested(&self, id: &str) -> Result<Commitment> {
        let c = self
            .get(id)?
            .ok_or_else(|| CommitmentError::NotFound(id.to_string()))?;
        if c.soft_due_date.is_none() {
            return Err(CommitmentError::MissingDue);
        }
        self.patch(id, |c| {
            c.status = Status::Open;
            c.suggested_at = None;
        })
    }

    pub fn reject_suggested(&self, id: &str) -> Result<Commitment> {
        self.patch(id, |c| {
            c.status = Status::Dropped;
            c.suggested_at = None;
        })
    }

    pub fn close(&self, id: &str, status: Status) -> Result<Commitment> {
        if !matches!(status, Status::Done | Status::Dropped) {
            return self.patch(id, |_| {});
        }
        self.patch(id, |c| {
            c.status = status;
        })
    }

    pub fn set_waiting(&self, id: &str, note: Option<String>) -> Result<Commitment> {
        self.patch(id, |c| {
            c.status = Status::Waiting;
            if let Some(n) = note {
                c.note = Some(n);
            }
        })
    }

    pub fn retitle(&self, id: &str, title: String) -> Result<Commitment> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(CommitmentError::EmptyTitle);
        }
        self.patch(id, |c| {
            c.title = title;
        })
    }

    pub fn reopen(&self, id: &str) -> Result<Commitment> {
        self.patch(id, |c| {
            if c.status == Status::Waiting || c.status == Status::Suggested {
                c.status = Status::Open;
                c.suggested_at = None;
            }
        })
    }

    pub fn patch_note(&self, id: &str, note: Option<String>) -> Result<Commitment> {
        self.patch(id, |c| {
            c.note = note.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            });
        })
    }

    pub fn patch_soft_due(&self, id: &str, phrase: String) -> Result<Commitment> {
        let phrase = phrase.trim().to_string();
        if phrase.is_empty() {
            return Err(CommitmentError::MissingDue);
        }
        let today = chrono::Local::now().date_naive();
        let (kept, date) = parse_due(&phrase, today).map_err(|e| match e {
            DueError::Vague => CommitmentError::VagueDue,
            DueError::Unparsed => CommitmentError::MissingDue,
        })?;
        self.patch(id, |c| {
            c.soft_due = Some(kept);
            c.soft_due_date = Some(date);
        })
    }

    pub fn set_done_when(&self, id: &str, done_when: Option<String>) -> Result<Commitment> {
        self.patch(id, |c| {
            c.done_when = done_when.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            });
        })
    }

    pub fn merge_ids(&self, keep_id: &str, other_id: &str) -> Result<Commitment> {
        if keep_id == other_id {
            return self
                .get(keep_id)?
                .ok_or_else(|| CommitmentError::NotFound(keep_id.to_string()));
        }
        let other = self
            .get(other_id)?
            .ok_or_else(|| CommitmentError::NotFound(other_id.to_string()))?;
        self.fold_into(keep_id, &other)?;
        let _ = self.close(other_id, Status::Dropped);
        self.get(keep_id)?
            .ok_or_else(|| CommitmentError::NotFound(keep_id.to_string()))
    }

    /// First title stays on `id`; remaining titles become new open rows.
    pub fn split(&self, id: &str, titles: &[String]) -> Result<Vec<Commitment>> {
        let clean: Vec<String> = titles
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if clean.len() < 2 {
            return Err(CommitmentError::Parse(
                "split needs at least two titles".into(),
            ));
        }
        let orig = self
            .get(id)?
            .ok_or_else(|| CommitmentError::NotFound(id.to_string()))?;
        let first = clean[0].clone();
        let kept = self.retitle(id, first)?;
        let mut out = vec![kept.clone()];
        for title in clean.iter().skip(1) {
            let mut item = Commitment::new(title, orig.source)?;
            item.status = Status::Open;
            item.session_id = orig.session_id.clone();
            item.soft_due = orig.soft_due.clone();
            item.soft_due_date = orig.soft_due_date;
            match self.save(item, SaveMode::ForceNew)? {
                SaveOutcome::Created(c) => out.push(c),
                SaveOutcome::Folded { into } => out.push(into),
                SaveOutcome::Near { existing, .. } => out.push(existing),
            }
        }
        Ok(out)
    }

    /// First pair of owed items that look like the same debt (lexical).
    pub fn lexical_merge_pair(&self) -> Result<Option<(Commitment, Commitment, f64)>> {
        let owed = self.list_owed()?;
        let mut best: Option<(Commitment, Commitment, f64)> = None;
        for i in 0..owed.len() {
            for j in (i + 1)..owed.len() {
                let score = owed[i]
                    .phrases()
                    .flat_map(|a| owed[j].phrases().map(move |b| score_near(a, b)))
                    .fold(0.0_f64, f64::max);
                if score >= NEAR_ASK
                    && best.as_ref().map(|(_, _, s)| score > *s).unwrap_or(true)
                {
                    best = Some((owed[i].clone(), owed[j].clone(), score));
                }
            }
        }
        Ok(best)
    }

    fn patch(&self, id: &str, f: impl FnOnce(&mut Commitment)) -> Result<Commitment> {
        let mut all = self.load()?;
        let target = all
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| CommitmentError::NotFound(id.to_string()))?;
        f(target);
        target.updated_at = Utc::now();
        let out = target.clone();
        self.write_all(&all)?;
        Ok(out)
    }
}

fn due_then_updated(a: &Commitment, b: &Commitment) -> std::cmp::Ordering {
    let today = chrono::Local::now().date_naive();
    rank_due(a, today)
        .cmp(&rank_due(b, today))
        .then(b.updated_at.cmp(&a.updated_at))
}

fn rank_due(c: &Commitment, today: NaiveDate) -> (u8, NaiveDate) {
    match c.soft_due_date {
        Some(d) if d < today => (0, d),
        None => (0, NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()),
        Some(d) if d == today => (1, d),
        Some(d) => (2, d),
    }
}

pub fn standard_path() -> PathBuf {
    hermes_core::data_path("commitments.json")
}

/// Best live near-hit as a compact struct (tools / GUI).
pub fn best_near(store: &CommitmentStore, title: &str) -> Result<Option<NearHit>> {
    Ok(store.find_near_live(title)?.map(|(c, score)| NearHit {
        id: c.id,
        title: c.title,
        score,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> CommitmentStore {
        let dir = tempdir().unwrap();
        CommitmentStore::new(dir.path().join("commitments.json"))
    }

    fn owed(title: &str) -> Commitment {
        let mut c = Commitment::new(title, Source::User).unwrap();
        let today = chrono::Local::now().date_naive();
        c.soft_due = Some("这周".into());
        c.soft_due_date = Some(today + Duration::days(4));
        c
    }

    #[test]
    fn create_and_list() {
        let s = store();
        let item = owed("周五交改稿");
        let out = s.save(item, SaveMode::ForceNew).unwrap();
        assert!(matches!(out, SaveOutcome::Created(_)));
        assert_eq!(s.list_owed().unwrap().len(), 1);
    }

    #[test]
    fn near_ask_does_not_duplicate() {
        let s = store();
        let a = owed("周五交改稿");
        s.save(a, SaveMode::ForceNew).unwrap();
        let b = owed("交改稿");
        match s.save(b, SaveMode::Ask).unwrap() {
            SaveOutcome::Near { existing, score } => {
                assert!(score >= NEAR_ASK);
                assert!(existing.title.contains("改稿"));
            }
            other => panic!("expected Near, got {other:?}"),
        }
        assert_eq!(s.list_owed().unwrap().len(), 1);
    }

    #[test]
    fn fold_keeps_user_title() {
        let s = store();
        let a = owed("周五交改稿");
        s.save(a, SaveMode::ForceNew).unwrap();
        let mut b = owed("交改稿");
        b.note = Some("等老王".into());
        match s.save(b, SaveMode::FoldStrong).unwrap() {
            SaveOutcome::Folded { into } => {
                assert_eq!(into.title, "周五交改稿");
                assert_eq!(into.note.as_deref(), Some("等老王"));
                assert!(into.aliases.iter().any(|x| x == "交改稿"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.list_owed().unwrap().len(), 1);
    }

    #[test]
    fn done_is_not_merge_target() {
        let s = store();
        let a = owed("交改稿");
        let id = match s.save(a, SaveMode::ForceNew).unwrap() {
            SaveOutcome::Created(c) => c.id,
            _ => panic!(),
        };
        s.close(&id, Status::Done).unwrap();
        let b = owed("交改稿再改一版");
        // Containment might still hit if we searched done — we must not.
        match s.save(b, SaveMode::Ask).unwrap() {
            SaveOutcome::Created(_) => {}
            SaveOutcome::Near { existing, .. } => {
                panic!("must not merge into done {}", existing.id)
            }
            SaveOutcome::Folded { .. } => panic!("folded into done"),
        }
    }

    #[test]
    fn suggested_expires() {
        let s = store();
        let mut a = Commitment::new("余债", Source::Residue).unwrap();
        a.status = Status::Suggested;
        a.suggested_at = Some(Utc::now() - Duration::days(SUGGESTED_TTL_DAYS + 1));
        s.save(a, SaveMode::ForceNew).unwrap();
        assert!(s.list_live().unwrap().is_empty());
    }

    #[test]
    fn merge_ids_drops_other() {
        let s = store();
        let a = match s
            .save(
                owed("周五交改稿"),
                SaveMode::ForceNew,
            )
            .unwrap()
        {
            SaveOutcome::Created(c) => c,
            _ => panic!(),
        };
        let b = match s
            .save(
                owed("约设计"),
                SaveMode::ForceNew,
            )
            .unwrap()
        {
            SaveOutcome::Created(c) => c,
            _ => panic!(),
        };
        s.merge_ids(&a.id, &b.id).unwrap();
        let owed = s.list_owed().unwrap();
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].id, a.id);
        assert!(owed[0].aliases.iter().any(|x| x == "约设计"));
    }

    #[test]
    fn split_makes_two() {
        let s = store();
        let a = match s
            .save(
                owed("一堆事"),
                SaveMode::ForceNew,
            )
            .unwrap()
        {
            SaveOutcome::Created(c) => c,
            _ => panic!(),
        };
        let out = s.split(&a.id, &["交改稿".into(), "约设计".into()]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(s.list_owed().unwrap().len(), 2);
        assert_eq!(s.get(&a.id).unwrap().unwrap().title, "交改稿");
    }

    #[test]
    fn open_without_due_refused() {
        let s = store();
        let item = Commitment::new("交改稿", Source::User).unwrap();
        match s.save(item, SaveMode::ForceNew) {
            Err(CommitmentError::MissingDue) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn retitle_keeps_due() {
        let s = store();
        let a = match s.save(owed("交改稿"), SaveMode::ForceNew).unwrap() {
            SaveOutcome::Created(c) => c,
            _ => panic!(),
        };
        let due = a.soft_due_date;
        s.retitle(&a.id, "周五交改稿".into()).unwrap();
        let got = s.get(&a.id).unwrap().unwrap();
        assert_eq!(got.title, "周五交改稿");
        assert_eq!(got.soft_due_date, due);
    }

    #[test]
    fn accept_suggested_needs_due() {
        let s = store();
        let mut a = Commitment::new("余债", Source::Residue).unwrap();
        a.status = Status::Suggested;
        a.suggested_at = Some(Utc::now());
        let id = match s.save(a, SaveMode::ForceNew).unwrap() {
            SaveOutcome::Created(c) => c.id,
            _ => panic!(),
        };
        match s.accept_suggested(&id) {
            Err(CommitmentError::MissingDue) => {}
            other => panic!("{other:?}"),
        }
        s.patch_soft_due(&id, "这周".into()).unwrap();
        let got = s.accept_suggested(&id).unwrap();
        assert_eq!(got.status, Status::Open);
        assert!(got.soft_due_date.is_some());
    }
}
