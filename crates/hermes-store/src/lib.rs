//! hermes-store: generic frontmatter document IO + session JSONL writer.
//!
//! Domain layers (`hermes-skills`, `hermes-memory`, `hermes-reflect`) build
//! on top of this. This crate has no opinion about what skills, memories,
//! or session contents *mean*.

pub mod frontmatter;
pub mod path_guard;
pub mod session;

pub use frontmatter::{
    parse_doc_str, read_doc, write_doc, write_doc_atomic, FrontmatterDoc, FrontmatterError,
};
pub use path_guard::{ensure_session_path, sessions_root};
pub use session::{
    channel_of_session_path, list_sessions, purge_empty_sessions, read_session, rewrite_session,
    update_session_title, SessionError, SessionWriter,
};
