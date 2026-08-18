//! Append-only JSONL session writer + reader.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use hermes_core::{Session, SessionEvent};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error at {path} line {line}: {source}")]
    Json {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("json serialize: {0}")]
    JsonSerialize(#[from] serde_json::Error),
    #[error("session at {path} has no meta record on the first valid line")]
    MissingMeta { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, SessionError>;

/// Owns an open file handle, appends one JSONL event per call, fsyncs on
/// every write so a crash leaves a valid transcript up to the last event.
#[derive(Debug)]
pub struct SessionWriter {
    path: PathBuf,
    file: File,
}

impl SessionWriter {
    /// Create a new session file. Errors if it already exists.
    pub fn create(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .map_err(|source| SessionError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(Self { path, file })
    }

    /// Open an existing session for appending. Use this when resuming a
    /// previous session: subsequent events land at the end of the file
    /// alongside the original transcript.
    pub fn open_append(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|source| SessionError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(Self { path, file })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, event: &SessionEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        writeln!(self.file, "{line}").map_err(|source| SessionError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.file.sync_data().map_err(|source| SessionError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// Replay a JSONL session file into an in-memory [`Session`].
///
/// The first valid line MUST be a `Meta` event (otherwise we don't know
/// the session id / model / provider). All `Message` events build the
/// transcript; `Usage` events accumulate into running totals. Unknown
/// or malformed lines are skipped with a warning.
pub fn read_session(path: impl AsRef<Path>) -> Result<Session> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);

    let mut session: Option<Session> = None;
    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let event: SessionEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(source) => {
                tracing::warn!(line = idx + 1, %source, "skipping malformed session line");
                continue;
            }
        };
        match event {
            SessionEvent::Meta(meta) => {
                if session.is_none() {
                    session = Some(Session::new(meta));
                } else {
                    // Some sessions resumed multiple times might have
                    // additional Meta records; ignore them after the first.
                    tracing::debug!(line = idx + 1, "extra meta event ignored");
                }
            }
            SessionEvent::Message(msg) => {
                let s = session.as_mut().ok_or_else(|| SessionError::MissingMeta {
                    path: path.to_path_buf(),
                })?;
                s.messages.push(msg);
            }
            SessionEvent::Usage(u) => {
                let s = session.as_mut().ok_or_else(|| SessionError::MissingMeta {
                    path: path.to_path_buf(),
                })?;
                s.record_usage(u);
            }
        }
    }

    session.ok_or_else(|| SessionError::MissingMeta {
        path: path.to_path_buf(),
    })
}

/// List session JSONL files under `dir` (including `wechat/` etc.), newest first.
pub fn list_sessions(dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    collect_jsonl(dir, &mut entries)?;
    entries.sort_by_key(|b| std::cmp::Reverse(b.0));
    Ok(entries.into_iter().map(|(_, p)| p).collect())
}

fn collect_jsonl(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) -> Result<()> {
    let rd = std::fs::read_dir(dir).map_err(|source| SessionError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect_jsonl(&p, out)?;
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".jsonl") || name.ends_with(".tmp") {
            continue;
        }
        let m = e.metadata().ok();
        let t = m.and_then(|m| m.modified().ok());
        if let Some(t) = t {
            out.push((t, p));
        }
    }
    Ok(())
}

/// `wechat` / `feishu` / `telegram` when `path` lives under that channel folder.
pub fn channel_of_session_path(path: &Path) -> Option<&'static str> {
    let root = hermes_core::data_path("sessions");
    let rel = path.strip_prefix(&root).ok()?;
    match rel.iter().next()?.to_str()? {
        "wechat" => Some("wechat"),
        "feishu" => Some("feishu"),
        "telegram" => Some("telegram"),
        _ => None,
    }
}

/// Atomically rewrite a session file from an in-memory [`Session`].
///
/// Used when truncating history (edit / regenerate). Writes Meta + all
/// Messages, then a single aggregated Usage line so reload keeps totals.
pub fn rewrite_session(path: impl AsRef<Path>, session: &Session) -> Result<()> {
    use hermes_core::Usage;

    let path = path.as_ref();
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut file = File::create(&tmp).map_err(|source| SessionError::Io {
            path: tmp.clone(),
            source,
        })?;
        let meta_line = serde_json::to_string(&SessionEvent::Meta(session.meta.clone()))?;
        writeln!(file, "{meta_line}").map_err(|source| SessionError::Io {
            path: tmp.clone(),
            source,
        })?;
        for msg in &session.messages {
            let line = serde_json::to_string(&SessionEvent::Message(msg.clone()))?;
            writeln!(file, "{line}").map_err(|source| SessionError::Io {
                path: tmp.clone(),
                source,
            })?;
        }
        if session.total_input_tokens > 0 || session.total_output_tokens > 0 {
            let usage = Usage {
                input_tokens: session.total_input_tokens,
                output_tokens: session.total_output_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            };
            let line = serde_json::to_string(&SessionEvent::Usage(usage))?;
            writeln!(file, "{line}").map_err(|source| SessionError::Io {
                path: tmp.clone(),
                source,
            })?;
        }
        file.sync_data().map_err(|source| SessionError::Io {
            path: tmp.clone(),
            source,
        })?;
    }
    std::fs::rename(&tmp, path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Rewrite the first Meta line's `title` field; preserve the rest of the file.
///
/// Append-only JSONL cannot update line 0 in place without a rewrite; session
/// files stay small enough that a full rewrite is acceptable.
pub fn update_session_title(path: impl AsRef<Path>, title: impl Into<String>) -> Result<()> {
    let path = path.as_ref();
    let title = title.into();
    let raw = std::fs::read_to_string(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut out_lines: Vec<String> = Vec::new();
    let mut saw_meta = false;
    for (idx, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if !saw_meta {
            let event: SessionEvent =
                serde_json::from_str(line).map_err(|source| SessionError::Json {
                    path: path.to_path_buf(),
                    line: idx + 1,
                    source,
                })?;
            match event {
                SessionEvent::Meta(mut meta) => {
                    meta.title = Some(title.clone());
                    let rewritten = serde_json::to_string(&SessionEvent::Meta(meta))?;
                    out_lines.push(rewritten);
                    saw_meta = true;
                    continue;
                }
                other => {
                    // Unexpected first event — keep as-is but still fail soft.
                    out_lines.push(serde_json::to_string(&other)?);
                    saw_meta = true;
                    continue;
                }
            }
        }
        out_lines.push(line.to_string());
    }
    if !saw_meta {
        return Err(SessionError::MissingMeta {
            path: path.to_path_buf(),
        });
    }
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, out_lines.join("\n") + "\n").map_err(|source| SessionError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Delete JSONL sessions that have no user text (meta-only drafts / tool-noise).
/// Returns how many files were removed.
pub fn purge_empty_sessions(dir: impl AsRef<Path>) -> Result<usize> {
    let paths = list_sessions(dir)?;
    let mut removed = 0usize;
    for path in paths {
        let session = match read_session(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if hermes_core::session_has_user_text(&session.messages) {
            continue;
        }
        // Keep files that only exist in-memory concurrent with a live window:
        // if modified in the last 30s, skip (race with brand-new draft).
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed.as_secs() < 30 {
                        continue;
                    }
                }
            }
        }
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::{Message, SessionMeta, Usage};

    #[test]
    fn append_meta_then_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");

        let meta = SessionMeta::new("deepseek-v4-pro", "anthropic");
        let mut writer = SessionWriter::create(&path).unwrap();
        writer.append(&SessionEvent::Meta(meta.clone())).unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("hello")))
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let mut lines = raw.lines();
        let l1 = lines.next().unwrap();
        let l2 = lines.next().unwrap();
        assert!(l1.contains("\"meta\""));
        assert!(l1.contains(&meta.id));
        assert!(l2.contains("\"message\""));
        assert!(l2.contains("hello"));
        assert!(lines.next().is_none());
    }

    #[test]
    fn create_refuses_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(&path, b"").unwrap();
        let err = SessionWriter::create(&path).unwrap_err();
        assert!(matches!(err, SessionError::Io { .. }));
    }

    #[test]
    fn read_back_meta_messages_usage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");

        let meta = SessionMeta::new("deepseek-v4-pro", "anthropic");
        let id = meta.id.clone();
        let mut writer = SessionWriter::create(&path).unwrap();
        writer.append(&SessionEvent::Meta(meta)).unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("hi")))
            .unwrap();
        writer
            .append(&SessionEvent::Message(Message::assistant_text("hey")))
            .unwrap();
        writer
            .append(&SessionEvent::Usage(Usage {
                input_tokens: 12,
                output_tokens: 30,
                ..Default::default()
            }))
            .unwrap();

        let session = read_session(&path).unwrap();
        assert_eq!(session.meta.id, id);
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.total_input_tokens, 12);
        assert_eq!(session.total_output_tokens, 30);
    }

    #[test]
    fn open_append_continues_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");

        let meta = SessionMeta::new("m", "p");
        {
            let mut w = SessionWriter::create(&path).unwrap();
            w.append(&SessionEvent::Meta(meta.clone())).unwrap();
            w.append(&SessionEvent::Message(Message::user_text("first")))
                .unwrap();
        }
        {
            let mut w = SessionWriter::open_append(&path).unwrap();
            w.append(&SessionEvent::Message(Message::user_text("resumed")))
                .unwrap();
        }
        let session = read_session(&path).unwrap();
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn missing_meta_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(
            &path,
            br#"{"message":{"role":"user","content":[{"type":"text","text":"x"}]}}"#,
        )
        .unwrap();
        let err = read_session(&path).unwrap_err();
        assert!(matches!(err, SessionError::MissingMeta { .. }));
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let raw = format!(
            "{}\nthis-is-not-json\n{}\n",
            r#"{"meta":{"id":"abc","created_at":"2025-01-01T00:00:00Z","model":"m","provider":"p"}}"#,
            r#"{"message":{"role":"user","content":[{"type":"text","text":"x"}]}}"#
        );
        std::fs::write(&path, raw).unwrap();
        let session = read_session(&path).unwrap();
        assert_eq!(session.meta.id, "abc");
        assert_eq!(session.messages.len(), 1);
    }

    #[test]
    fn rewrite_session_truncates_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let meta = SessionMeta::new("m", "p");
        let mut writer = SessionWriter::create(&path).unwrap();
        writer.append(&SessionEvent::Meta(meta.clone())).unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("a")))
            .unwrap();
        writer
            .append(&SessionEvent::Message(Message::assistant_text("b")))
            .unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("c")))
            .unwrap();
        drop(writer);

        let mut session = read_session(&path).unwrap();
        session.messages.truncate(1);
        rewrite_session(&path, &session).unwrap();
        let again = read_session(&path).unwrap();
        assert_eq!(again.messages.len(), 1);
        assert_eq!(again.messages[0].content[0].as_text(), Some("a"));
    }

    #[test]
    fn update_session_title_rewrites_meta() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let meta = SessionMeta::new("m", "p");
        let mut writer = SessionWriter::create(&path).unwrap();
        writer.append(&SessionEvent::Meta(meta)).unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("你好啊")))
            .unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text("做短视频")))
            .unwrap();
        update_session_title(&path, "做短视频").unwrap();
        let s = read_session(&path).unwrap();
        assert_eq!(s.meta.title.as_deref(), Some("做短视频"));
        assert_eq!(s.messages.len(), 2);
    }

    #[test]
    fn list_sessions_orders_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jsonl");
        let b = dir.path().join("b.jsonl");
        std::fs::write(&a, b"").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&b, b"").unwrap();
        let listed = list_sessions(dir.path()).unwrap();
        assert_eq!(listed[0], b);
        assert_eq!(listed[1], a);
    }

    #[test]
    fn list_sessions_includes_nested_wechat() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("wechat").join("uid");
        std::fs::create_dir_all(&nested).unwrap();
        let inner = nested.join("w.jsonl");
        std::fs::write(&inner, b"").unwrap();
        let listed = list_sessions(dir.path()).unwrap();
        assert!(listed.iter().any(|p| p == &inner), "{listed:?}");
    }
}
