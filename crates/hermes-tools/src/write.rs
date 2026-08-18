//! `write` — create or overwrite a file.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;

#[derive(Deserialize)]
struct Args {
    path: String,
    content: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "write".into(),
        description: "Create a new file or completely overwrite an existing file. \
            Parent directories are created automatically. WARNING: this replaces \
            the entire file — use `edit` instead to modify parts of an existing file. \
            For large content (>150 lines), write a skeleton first, then use `edit` \
            to fill in sections incrementally.\n\
            **Generated artifacts (product default):** When the user asks you to *generate* \
            a new deliverable and does **not** name a path, write under `outputs/` \
            (e.g. `outputs/meeting-notes.md`). \
            If the user asks for Desktop / Documents / Downloads (or gives `~/Desktop/...`), \
            write there — those home export folders are allowed. \
            Paths ending in .docx/.xlsx/.doc/.xls are packaged as real Office files \
            (plain text is not a Word/Excel document). \
            Do **not** move edits of *existing* workspace files into `outputs/`."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Workspace-relative path, or absolute/~/ path under Desktop/Documents/Downloads (e.g. ~/Desktop/report.docx). Prefer outputs/<name> for new deliverables when user does not specify a location."
                },
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path", "content"]
        }),
        // Outside-workspace export paths are gated in hermes_turn::danger.
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("write: bad args: {e}")))?;

    let (resolved, _export) = safety::resolve_for_write(workspace, &a.path)?;
    let (path, bytes) = match crate::office_export::maybe_package(&resolved, &a.content) {
        Some((p, b)) => (p, b),
        None => (resolved, a.content.into_bytes()),
    };

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| hermes_core::Error::ToolHost(format!("write mkdir: {e}")))?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("write {}: {e}", path.display())))?;

    Ok(ToolCallOutcome {
        content: format!("wrote {} bytes to {}", bytes.len(), path.display()),
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn writes_text_under_outputs() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({"path": "outputs/notes.md", "content": "hello"}),
        )
        .await
        .unwrap();
        assert!(!out.is_error);
        let text = std::fs::read_to_string(dir.path().join("outputs/notes.md")).unwrap();
        assert_eq!(text, "hello");
    }

    #[tokio::test]
    async fn writes_real_docx_not_plain_text() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({
                "path": "outputs/brief.docx",
                "content": "标题\n正文一段"
            }),
        )
        .await
        .unwrap();
        assert!(!out.is_error);
        let bytes = std::fs::read(dir.path().join("outputs/brief.docx")).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(String::from_utf8_lossy(&bytes).contains("正文一段"));
    }

    #[tokio::test]
    async fn writes_real_xlsx() {
        let dir = tempdir().unwrap();
        let out = run(
            dir.path(),
            serde_json::json!({
                "path": "outputs/table.xlsx",
                "content": "项,值\nA,1"
            }),
        )
        .await
        .unwrap();
        assert!(!out.is_error);
        let bytes = std::fs::read(dir.path().join("outputs/table.xlsx")).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[tokio::test]
    async fn tilde_desktop_export_docx() {
        let dir = tempdir().unwrap();
        let name = format!(
            "lebi-write-test-{}.docx",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        let rel = format!("~/Desktop/{name}");
        let out = run(
            dir.path(),
            serde_json::json!({"path": rel, "content": "桌面出口测试"}),
        )
        .await
        .unwrap();
        assert!(!out.is_error, "{}", out.content);
        assert!(!out.content.contains(dir.path().to_string_lossy().as_ref()));
        let desktop = dirs::home_dir().unwrap().join("Desktop").join(&name);
        assert!(desktop.exists(), "expected {}", desktop.display());
        let bytes = std::fs::read(&desktop).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        let _ = std::fs::remove_file(&desktop);
    }
}
