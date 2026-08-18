//! hermes-core: agent loop, traits, and events.
//!
//! This crate defines the project's central abstractions. Everything else
//! (LLM providers, MCP transports, stores, UIs) implements or consumes the
//! traits declared here. It must not depend on any UI or transport-specific
//! library.

pub mod banner;
pub mod compaction;
pub mod companion;
pub mod error;
pub mod license;
pub mod message;
pub mod paths;
pub mod provider;
pub mod session;
pub mod style;
pub mod tool_host;
pub mod workspace_hygiene;

pub use error::{Error, Result};
pub use license::{
    apply_token, can_use_main, dev_has_license_backup, dev_restore_license_backup,
    dev_simulate_expired, dev_tools_enabled, load_status, mark_nudge_seen, LicenseError,
    LicensePhase, LicenseStatus, LicenseUrgency, PRODUCT_ID, TRIAL_DAYS, WECHAT_CONTACT,
};
pub use message::{sanitize_history_for_provider, ContentBlock, ImageSource, Message, Role};
pub use paths::{
    clear_data_dir_pointer, data_dir_pointer_path, data_path, data_root, ensure_data_root,
    is_user_chosen_data_root, maybe_migrate_data_root, project_data_dirname, read_data_dir_pointer,
    write_data_dir_pointer, DEFAULT_DATA_DIRNAME, ENV_DATA_DIR, LEGACY_DATA_DIRNAME,
    WORKSPACE_OUTPUTS_DIR,
};
pub use provider::{
    Capabilities, CompletionRequest, CompletionResponse, LlmProvider, StopReason, StreamEvent,
    ToolSpec, Usage,
};
pub use session::{
    derive_title_from_messages, is_trivial_user_text, last_human_send, session_has_user_text,
    truncate_title, Session, SessionEvent, SessionMeta, DEFAULT_SESSION_TITLE,
};
pub use tool_host::{ToolCallOutcome, ToolHost};
pub use workspace_hygiene::quarantine_lawyer_workspace_files;
