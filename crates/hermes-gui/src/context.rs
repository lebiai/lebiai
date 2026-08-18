//! Re-export shared companion context builder from `hermes-channel`.
//!
//! Historical path: this module used to own a forked copy of the builder.
//! All surfaces that need the GUI-style turn system prompt must use this
//! re-export so Continuity/Care nudges stay identical to hermes-server.

pub use hermes_channel::CompanionContextSources as ContextSources;
