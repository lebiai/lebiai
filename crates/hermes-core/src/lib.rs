//! hermes-core: agent loop, traits, and events.
//!
//! This crate defines the project's central abstractions. Everything else
//! (LLM providers, MCP transports, stores, UIs) implements or consumes the
//! traits declared here. It must not depend on any UI or transport-specific
//! library.

pub mod banner;
pub mod compaction;
pub mod error;
pub mod message;
pub mod provider;
pub mod session;
pub mod style;
pub mod tool_host;

pub use error::{Error, Result};
pub use message::{ContentBlock, ImageSource, Message, Role};
pub use provider::{
    Capabilities, CompletionRequest, CompletionResponse, LlmProvider, StopReason, StreamEvent,
    ToolSpec, Usage,
};
pub use session::{Session, SessionEvent, SessionMeta};
pub use tool_host::{NullToolHost, ToolCallOutcome, ToolHost};
