//! TUI application state.
//!
//! The data here is everything the render layer consumes. The event loop
//! mutates it and then calls `draw`. Nothing in here knows about tokio,
//! crossterm, or LLM providers — all IO lives in `run.rs`.

use hermes_memory::LoadedMemory;
use hermes_reflect::{ConflictCandidate, MemoryCandidate, SkillCandidate};
use hermes_skills::LoadedSkill;

/// One line in the scrollback transcript.
#[derive(Debug, Clone)]
pub enum Entry {
    /// User's own prompt (echoed after Enter).
    User(String),
    /// Assistant text reply. Accumulates via [`App::extend_assistant`].
    Assistant(String),
    /// Assistant tool invocation announcement (one per tool_use).
    Tool(String),
    /// Informational banner (session started, command output, error).
    Info(String),
    /// Assistant "thinking" indicator — replaced/removed when text arrives.
    Thinking,
}

/// Streaming state for the current in-flight turn, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Streaming {
    Idle,
    Thinking,
    Text,
}

/// Top-level UI mode. `Normal` is the chat REPL; `Reflect` walks a
/// queue of reflection candidates produced at session end or via
/// the `/reflect` command. `Confirm` pauses for tool-call approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Reflect,
    ReflectWaiting,
    Confirm,
}

/// A single reflection candidate awaiting user action.
#[derive(Debug, Clone)]
pub enum Candidate {
    Skill(SkillCandidate),
    /// A memory candidate plus the single conflict (if any) that
    /// references any of its `supersedes` ids. Presentation differs.
    Memory {
        cand: MemoryCandidate,
        conflict: Option<(String, ConflictCandidate)>,
    },
    /// A conflict that no memory candidate claimed — surfaced last
    /// so the user can acknowledge it.
    OrphanConflict(ConflictCandidate),
}

/// What the footer shows right now.
pub struct Status {
    pub model: String,
    pub tools: usize,
    pub memories: usize,
    pub pinned: usize,
    pub skills: usize,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub session_path: String,
}

pub struct App {
    pub entries: Vec<Entry>,
    pub input: String,
    /// Byte offset of the insertion point within `input`.
    pub cursor: usize,
    /// Offset from the bottom of the transcript (0 = pinned to bottom).
    pub scroll: u16,
    pub streaming: Streaming,
    pub status: Status,
    pub should_quit: bool,
    /// Cached snapshots from disk so the event loop can reassemble system
    /// prompts per turn without re-reading the filesystem.
    pub skills: Vec<LoadedSkill>,
    pub active_memories: Vec<LoadedMemory>,
    pub pinned_memories: Vec<LoadedMemory>,

    pub mode: Mode,
    pub candidates: std::collections::VecDeque<Candidate>,
    /// Set when quit was requested and the remaining pipeline (reflect,
    /// drain streams) should run before the event loop exits.
    pub quit_after_reflect: bool,
    /// Set by `/reflect` slash or quit-driven path; the event loop drains
    /// it on each iteration to spawn the reflection task.
    pub reflect_pending: bool,
    /// True when the pending reflection was triggered explicitly by the
    /// user (e.g. `/reflect`); skips the min_turns gate.
    pub reflect_force: bool,
    /// Pending tool confirmation: the oneshot sender to reply Allow/Deny,
    /// plus a human-readable summary for the prompt.
    pub pending_confirm: Option<(tokio::sync::oneshot::Sender<hermes_turn::ConfirmAction>, String)>,
}

impl App {
    pub fn push_info(&mut self, msg: impl Into<String>) {
        self.entries.push(Entry::Info(msg.into()));
    }

    pub fn push_user(&mut self, text: String) {
        self.entries.push(Entry::User(text));
    }

    pub fn push_tool(&mut self, text: String) {
        self.entries.push(Entry::Tool(text));
    }

    pub fn begin_thinking(&mut self) {
        if !matches!(self.entries.last(), Some(Entry::Thinking)) {
            self.entries.push(Entry::Thinking);
        }
        self.streaming = Streaming::Thinking;
    }

    /// Append text to the current assistant entry; create one if the last
    /// entry isn't an Assistant block. Replaces a trailing Thinking entry.
    pub fn extend_assistant(&mut self, delta: &str) {
        if matches!(self.entries.last(), Some(Entry::Thinking)) {
            self.entries.pop();
        }
        match self.entries.last_mut() {
            Some(Entry::Assistant(buf)) => buf.push_str(delta),
            _ => self.entries.push(Entry::Assistant(delta.to_string())),
        }
        self.streaming = Streaming::Text;
    }

    pub fn end_streaming(&mut self) {
        if matches!(self.entries.last(), Some(Entry::Thinking)) {
            self.entries.pop();
        }
        self.streaming = Streaming::Idle;
    }

    // ---- input buffer ----

    pub fn input_insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Move one char left, accounting for UTF-8 boundaries.
        let new = self.input[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input.drain(new..self.cursor);
        self.cursor = new;
    }

    pub fn input_clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    pub fn take_input(&mut self) -> String {
        let s = self.input.trim().to_string();
        self.input_clear();
        s
    }

    pub fn cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.input[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn cursor_right(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let rest = &self.input[self.cursor..];
        let next = rest
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.input.len());
        self.cursor = next;
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    pub fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_add(n);
    }
    pub fn scroll_down(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }
    pub fn scroll_bottom(&mut self) {
        self.scroll = 0;
    }
}
