//! hermes-tui: ratatui frontend over the same core/LLM/MCP stack as the
//! CLI.
//!
//! Entry point is [`main_loop`]; the binary is a thin wrapper that
//! initialises tracing and calls it.

pub mod app;
pub mod context;
pub mod run;
pub mod ui;
pub mod util;

pub use run::main_loop;
