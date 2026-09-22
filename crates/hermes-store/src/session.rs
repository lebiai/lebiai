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

/// Unicode 行分隔符 / 段分隔符。JSON 字符串里合法，`serde_json` 也确实
/// 原样写出来 —— 但几乎所有**按行处理文本**的工具（python 的
/// `str.splitlines()`、多数编辑器的批量编辑）都把它们当换行，于是把一条
/// 事件劈成多行，再读回来时整条消息被当成坏行丢掉。
///
/// 会话是 append-only 的活文件，必须对任何工具都安全：落盘时换成普通
/// 换行的转义（`\n`），语义不变，文件里一个原始分隔符都不留。
const LINE_SEPARATORS: &[char] = &['\u{2028}', '\u{2029}'];

fn escape_line_separators(line: String) -> String {
    if line.contains(LINE_SEPARATORS) {
        line.replace(LINE_SEPARATORS, "\\n")
    } else {
        line
    }
}

/// 落盘编码的唯一入口：所有写会话文件的地方都必须走这里。
fn encode_event(event: &SessionEvent) -> Result<String> {
    Ok(escape_line_separators(serde_json::to_string(event)?))
}

/// 单条事件拼接上限。超过这个长度还读不出来就放弃，避免一条坏行把
/// 后面的整段会话都吞掉。
const MAX_EVENT_BYTES: usize = 1 << 20;

/// 逐行读 JSONL 事件；历史上被行分隔符劈开的半条事件在这里拼回来。
///
/// 拼接只在「这行自己不是完整事件」时发生，且每次拼之前先确认下一行不是
/// 独立事件 —— 否则一条坏行会把后面的会话全吞掉。拼不回来的行如实计数
/// 并写进 warn，不装看不见。
pub(crate) fn read_events(reader: impl BufRead, path: &Path) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    let mut merged = 0usize;
    let mut dropped = 0usize;

    for (idx, raw) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        if let Some((start, buf)) = pending.take() {
            let joined = if buf.len() + line.len() <= MAX_EVENT_BYTES {
                Some(format!("{buf}\\n{line}"))
            } else {
                None
            };
            if let Some(Ok(event)) = joined.as_deref().map(serde_json::from_str::<SessionEvent>) {
                merged += 1;
                events.push(event);
                continue;
            }
            if serde_json::from_str::<SessionEvent>(&line).is_err() {
                match joined {
                    Some(j) => {
                        pending = Some((start, j));
                        continue;
                    }
                    None => {
                        dropped += 1;
                        tracing::warn!(path = %path.display(), line = start,
                            "dropped an unrecoverable session line (too long to merge)");
                        continue;
                    }
                }
            }
            dropped += 1;
            tracing::warn!(path = %path.display(), line = start,
                "dropped an unrecoverable session line");
            // 这行自己是完整事件，落到下面正常处理。
        }

        match serde_json::from_str::<SessionEvent>(&line) {
            Ok(event) => events.push(event),
            Err(source) => {
                tracing::warn!(path = %path.display(), line = line_no, %source,
                    "session line is not complete JSON; merging with the next line may fix it");
                pending = Some((line_no, line));
            }
        }
    }
    if let Some((start, _)) = pending {
        dropped += 1;
        tracing::warn!(path = %path.display(), line = start,
            "dropped an unrecoverable session line");
    }
    if merged > 0 || dropped > 0 {
        tracing::warn!(path = %path.display(), merged, dropped,
            "session file contained split or malformed lines");
    }
    Ok(events)
}

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
        let line = encode_event(event)?;
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
/// transcript; `Usage` events accumulate into running totals.
///
/// 历史遗留的半条事件（U+2028 被上游工具当换行劈开）会在这里拼回来；
/// 真正读不出来的行才丢弃，并写进 warn。
pub fn read_session(path: impl AsRef<Path>) -> Result<Session> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut session: Option<Session> = None;
    for (idx, event) in read_events(BufReader::new(file), path)?
        .into_iter()
        .enumerate()
    {
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
            // 压缩是顺序可回放的：每次把**当前**列表最前面的 `replaced` 条
            // 换成摘要。连压多次，逐条应用就能重建出与当时一致的内存状态。
            SessionEvent::Compaction(rec) => {
                let s = session.as_mut().ok_or_else(|| SessionError::MissingMeta {
                    path: path.to_path_buf(),
                })?;
                let replaced = rec.replaced.min(s.messages.len());
                if replaced == 0 {
                    tracing::debug!(line = idx + 1, "compaction record replaced nothing");
                    continue;
                }
                let rest = s.messages.split_off(replaced);
                s.messages.clear();
                s.messages
                    .push(hermes_core::Message::user_text(rec.summary));
                s.messages.extend(rest);
            }
            // 接力也是顺序可回放的：最后一棒就是现在谁在手上。
            // 不另存「当前持有人」——存了就有一天会和文件对不上。
            SessionEvent::Handoff(rec) => {
                let s = session.as_mut().ok_or_else(|| SessionError::MissingMeta {
                    path: path.to_path_buf(),
                })?;
                s.flow.push(rec);
            }
        }
    }

    session.ok_or_else(|| SessionError::MissingMeta {
        path: path.to_path_buf(),
    })
}

