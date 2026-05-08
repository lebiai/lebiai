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

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "bash".into(),
        description: "Run a shell command in the workspace directory. Returns stdout, stderr, and exit code.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to execute"},
                "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds (default 120000)"}
            },
            "required": ["command"]
        }),
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
            out.push_str(&format!("\n[exit code: {code}]"));
            Ok(ToolCallOutcome {
                content: out,
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
