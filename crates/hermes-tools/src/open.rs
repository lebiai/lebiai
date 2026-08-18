//! `open` — open a local file/folder or an http(s) URL with the OS default app.
//!
//! Runs **outside** the bash sandbox. Guessing Word / WPS / Pages is not this
//! tool's job; Launch Services / xdg-open / start pick the handler.

use std::path::Path;
use std::process::Command;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;
use crate::url_safety::validate_public_http_url;

#[derive(Deserialize)]
struct Args {
    /// Local path (workspace, `~/Desktop/…`, Desktop/Documents/Downloads)
    /// or `http(s)://…` page.
    target: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "open".into(),
        description: "Open a file, folder, or web page with the user's default app. \
Use this when they ask to open a document, video, image, or URL. \
Pass the real path (`~/Desktop/foo.docx`, workspace-relative, or https://…). \
Do NOT use bash `open` / osascript / guess Microsoft Word, Pages, or WPS."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Existing local path or http(s) URL"
                }
            },
            "required": ["target"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("open: bad args: {e}")))?;
    let target = a.target.trim();
    if target.is_empty() {
        return Ok(ToolCallOutcome {
            content: "open: missing target".into(),
            is_error: true,
        });
    }

    if looks_like_url(target) {
        if let Err(reason) = validate_public_http_url(target) {
            return Ok(ToolCallOutcome {
                content: format!("open: blocked URL ({reason})"),
                is_error: true,
            });
        }
        return launch(target, &format!("opened {target}"));
    }

    let path = match safety::resolve_for_open(workspace, target) {
        Ok(p) => p,
        Err(e) => {
            return Ok(ToolCallOutcome {
                content: format!("open: {e}"),
                is_error: true,
            });
        }
    };
    launch(
        &path.to_string_lossy(),
        &format!("opened {}", path.display()),
    )
}

fn looks_like_url(s: &str) -> bool {
    let t = s.trim();
    if let Some(i) = t.find("://") {
        let scheme = &t[..i];
        return !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic());
    }
    false
}

fn launch(arg: &str, ok_msg: &str) -> Result<ToolCallOutcome> {
    let mut cmd = platform_open_cmd(arg);
    match cmd.output() {
        Ok(out) if out.status.success() => Ok(ToolCallOutcome {
            content: ok_msg.into(),
            is_error: false,
        }),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let err = err.trim();
            Ok(ToolCallOutcome {
                content: if err.is_empty() {
                    format!(
                        "could not open {arg} (exit {}). Double-click the file if it is on your Desktop.",
                        out.status
                    )
                } else {
                    format!("could not open {arg}: {err}")
                },
                is_error: true,
            })
        }
        Err(e) => Ok(ToolCallOutcome {
            content: format!("could not start the system opener: {e}"),
            is_error: true,
        }),
    }
}

fn platform_open_cmd(arg: &str) -> Command {
    if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(arg);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", arg]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(arg);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"target": "outputs/nope.docx"}),
        )
        .await
        .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("does not exist") || out.content.contains("open:"));
    }

    #[tokio::test]
    async fn rejects_workspace_escape() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"target": "/etc/passwd"}),
        )
        .await
        .unwrap();
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"target": "file:///etc/passwd"}),
        )
        .await
        .unwrap();
        assert!(out.is_error);
    }

    #[test]
    fn url_detect() {
        assert!(looks_like_url("https://example.com/a"));
        assert!(looks_like_url("file:///etc/passwd"));
        assert!(!looks_like_url("~/Desktop/a.docx"));
        assert!(!looks_like_url("outputs/a.mp4"));
    }

    #[tokio::test]
    async fn opens_existing_workspace_file() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("note.txt");
        std::fs::write(&f, "hello").unwrap();
        // Resolve only — do not launch a GUI app in unit tests.
        let p = crate::safety::resolve_for_open(dir.path(), "note.txt").unwrap();
        assert!(p.exists());
    }
}