/// 只读接力链：扫一遍会话文件，只留 `Handoff` 事件。
///
/// 组会话注定很长（一天一期、日复一日），所以这里**不**走 [`read_session`]——
/// 那条路会把整份转写解析进内存。这里按行过，先看这一行认不认得出是接力。
pub fn read_flow(path: impl AsRef<Path>) -> Result<hermes_core::Flow> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut flow = hermes_core::Flow::default();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !line.contains("\"handoff\"") {
            continue;
        }
        if let Ok(hermes_core::SessionEvent::Handoff(rec)) =
            serde_json::from_str::<hermes_core::SessionEvent>(&line)
        {
            flow.push(rec);
        }
    }
    Ok(flow)
}

/// Sidebar listing: first Meta + whether a human user line exists. Does not
/// load the full transcript.
pub fn read_session_listing(path: impl AsRef<Path>) -> Result<(hermes_core::SessionMeta, bool)> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut meta: Option<hermes_core::SessionMeta> = None;
    let mut has_user = false;
    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        if meta.is_none() {
            match serde_json::from_str::<SessionEvent>(&line) {
                Ok(SessionEvent::Meta(m)) => {
                    meta = Some(m);
                    continue;
                }
                Ok(_) => {}
                Err(source) => {
                    tracing::warn!(line = idx + 1, %source, "skipping malformed session line");
                    continue;
                }
            }
        }
        if !has_user && line_looks_like_human_user(&line) {
            has_user = !line.contains("[lebi-AI") && !line.contains("[Hermes ");
        }
        if meta.is_some() && has_user {
            break;
        }
    }
    let meta = meta.ok_or_else(|| SessionError::MissingMeta {
        path: path.to_path_buf(),
    })?;
    Ok((meta, has_user))
}

