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
/// Leave-session / background path: smaller budget so the call finishes sooner.
const REFLECT_MAX_TOKENS_QUICK: u32 = 3072;

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
    reflect_with_max_tokens(provider, session, skills, memories, REFLECT_MAX_TOKENS).await
}

/// Faster reflection for non-blocking leave-session / background jobs.
pub async fn reflect_quick(
    provider: &dyn LlmProvider,
    session: &Session,
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
) -> Result<ReflectionOutput> {
    reflect_with_max_tokens(
        provider,
        session,
        skills,
        memories,
        REFLECT_MAX_TOKENS_QUICK,
    )
    .await
}

async fn reflect_with_max_tokens(
    provider: &dyn LlmProvider,
    session: &Session,
    skills: &[LoadedSkill],
    memories: &[LoadedMemory],
    max_tokens: u32,
) -> Result<ReflectionOutput> {
    let system = system_prompt();
    let user = user_prompt(session, skills, memories);

    let req = CompletionRequest {
        model: String::new(), // provider uses its configured default
        system: Some(system),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens,
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

    match serde_json::from_str(json_str) {
        Ok(out) => Ok(crate::episode::finalize_reflection_output_with(
            out, memories,
        )),
        Err(first_err) => {
            // Try to repair truncated JSON by closing unclosed brackets.
            if let Some(repaired) = repair_truncated_json(json_str) {
                if let Ok(out) = serde_json::from_str(&repaired) {
                    tracing::info!("recovered truncated reflection JSON");
                    return Ok(crate::episode::finalize_reflection_output_with(
                        out, memories,
                    ));
                }
            }
            Err(ReflectError::ParseFailed {
                error: first_err.to_string(),
                raw: text,
            })
        }
    }
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

/// Attempt to repair a truncated JSON object by closing unclosed brackets.
/// Strategy: find the last complete element boundary (`,` or `[` or `{` at the
/// top nesting level), truncate there, then close all open brackets.
/// Returns None if the input doesn't look like a partial JSON object.
pub fn repair_truncated_json(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with('{') {
        return None;
    }

    // Walk the string tracking nesting depth and string state.
    // Record positions where we could safely truncate (after a complete value).
    let mut curly = 0i32;
    let mut square = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut last_safe_cut: Option<usize> = None;

    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        let ch_len = ch.len_utf8();

        if escape {
            escape = false;
            i += ch_len;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            i += ch_len;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            i += ch_len;
            continue;
        }
        if in_string {
            i += ch_len;
            continue;
        }
        match ch {
            '{' => {
                curly += 1;
                // Opening brace of a top-level object value is a safe cut point
                // (we'll drop this incomplete element).
                if curly == 2 && square == 1 {
                    // This is the start of an element inside the conflicts/memory_candidates array.
                    // The position *before* this is safe (previous element ended with comma or `[`).
                }
            }
            '}' => {
                curly -= 1;
                // After closing a nested object, the next comma or closing bracket is safe.
                if curly == 1 {
                    // We just closed an element inside the top-level object.
                    // Look ahead for comma — if found, cut point is after it.
                    let rest = &s[i + ch_len..].trim_start();
                    if rest.starts_with(',') {
                        last_safe_cut = Some(i + ch_len + 1); // after the comma
                    } else if rest.starts_with('}') {
                        last_safe_cut = Some(i + ch_len); // before closing brace
                    }
                }
            }
            '[' => {
                square += 1;
            }
            ']' => {
                square -= 1;
                if square == 0 && curly == 1 {
                    // Just closed an array — safe cut point.
                    let rest = &s[i + ch_len..].trim_start();
                    if rest.starts_with(',') {
                        last_safe_cut = Some(i + ch_len + 1);
                    } else if rest.starts_with('}') {
                        last_safe_cut = Some(i + ch_len);
                    }
                }
            }
            _ => {}
        }
        i += ch_len;
    }

    if curly <= 0 && square <= 0 {
        return None; // already balanced
    }

    // If we found a safe cut point, truncate there and close brackets.
    // Otherwise, fall back to brute-force bracket closing.
    let mut repaired = if let Some(cut) = last_safe_cut {
        let r = s[..cut].to_string();
        // Trailing comma is fine in the cut — we'll close brackets next.
        r.trim_end().to_string()
    } else {
        s.to_string()
    };

    // Recount unclosed brackets in the truncated string.
    let mut c2 = 0i32;
    let mut s2 = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for ch in repaired.chars() {
        if esc {
            esc = false;
            continue;
        }
        if ch == '\\' && in_str {
            esc = true;
            continue;
        }
        if ch == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match ch {
            '{' => c2 += 1,
            '}' => c2 -= 1,
            '[' => s2 += 1,
            ']' => s2 -= 1,
            _ => {}
        }
    }

    // Remove trailing comma before closing brackets.
    if let Some(stripped) = repaired.trim_end().strip_suffix(',') {
        repaired = stripped.to_string();
    }

    for _ in 0..s2.max(0) {
        repaired.push(']');
    }
    for _ in 0..c2.max(0) {
        repaired.push('}');
    }
    Some(repaired)
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
        let raw =
            r#"{"summary":"hello","skill_candidates":[],"memory_candidates":[],"conflicts":[]}"#;
        let out: ReflectionOutput = serde_json::from_str(raw).unwrap();
        assert_eq!(out.summary, "hello");
        assert!(out.is_empty());
    }

    #[test]
    fn repair_truncated_conflicts_array() {
        // Simulates the real failure: conflicts array opened but not closed
        let truncated =
            r#"{"summary":"test","skill_candidates":[],"memory_candidates":[],"conflicts": [{"#;
        let repaired = repair_truncated_json(truncated).unwrap();
        let out: ReflectionOutput = serde_json::from_str(&repaired).unwrap();
        assert_eq!(out.summary, "test");
        assert!(out.conflicts.is_empty()); // incomplete element dropped
    }

    #[test]
    fn repair_truncated_mid_object() {
        // Truncated in the middle of a memory candidate object
        let truncated =
            r#"{"summary":"test","skill_candidates":[],"memory_candidates":[{"fact":"x"#;
        let repaired = repair_truncated_json(truncated).unwrap();
        let out: ReflectionOutput = serde_json::from_str(&repaired).unwrap();
        assert_eq!(out.summary, "test");
        assert!(out.memory_candidates.is_empty()); // incomplete element dropped
    }

    #[test]
    fn repair_truncated_after_complete_element() {
        // One complete conflict, then truncated second one
        let truncated = r#"{"summary":"test","skill_candidates":[],"memory_candidates":[],"conflicts": [{"with":"mem_1","kind":"stale","explain":"old","options":[]},{"#;
        let repaired = repair_truncated_json(truncated).unwrap();
        let out: ReflectionOutput = serde_json::from_str(&repaired).unwrap();
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].with, "mem_1");
    }

    #[test]
    fn repair_returns_none_for_balanced_json() {
        let balanced =
            r#"{"summary":"x","skill_candidates":[],"memory_candidates":[],"conflicts":[]}"#;
        assert!(repair_truncated_json(balanced).is_none());
    }

    #[test]
    fn repair_returns_none_for_non_object() {
        assert!(repair_truncated_json("not json").is_none());
    }
}
