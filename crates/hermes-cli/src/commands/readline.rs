//! Line editor for the chat REPL.
//!
//! Wraps rustyline in an async-friendly facade: the blocking readline call
//! is moved to a spawn_blocking task so the tokio runtime stays free. The
//! editor is kept across calls so history, completion state, and the
//! terminal handle are preserved.
//!
//! Non-TTY stdin (piped / redirected input) is auto-detected by rustyline
//! and reads act like plain line reads — keeps piped smoke-tests working.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::Editor;

/// Outcome of one read attempt.
pub enum LineOutcome {
    /// A line was entered (already trimmed).
    Line(String),
    /// User pressed Ctrl-C on an empty prompt; treat as "cancel this line".
    Interrupted,
    /// EOF (Ctrl-D / stdin closed). Caller should exit the loop.
    Eof,
}

pub struct ChatLineEditor {
    inner: Option<Editor<(), FileHistory>>,
    history_path: Option<PathBuf>,
}

impl ChatLineEditor {
    pub fn new() -> Result<Self> {
        let inner = Editor::<(), FileHistory>::new().context("creating rustyline editor")?;
        let history_path = dirs::home_dir()
            .map(|h| h.join(".small-rust-hermes").join("history"));

        let mut this = Self {
            inner: Some(inner),
            history_path,
        };
        if let (Some(ed), Some(p)) = (this.inner.as_mut(), this.history_path.as_ref()) {
            if p.exists() {
                if let Err(e) = ed.load_history(p) {
                    tracing::debug!(error=%e, "rustyline: no prior history loaded");
                }
            }
        }
        Ok(this)
    }

    /// Read one line, prompting with `prompt`. Runs the blocking readline
    /// on a spawn_blocking task to avoid stalling the tokio runtime.
    pub async fn readline(&mut self, prompt: &str) -> Result<LineOutcome> {
        let mut editor = self
            .inner
            .take()
            .ok_or_else(|| anyhow::anyhow!("editor missing — prior call panicked?"))?;
        let prompt = prompt.to_string();

        let (editor, result) = tokio::task::spawn_blocking(move || {
            let r = editor.readline(&prompt);
            (editor, r)
        })
        .await
        .context("rustyline spawn_blocking join")?;

        self.inner = Some(editor);

        match result {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    if let Some(ed) = self.inner.as_mut() {
                        let _ = ed.add_history_entry(&trimmed);
                    }
                }
                Ok(LineOutcome::Line(trimmed))
            }
            Err(ReadlineError::Interrupted) => Ok(LineOutcome::Interrupted),
            Err(ReadlineError::Eof) => Ok(LineOutcome::Eof),
            Err(e) => Err(anyhow::anyhow!("readline: {e}")),
        }
    }

    /// Persist history. Best-effort — failures are logged, not returned.
    pub fn save_history(&mut self) {
        if let (Some(ed), Some(p)) = (self.inner.as_mut(), self.history_path.as_ref()) {
            if let Some(parent) = p.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!(error=%e, "rustyline: could not create history dir");
                    return;
                }
            }
            if let Err(e) = ed.save_history(p) {
                tracing::warn!(error=%e, "rustyline: save_history failed");
            }
        }
    }
}
