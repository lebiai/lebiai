//! Shared chat-channel driver for lebi-AI surfaces (CLI / GUI / IM).
//!
//! Owns everything a channel needs beyond protocol specifics: the [`Channel`]
//! trait, [`ServeCtx`] (engine-wiring snapshot), per-user session persistence,
//! the inbound-turn driver [`serve_inbound`], context assembly
//! ([`ContextSources`]) and the cache-stable system prompt.

pub mod access;
pub mod channel;
pub mod companion_context;
pub mod context;
pub mod system_prompt;

pub use access::{deny_message as channel_deny_message, is_sender_allowed};
pub use channel::{
    handle_text_message, serve_inbound, Channel, ServeCtx, UserState, IM_TOOL_WHITELIST,
};
/// GUI/server companion prompt builder (shared).
pub use companion_context::ContextSources as CompanionContextSources;
pub use context::ContextSources;
pub use system_prompt::{compose_system_prompt, inject_time_header, PromptKind};
