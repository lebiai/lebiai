//! hermes-llm: LLM provider implementations.
//!
//! - [`AnthropicProvider`] — Anthropic Messages API (also DeepSeek's
//!   anthropic-compat endpoint).
//! - [`OpenAiProvider`] — OpenAI Chat Completions API and any compatible
//!   endpoint (DeepSeek `/v1`, Qwen DashScope, OpenRouter, vLLM, ...).

pub mod anthropic;
pub mod config;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use config::{Config, PermissionsConfig, ProviderConfig, ProviderKind};
pub use openai::OpenAiProvider;
