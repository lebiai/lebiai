//! `bash` — run a shell command inside the workspace.

use std::path::Path;
use std::time::Duration;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct Args {
    command: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 {
    120_000
}

/// Maximum characters of combined stdout+stderr returned to the model.
/// Output beyond this is elided head+tail so the model still sees how a
/// command started and how it ended, without flooding the context window.
const MAX_OUTPUT_CHARS: usize = 30_000;

/// Cap `s` to at most `max` chars, keeping a head and a tail with an elision
/// marker in the middle. Favors the tail, where errors and exit summaries
/// usually live. Mirrors `hermes_core::compaction`'s preview helper; kept
/// local to avoid a cross-crate dependency for one small function.
fn cap_output(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let head_chars = max * 2 / 3;
    let tail_chars = max - head_chars;
    let head: String = chars.iter().take(head_chars).collect();
    let tail: String = chars.iter().skip(chars.len() - tail_chars).collect();
    let elided = chars.len() - head_chars - tail_chars;
    format!("{head}\n…[{elided} chars elided — output capped at {max}]…\n{tail}")
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "bash".into(),
        description:
            "Run a shell command in the workspace directory. Returns stdout, stderr, and exit code."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to execute"},
                "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds (default 120000)"}
            },
            "required": ["command"]
        }),
        // Default open; high-risk commands are gated in hermes_turn::danger.
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("bash: bad args: {e}")))?;

    let timeout = Duration::from_millis(a.timeout_ms.min(600_000));
    let result = tokio::time::timeout(timeout, async {
        Command::new("sh")
            .arg("-c")
            .arg(&a.command)
            .current_dir(workspace)
            .output()
            .await
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code().unwrap_or(-1);
            let mut out = String::new();
            if !stdout.is_empty() {
                out.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("[stderr]\n");
                out.push_str(&stderr);
            }
            if out.is_empty() {
                out.push_str("(no output)");
            }
            // Cap combined stdout+stderr so a runaway command (e.g. `find /`)
            // can't flood the model's context. The exit-code line is appended
            // after the cap so it is never elided.
            let out = cap_output(&out, MAX_OUTPUT_CHARS);
            Ok(ToolCallOutcome {
                content: format!("{out}\n[exit code: {code}]"),
                is_error: code != 0,
            })
        }
        Ok(Err(e)) => Ok(ToolCallOutcome {
            content: format!("bash exec error: {e}"),
            is_error: true,
        }),
        Err(_) => Ok(ToolCallOutcome {
            content: format!("bash timeout after {}ms", a.timeout_ms),
            is_error: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_passes_through_short_output() {
        let s = "short output\n[stderr]\nwarn";
        assert_eq!(cap_output(s, MAX_OUTPUT_CHARS), s);
    }

    #[test]
    fn cap_elides_long_output_head_and_tail() {
        let s = format!("{}{}", "H".repeat(50_000), "T".repeat(50_000));
        let capped = cap_output(&s, 30_000);
        assert!(capped.contains("chars elided"));
        // Stays within the cap plus the short marker line.
        assert!(
            capped.chars().count() < 31_000,
            "len {}",
            capped.chars().count()
        );
        // Head from the start, tail from the end are both preserved.
        assert!(capped.starts_with("HHH"));
        assert!(capped.ends_with("TTT"));
    }
}
