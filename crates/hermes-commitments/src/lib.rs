//! Open work (在办): debts that still owe after the conversation stops.
//!
//! Not session scratch (`todo_write`), not living rules (memory), not skills.

pub mod due;
pub mod near;
pub mod residue;
pub mod review;
pub mod store;

pub use due::{is_content_deliverable, parse_due, DueError};
pub use near::{score_near, NearHit, NEAR_ASK, NEAR_FOLD};
pub use residue::{
    looks_like_owe_language, scan_merge_pairs, scan_residue, session_has_owe_language, ResidueItem,
};
pub use review::{
    body_blank, evidence_user_prompt, fallback_review, format_markdown, invite_due, listed_reviews,
    load_prefs, parse_review_json, parse_review_md, reviewed_span, save_prefs, span_range,
    today_local, weekday_today, write_review_file, ReviewIndexEntry, ReviewJson, ReviewPrefs,
    REVIEW_SYSTEM,
};
pub use store::{
    standard_path, Commitment, CommitmentError, CommitmentStore, SaveMode, SaveOutcome, Source,
    Status, INDEX_CAP, OPEN_CROWD, SUGGESTED_TTL_DAYS,
};
