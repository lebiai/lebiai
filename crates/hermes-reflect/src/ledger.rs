//! Distill ledger: one cursor per session, written when a distill **finishes**.
//!
//! Stage = last human send time (and send count) at the snapshot we distilled.
//! Leave-session compares the live session against this cursor.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use hermes_core::{last_human_send, Session};
use serde::{Deserialize, Serialize};

fn inflight() -> &'static Mutex<HashSet<String>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn file_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Prevents leave-session and idle WeChat scans from distilling the same
/// file at once (two LLM jobs, racing inbox replace).
pub struct DistillSessionGuard {
    id: String,
}

impl DistillSessionGuard {
    pub fn try_acquire(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        let ok = inflight()
            .lock()
            .map(|mut s| s.insert(id.clone()))
            .unwrap_or(false);
        if ok {
            Some(Self { id })
        } else {
            None
        }
    }
}

impl Drop for DistillSessionGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = inflight().lock() {
            s.remove(&self.id);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillCursor {
    pub through_seq: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_at: Option<DateTime<Utc>>,
    pub distill_id: String,
    pub distilled_at: DateTime<Utc>,
    pub outcome: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LedgerFile {
    #[serde(default)]
    sessions: HashMap<String, DistillCursor>,
}

fn path() -> PathBuf {
    hermes_core::data_path("distill-ledger.json")
}

fn load() -> Option<LedgerFile> {
    let p = path();
    if !p.exists() {
        return Some(LedgerFile::default());
    }
    let Ok(raw) = fs::read_to_string(&p) else {
        return Some(LedgerFile::default());
    };
    if raw.trim().is_empty() {
        return Some(LedgerFile::default());
    }
    match serde_json::from_str(&raw) {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!(error = %e, "distill ledger unreadable; refusing to wipe");
            None
        }
    }
}

fn save(file: &LedgerFile) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(raw) = serde_json::to_string_pretty(file) else {
        return;
    };
    let tmp = p.with_extension("json.tmp");
    if fs::write(&tmp, raw).is_ok() {
        let _ = fs::rename(&tmp, &p);
    }
}

pub fn new_distill_id() -> String {
    format!("dst_{}", Utc::now().timestamp_millis())
}

pub fn get(session_id: &str) -> Option<DistillCursor> {
    let _g = file_lock().lock();
    get_unlocked(session_id)
}

fn get_unlocked(session_id: &str) -> Option<DistillCursor> {
    load()?.sessions.get(session_id).cloned()
}

/// True when this leave should run a full distill.
pub fn needs_distill(session: &Session) -> bool {
    let _g = file_lock().lock();
    let id = session.meta.id.as_str();
    let (seq, at) = last_human_send(&session.messages);
    if seq == 0 {
        return false;
    }
    let Some(cur) = get_unlocked(id) else {
        return true;
    };
    if seq > cur.through_seq {
        return true;
    }
    if seq < cur.through_seq {
        // Edit/truncate shrank the send count — already covered, do not loop.
        return false;
    }
    match (at, cur.through_at) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Record a finished distill (empty or enqueued). Never call on timeout/error.
pub fn record_success(session: &Session, distill_id: &str, outcome: &str) {
    let _g = file_lock().lock();
    let (seq, at) = last_human_send(&session.messages);
    let Some(mut file) = load() else {
        return;
    };
    let prev = file.sessions.get(&session.meta.id);
    if let Some(p) = prev {
        if seq < p.through_seq {
            return;
        }
        if let (Some(a), Some(b)) = (at, p.through_at) {
            if a < b {
                return;
            }
        }
    }
    file.sessions.insert(
        session.meta.id.clone(),
        DistillCursor {
            through_seq: seq,
            through_at: at,
            distill_id: distill_id.to_string(),
            distilled_at: Utc::now(),
            outcome: outcome.to_string(),
        },
    );
    save(&file);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::{Message, Session, SessionMeta};

    fn sess_with(msgs: Vec<Message>) -> Session {
        let mut s = Session::new(SessionMeta::new("m", "p"));
        s.messages = msgs;
        s
    }

    #[test]
    fn empty_session_does_not_need_distill() {
        let s = Session::new(SessionMeta::new("m", "p"));
        assert!(!needs_distill(&s));
    }

    #[test]
    fn first_human_send_needs_distill() {
        let s = sess_with(vec![Message::user_sent("查一下")]);
        assert!(needs_distill(&s));
    }

    #[test]
    fn session_guard_is_single_flight() {
        let a = DistillSessionGuard::try_acquire("sess_lock_test");
        assert!(a.is_some());
        assert!(DistillSessionGuard::try_acquire("sess_lock_test").is_none());
        drop(a);
        assert!(DistillSessionGuard::try_acquire("sess_lock_test").is_some());
    }
}
