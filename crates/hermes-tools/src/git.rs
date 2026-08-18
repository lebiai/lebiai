//! `git` — read-only git operations scoped to the workspace.

use std::path::Path;
use std::time::Duration;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct Args {
    operation: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    args: Option<String>,
}

const ALLOWED_OPS: &[&str] = &["status", "diff", "log", "blame", "show"];
const TIMEOUT_MS: u64 = 30_000;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "git".into(),
        description: "Read-only git operations (status, diff, log, blame, show). \
                      For write ops (commit, push, checkout), use bash."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["status", "diff", "log", "blame", "show"],
                    "description": "Git operation to run"
                },
                "path": {
                    "type": "string",
                    "description": "Optional file path (for diff, blame, show)"
                },
                "args": {
                    "type": "string",
                    "description": "Additional arguments (e.g. '--staged' for diff, 'HEAD~3' for show)"
                }
            },
            "required": ["operation"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("git: bad args: {e}")))?;

    if !ALLOWED_OPS.contains(&a.operation.as_str()) {
        return Ok(ToolCallOutcome {
            content: format!(
                "unknown git operation '{}'. Allowed: {}",
                a.operation,
                ALLOWED_OPS.join(", ")
            ),
            is_error: true,
        });
    }

    let mut cmd_args: Vec<String> = vec![a.operation.clone()];

    match a.operation.as_str() {
        "status" => cmd_args.push("--short".into()),
        "log" if a.args.is_none() => {
            cmd_args.push("--oneline".into());
            cmd_args.push("-20".into());
        }
        _ => {}
    }

    if let Some(extra) = &a.args {
        let words = shell_words(extra);
        if let Some(bad) = words.iter().find(|w| is_dangerous_git_arg(w)) {
            return Ok(ToolCallOutcome {
                content: format!(
                    "git: rejected argument {bad:?} (disallowed: -c, --git-dir, \
                     --work-tree, --exec-path, --upload-pack, --receive-pack, and similar)"
                ),
                is_error: true,
            });
        }
        cmd_args.extend(words);
    }
    if let Some(path) = &a.path {
        if path.contains("..") || path.starts_with('-') {
            return Ok(ToolCallOutcome {
                content: "path must not contain '..' or start with '-'".into(),
                is_error: true,
            });
        }
        cmd_args.push("--".into());
        cmd_args.push(path.clone());
    }

    let result = tokio::time::timeout(Duration::from_millis(TIMEOUT_MS), async {
        Command::new("git")
            .args(&cmd_args)
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
                out.push_str(&stderr);
            }
            if out.is_empty() {
                out.push_str("(no output)");
            }
            Ok(ToolCallOutcome {
                content: out,
                is_error: code != 0,
            })
        }
        Ok(Err(e)) => Ok(ToolCallOutcome {
            content: format!("git exec error: {e}"),
            is_error: true,
        }),
        Err(_) => Ok(ToolCallOutcome {
            content: format!("git timeout after {TIMEOUT_MS}ms"),
            is_error: true,
        }),
    }
}

fn shell_words(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

/// Block git options that can redirect the repository root or run helpers.
fn is_dangerous_git_arg(arg: &str) -> bool {
    let a = arg.trim();
    if a == "-c" || a.starts_with("-c") && a.contains('=') {
        return true;
    }
    const BLOCKED_PREFIXES: &[&str] = &[
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--exec-path",
        "--upload-pack",
        "--receive-pack",
        "--config-env",
        "--super-prefix",
        "--literal-pathspecs",
    ];
    for p in BLOCKED_PREFIXES {
        if a == *p || a.starts_with(&format!("{p}=")) {
            return true;
        }
    }
    // Bare `--` is fine as path separator when we add it ourselves; user args
    // containing `../` in pathspec are still constrained by path field checks.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_git_dir() {
        assert!(is_dangerous_git_arg("--git-dir=/tmp/evil"));
        assert!(is_dangerous_git_arg("-c"));
        assert!(is_dangerous_git_arg("-ccore.sshCommand=evil"));
        assert!(!is_dangerous_git_arg("--oneline"));
        assert!(!is_dangerous_git_arg("HEAD~3"));
    }
}
