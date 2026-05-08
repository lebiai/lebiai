//! `reflect()` — the actual call into an LLM, with structured-JSON parsing.

use hermes_core::{CompletionRequest, LlmProvider, Message, Session};
use hermes_memory::LoadedMemory;
use hermes_skills::LoadedSkill;
use thiserror::Error;

use crate::output::ReflectionOutput;
use crate::prompt::{system_prompt, user_prompt};

#[derive(Debug, Error)]
pub enum ReflectError {
    #[error("provider call failed: {0}")]
    Provider(String),

    #[error("could not parse reflection JSON: {error}\n--- raw response ---\n{raw}")]
    ParseFailed { error: String, raw: String },
}

pub type Result<T> = std::result::Result<T, ReflectError>;

const REFLECT_MAX_TOKENS: u32 = 8192;

/// Run one reflection pass.
///
/// Returns an `Option<ReflectionOutput>`:
/// - `Some` on a successful round-trip and parse
/// - never `None` from this function — failure is an `Err`. The Option exists
///   in case future extensions allow "no reflection needed" early-exit.
pub async fn reflect(
    provider: &dyn LlmProvider,
    session: &Session,
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
) -> Result<ReflectionOutput> {
    let system = system_prompt();
    let user = user_prompt(session, skills, memories);

    let req = CompletionRequest {
        model: String::new(), // provider uses its configured default
        system: Some(system),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: REFLECT_MAX_TOKENS,
        // Low temperature for structured output; we want determinism over
        // creativity here.
        temperature: Some(0.2),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text();
    let json_str = strip_code_fence(&text);

    serde_json::from_str(json_str).map_err(|e| ReflectError::ParseFailed {
        error: e.to_string(),
        raw: text,
    })
}

/// If the model wrapped its JSON in a ```json ... ``` fence, peel it. Also
/// trims leading/trailing whitespace and any prose before the first `{`.
/// Public alias for micro.rs to reuse.
pub fn strip_code_fence_pub(s: &str) -> &str {
    strip_code_fence(s)
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();

    // ```json ... ``` or ``` ... ```
    if let Some(rest) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        let rest = rest.trim_start();
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
        return rest;
    }

    // Scan to the first `{` if there's prose preamble.
    if let Some(start) = s.find('{') {
        if start > 0 {
            return &s[start..];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_code_fence_handles_plain_json() {
        let s = r#"{"summary":"x"}"#;
        assert_eq!(strip_code_fence(s), s);
    }

    #[test]
    fn strip_code_fence_handles_json_fence() {
        let s = "```json\n{\"summary\":\"x\"}\n```";
        assert_eq!(strip_code_fence(s), r#"{"summary":"x"}"#);
    }

    #[test]
    fn strip_code_fence_handles_bare_fence() {
        let s = "```\n{\"summary\":\"x\"}\n```";
        assert_eq!(strip_code_fence(s), r#"{"summary":"x"}"#);
    }

    #[test]
    fn strip_code_fence_handles_prose_preamble() {
        let s = "Here is your JSON:\n{\"summary\":\"x\"}";
        assert_eq!(strip_code_fence(s), r#"{"summary":"x"}"#);
    }

    #[test]
    fn parses_minimal_output() {
        let raw = r#"{"summary":"hello","skill_candidates":[],"memory_candidates":[],"conflicts":[]}"#;
        let out: ReflectionOutput = serde_json::from_str(raw).unwrap();
        assert_eq!(out.summary, "hello");
        assert!(out.is_empty());
    }
}
