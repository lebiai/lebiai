//! hermes-store: generic frontmatter document IO + session JSONL writer.
//!
//! Domain layers (`hermes-skills`, `hermes-memory`, `hermes-reflect`) build
//! on top of this. This crate has no opinion about what skills, memories,
//! or session contents *mean*.

pub mod frontmatter;
pub mod path_guard;
pub mod session;
pub mod session_days;

pub use frontmatter::{
    parse_doc_str, read_doc, write_doc, write_doc_atomic, FrontmatterDoc, FrontmatterError,
};
pub use path_guard::{ensure_session_path, sessions_root};
pub use session::{
    channel_of_session_path, list_sessions, purge_empty_sessions, read_flow, read_session,
    read_session_listing, rewrite_session, update_session_title, SessionError, SessionWriter,
};
pub use session_days::{
    day_groups, day_of, is_human_turn, label_for, recall_in_session, window_day, window_split,
    DayGroup, RecallHit, DEFAULT_WINDOW,
};
