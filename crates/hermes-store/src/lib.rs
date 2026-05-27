//! hermes-store: generic frontmatter document IO + session JSONL writer.
//!
//! Domain layers (`hermes-skills`, `hermes-memory`, `hermes-reflect`) build
//! on top of this. This crate has no opinion about what skills, memories,
//! or session contents *mean*.

pub mod frontmatter;
pub mod session;

pub use frontmatter::{
    parse_doc_str, read_doc, write_doc, write_doc_atomic, FrontmatterDoc, FrontmatterError,
};
pub use session::{list_sessions, read_session, SessionError, SessionWriter};
