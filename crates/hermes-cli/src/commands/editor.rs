//! Launch the user's `$EDITOR` on a temp file and return the edited text.

use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

/// Write `initial` to a temp file, exec `$EDITOR` (default `vi`) on it
/// synchronously, then read the file back.
///
/// Returns [`None`] if the user left the file unchanged (verbatim match)
/// or cleared it to whitespace — treated as "cancel".
pub fn edit(initial: &str, filename_hint: &str) -> Result<Option<String>> {
    let mut path: PathBuf = std::env::temp_dir();
    let rand = uuid::Uuid::new_v4().simple().to_string();
    path.push(format!("hermes-{}-{rand}.md", sanitize(filename_hint)));

    {
        let mut f = std::fs::File::create(&path)
            .with_context(|| format!("creating temp file {}", path.display()))?;
        f.write_all(initial.as_bytes())?;
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("running $EDITOR={editor}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(anyhow!("editor exited with {status}"));
    }

    let mut edited = String::new();
    std::fs::File::open(&path)?
        .read_to_string(&mut edited)
        .with_context(|| format!("reading back {}", path.display()))?;
    let _ = std::fs::remove_file(&path);

    if edited.trim().is_empty() {
        return Ok(None);
    }
    if edited == initial {
        return Ok(None);
    }
    Ok(Some(edited))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(32)
        .collect()
}
