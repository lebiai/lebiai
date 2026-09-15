//! Shared append/read for the line-delimited JSON files under the data root.
//!
//! Two properties live here so no caller can get them wrong:
//!
//! 1. **One record = one `write_all`.** The file is opened `O_APPEND` and
//!    unbuffered, so a single `write_all` of `"{json}\n"` is atomic: concurrent
//!    writers can never interleave two records onto one line. `writeln!` would
//!    split the payload and the newline into two writes and can tear a line.
//! 2. **A torn line must never kill the read.** Unparseable lines are logged at
//!    debug and skipped, so one bad byte cannot hide the whole history.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Append one JSON record as a single complete line, creating parent dirs.
pub(crate) fn append_line<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(())
}

/// Read every parseable record in file order. A missing file is empty.
pub(crate) fn read_lines<T: DeserializeOwned>(path: &Path, what: &str) -> Result<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for line in BufReader::new(std::fs::File::open(path)?)
        .lines()
        .map_while(|r| r.ok())
    {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(&line) {
            Ok(v) => out.push(v),
            Err(e) => tracing::debug!(error=%e, "skipping bad {} line", what),
        }
    }
    Ok(out)
}
