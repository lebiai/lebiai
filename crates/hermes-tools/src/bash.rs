//! `bash` — run a shell command inside the workspace (sandboxed when possible).

use std::path::Path;
use std::time::Duration;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::bash_sandbox::{sandboxed_shell, SandboxMode};

#[derive(Deserialize)]
struct Args {
    command: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 {
    120_000
}

const MAX_OUTPUT_CHARS: usize = 30_000;

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
        description: "Run a shell command in the workspace directory (OS sandbox when available: \
            macOS seatbelt / Linux bwrap). File writes outside the workspace are blocked by the \
            sandbox when active. Returns stdout, stderr, exit code, and sandbox mode. \
            Do NOT use bash to open files, videos, or web pages — use the `open` tool. \
            Do NOT guess Microsoft Word / Pages / WPS via `open -a` or osascript."
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

    if let Some(target) = intercept_open_target(&a.command) {
        return crate::open::run(workspace, serde_json::json!({ "target": target })).await;
    }
    if looks_like_app_open_workaround(&a.command) {
        return Ok(ToolCallOutcome {
            content: "Use the `open` tool with the file path or https URL. \
                      Do not launch Word / Pages / WPS / Finder from bash."
                .into(),
            is_error: true,
        });
    }

    let timeout = Duration::from_millis(a.timeout_ms.min(600_000));
    let (mut cmd, mode) = sandboxed_shell(workspace, &a.command);
    let result = tokio::time::timeout(timeout, cmd.output()).await;

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
            let out = cap_output(&out, MAX_OUTPUT_CHARS);
            let sandbox_note = if mode == SandboxMode::Unsandboxed {
                format!(
                    "\n[{}] writes outside workspace are NOT OS-enforced on this platform",
                    mode.label()
                )
            } else {
                format!("\n[{}]", mode.label())
            };
            Ok(ToolCallOutcome {
                content: format!("{out}\n[exit code: {code}]{sandbox_note}"),
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

/// If `command` is just the OS opener (`open` / `xdg-open` / `start`), return
/// the file or URL so we can run the unsandboxed `open` tool instead.
fn intercept_open_target(command: &str) -> Option<String> {
    let tokens = tokenize_shellish(strip_leading_env(command.trim()));
    if tokens.is_empty() {
        return None;
    }
    let prog = tokens[0]
        .rsplit('/')
        .next()
        .unwrap_or(&tokens[0])
        .to_ascii_lowercase();
    let mut i = 1usize;
    match prog.as_str() {
        "open" | "xdg-open" => {}
        "start" => {}
        "cmd" => {
            let next = tokens.get(1).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
            if next != "/c" && next != "/k" {
                return None;
            }
            let third = tokens.get(2).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
            if third != "start" {
                return None;
            }
            i = 3;
        }
        _ => return None,
    }
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "-a" || t == "-b" || t == "--args" {
            i += 2;
            continue;
        }
        if t.starts_with('-') || t.is_empty() {
            i += 1;
            continue;
        }
        return Some(t.clone());
    }
    None
}

fn looks_like_app_open_workaround(command: &str) -> bool {
    let c = command.to_ascii_lowercase();
    if !(c.contains("osascript") || c.contains("osacompile") || c.contains("finder")) {
        return false;
    }
    c.contains("word")
        || c.contains("pages")
        || c.contains("wps")
        || c.contains("excel")
        || c.contains("microsoft")
        || c.contains("open")
}

fn strip_leading_env(cmd: &str) -> &str {
    let mut rest = cmd;
    while let Some(eq) = rest.find('=') {
        let key = &rest[..eq];
        if key.is_empty()
            || key.contains(char::is_whitespace)
            || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            break;
        }
        let after = &rest[eq + 1..];
        let skip = if let Some(stripped) = after.strip_prefix('"') {
            stripped.find('"').map(|i| i + 2)
        } else {
            after.find(char::is_whitespace)
        };
        match skip {
            Some(n) => rest = after[n..].trim_start(),
            None => return "",
        }
    }
    rest
}

fn tokenize_shellish(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
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
        assert!(capped.chars().count() < 31_000);
        assert!(capped.starts_with("HHH"));
        assert!(capped.ends_with("TTT"));
    }

    #[tokio::test]
    async fn sandbox_echo_works() {
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"command": "echo lebi-sandbox-ok"}),
        )
        .await
        .unwrap();
        assert!(out.content.contains("lebi-sandbox-ok"), "{}", out.content);
        assert!(!out.is_error);
        assert!(out.content.contains("sandbox="));
    }

    #[test]
    fn intercepts_macos_open_and_word_workaround() {
        assert_eq!(
            intercept_open_target("open ~/Desktop/a.docx").as_deref(),
            Some("~/Desktop/a.docx")
        );
        assert_eq!(
            intercept_open_target("open -a \"Microsoft Word\" ~/Desktop/a.docx").as_deref(),
            Some("~/Desktop/a.docx")
        );
        assert_eq!(
            intercept_open_target("xdg-open https://example.com/x").as_deref(),
            Some("https://example.com/x")
        );
        assert!(intercept_open_target("python3 -c \"open('x')\"").is_none());
        assert!(intercept_open_target("ls -la").is_none());
        assert!(looks_like_app_open_workaround(
            "osascript -e 'tell application \"Microsoft Word\" to open POSIX file \"/tmp/a.docx\"'"
        ));
        assert!(!looks_like_app_open_workaround("echo hello"));
    }

    #[tokio::test]
    async fn bash_open_missing_file_does_not_hit_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"command": "open outputs/missing.docx"}),
        )
        .await
        .unwrap();
        assert!(out.is_error);
        assert!(
            out.content.contains("does not exist") || out.content.contains("open:"),
            "{}",
            out.content
        );
        assert!(!out.content.contains("LSOpenURLsWithCompletionHandler"));
        assert!(!out.content.contains("sandbox="));
    }
}