fn line_looks_like_human_user(line: &str) -> bool {
    (line.contains("\"role\":\"user\"") || line.contains("\"role\": \"user\""))
        && !line.contains("tool_result")
        && !line.contains("ToolResult")
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
        let meta_line = encode_event(&SessionEvent::Meta(session.meta.clone()))?;
        writeln!(file, "{meta_line}").map_err(|source| SessionError::Io {
            path: tmp.clone(),
            source,
        })?;
        for msg in &session.messages {
            let line = encode_event(&SessionEvent::Message(msg.clone()))?;
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
            let line = encode_event(&SessionEvent::Usage(usage))?;
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
                    let rewritten = encode_event(&SessionEvent::Meta(meta))?;
                    out_lines.push(rewritten);
                    saw_meta = true;
                    continue;
                }
                other => {
                    // Unexpected first event — keep as-is but still fail soft.
                    out_lines.push(encode_event(&other)?);
                    saw_meta = true;
                    continue;
                }
            }
        }
        out_lines.push(escape_line_separators(line.to_string()));
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
        if crate::channel_of_session_path(&path).is_some() {
            continue;
        }
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

    /// 一行 `{"compaction": {...}}`——磁盘上的真实形状。
    fn compaction_event(replaced: usize, summary: &str) -> SessionEvent {
        serde_json::from_str(&format!(
            r#"{{"compaction":{{"replaced":{replaced},"summary":"{summary}","at":"2026-09-16T00:00:00Z"}}}}"#
        ))
        .unwrap()
    }

    /// 压缩记录必须能回放：两次压缩按顺序应用，重建出与当时一致的内存状态。
    /// 不回放的话，重启后会话又变回全长 —— 那等于没修。
    #[test]
    fn compaction_records_fold_prefix_on_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut w = SessionWriter::create(&path).unwrap();
        w.append(&SessionEvent::Meta(SessionMeta::new("m", "stub")))
            .unwrap();
        for i in 1..=6 {
            w.append(&SessionEvent::Message(Message::user_text(format!("m{i}"))))
                .unwrap();
        }
        // 用字面 JSON 构造：既省掉一个 dev-dependency，也顺带钉住磁盘格式。
        w.append(&compaction_event(4, "S1")).unwrap();
        w.append(&SessionEvent::Message(Message::user_text("m7")))
            .unwrap();
        w.append(&SessionEvent::Message(Message::user_text("m8")))
            .unwrap();
        w.append(&compaction_event(2, "S2")).unwrap();
        drop(w);

        let s = read_session(&path).unwrap();
        let got: Vec<String> = s
            .messages
            .iter()
            .map(|m| m.content[0].as_text().unwrap_or_default().to_string())
            .collect();
        assert_eq!(got, vec!["S2", "m6", "m7", "m8"]);
    }

    /// 坏记录（replaced 超过实际条数 / 为 0）不能让会话读不出来。
    #[test]
    fn compaction_record_with_bad_count_is_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut w = SessionWriter::create(&path).unwrap();
        w.append(&SessionEvent::Meta(SessionMeta::new("m", "stub")))
            .unwrap();
        w.append(&SessionEvent::Message(Message::user_text("only")))
            .unwrap();
        w.append(&compaction_event(99, "S")).unwrap();
        w.append(&compaction_event(0, "ignored")).unwrap();
        drop(w);

        let s = read_session(&path).unwrap();
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].content[0].as_text(), Some("S"));
    }

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
        let (head, has_user) = read_session_listing(&path).unwrap();
        assert_eq!(head.id, meta.id);
        assert!(has_user);
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

    /// 用户从微信 / Word 粘过来的文本会带 U+2028 / U+2029；serde 原样写出来
    /// 时，任何按 Unicode 行边界切分的工具都会把这条事件劈成多行。
    /// 落盘必须换成转义 —— 文件里一个原始分隔符都不留。
    #[test]
    fn unicode_line_separators_never_land_raw_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let mut writer = SessionWriter::create(&path).unwrap();
        writer
            .append(&SessionEvent::Meta(SessionMeta::new("m", "p")))
            .unwrap();
        writer
            .append(&SessionEvent::Message(Message::user_text(
                "第一段\u{2028}第二段\u{2029}第三段",
            )))
            .unwrap();
        drop(writer);

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains('\u{2028}'), "原始行分隔符不许落盘");
        assert!(!raw.contains('\u{2029}'), "原始段分隔符不许落盘");
        assert_eq!(raw.lines().count(), 2, "两条事件就是两行，不许被劈开");

        let s = read_session(&path).unwrap();
        assert_eq!(
            s.messages[0].content[0].as_text(),
            Some("第一段\n第二段\n第三段"),
            "语义不变：分隔符还是换行"
        );
    }

    /// 历史文件里被劈开的半条事件要能读回来，不能静默丢掉。
    #[test]
    fn a_line_split_in_two_is_merged_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let meta = serde_json::to_string(&SessionEvent::Meta(SessionMeta::new("m", "p"))).unwrap();
        let broken =
            "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"选材标准\n1 大公司\"}]}}";
        let good =
            serde_json::to_string(&SessionEvent::Message(Message::user_text("收到"))).unwrap();
        std::fs::write(&path, format!("{meta}\n{broken}\n{good}\n")).unwrap();

        let s = read_session(&path).unwrap();
        assert_eq!(s.messages.len(), 2, "半条事件拼回来，后面的消息也没丢");
        assert!(s.messages[0].content[0]
            .as_text()
            .unwrap()
            .contains("选材标准\n1 大公司"));
    }

    /// 一条真的坏行只丢它自己，不许把后面的会话全吞掉。
    #[test]
    fn a_garbage_line_does_not_swallow_the_rest_of_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let meta = serde_json::to_string(&SessionEvent::Meta(SessionMeta::new("m", "p"))).unwrap();
        let one =
            serde_json::to_string(&SessionEvent::Message(Message::user_text("第一句"))).unwrap();
        let two =
            serde_json::to_string(&SessionEvent::Message(Message::user_text("第二句"))).unwrap();
        std::fs::write(&path, format!("{meta}\n这行根本不是 JSON\n{one}\n{two}\n")).unwrap();

        let s = read_session(&path).unwrap();
        assert_eq!(s.messages.len(), 2, "坏行只丢它自己");
    }
}
