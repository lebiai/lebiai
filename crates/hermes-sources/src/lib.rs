//! Work materials the user kept so later dialogue can ground in them.
//!
//! Not memories (living rules). Not session uploads (this-turn only).
//! Retrieval hits an in-memory inverted index — never walks the folder
//! on the hot path.

mod focus;
mod index;
mod store;
mod tokenize;

pub use focus::{
    is_followup_query, is_topic_reset, wants_keep, wants_on_hand, wants_other_file,
    wants_remember_standard,
};
pub use store::{
    is_auto_keep_ext, IngestOutcome, SourceHit, SourceItem, SourceMeta, SourceStore,
    SourceStoreError,
};
