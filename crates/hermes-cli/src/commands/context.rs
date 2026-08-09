//! Per-turn context assembly — lives in the shared `hermes-channel` crate.
//! CLI chat / agent / IM channels use `hermes-channel::ContextSources`;
//! the GUI and server keep their own equivalents in their crates.

pub use hermes_channel::ContextSources;
