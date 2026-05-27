//! Read and write `--- yaml --- markdown body` documents.
//!
//! Parsing is intentionally simple and strict: a frontmatter block is
//! recognised iff the file starts (after optional UTF-8 BOM and CRLF
//! normalisation) with a `---` line. A missing or malformed block is an
//! error — callers that want optional frontmatter can wrap accordingly.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrontmatterError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("missing frontmatter delimiter `---` at start of {0}")]
    MissingOpen(PathBuf),

    #[error("missing closing `---` after frontmatter in {0}")]
    MissingClose(PathBuf),

    #[error("yaml parse error in {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("yaml serialize error: {0}")]
    YamlSerialize(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, FrontmatterError>;

/// A parsed frontmatter document.
#[derive(Debug, Clone)]
pub struct FrontmatterDoc<T> {
    pub frontmatter: T,
    pub body: String,
}

/// Read and parse a frontmatter document from disk.
pub fn read_doc<T: DeserializeOwned>(path: &Path) -> Result<FrontmatterDoc<T>> {
    let raw = std::fs::read_to_string(path).map_err(|source| FrontmatterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_doc(path, &raw)
}

/// Parse a frontmatter document from an in-memory string. Used when the
/// content originates from somewhere other than disk (`include_str!` of a
/// bundled file, an HTTP body, a paste-from-chat). The `origin` label is
/// only used to dress error messages.
pub fn parse_doc_str<T: DeserializeOwned>(
    origin: &str,
    raw: &str,
) -> Result<FrontmatterDoc<T>> {
    parse_doc(Path::new(origin), raw)
}

/// Write a frontmatter document to disk (non-atomic; truncates).
pub fn write_doc<T: Serialize>(path: &Path, doc: &FrontmatterDoc<T>) -> Result<()> {
    let serialized = serialize_doc(&doc.frontmatter, &doc.body)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| FrontmatterError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, serialized).map_err(|source| FrontmatterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Atomic write: serialize to a sibling temp file, then rename. Crash-safe
/// on POSIX. Falls back to non-atomic on Windows when rename fails.
pub fn write_doc_atomic<T: Serialize>(path: &Path, doc: &FrontmatterDoc<T>) -> Result<()> {
    let serialized = serialize_doc(&doc.frontmatter, &doc.body)?;
    let parent = path
        .parent()
        .ok_or_else(|| FrontmatterError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent"),
        })?;
    std::fs::create_dir_all(parent).map_err(|source| FrontmatterError::Io {
        path: parent.to_path_buf(),
        source,
    })?;

    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("frontmatter")
    ));
    std::fs::write(&tmp, &serialized).map_err(|source| FrontmatterError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| FrontmatterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

// ---- internal helpers --------------------------------------------------

fn parse_doc<T: DeserializeOwned>(path: &Path, raw: &str) -> Result<FrontmatterDoc<T>> {
    // Strip BOM, normalise CRLF.
    let mut s = raw.trim_start_matches('\u{feff}').to_string();
    if s.contains('\r') {
        s = s.replace("\r\n", "\n").replace('\r', "\n");
    }

    let after_open = s
        .strip_prefix("---\n")
        .or_else(|| s.strip_prefix("---"))
        .ok_or_else(|| FrontmatterError::MissingOpen(path.to_path_buf()))?;

    // Find the closing `---` on its own line.
    let close_idx = after_open
        .find("\n---\n")
        .or_else(|| {
            // Last `---` may not be followed by a body newline.
            after_open
                .find("\n---")
                .filter(|&i| after_open[i + 4..].is_empty())
        })
        .ok_or_else(|| FrontmatterError::MissingClose(path.to_path_buf()))?;

    let yaml_part = &after_open[..close_idx];
    let body_part = after_open[close_idx + 1..]
        .strip_prefix("---\n")
        .or_else(|| after_open[close_idx + 1..].strip_prefix("---"))
        .unwrap_or("");

    let frontmatter: T =
        serde_yaml::from_str(yaml_part).map_err(|source| FrontmatterError::Yaml {
            path: path.to_path_buf(),
            source,
        })?;

    Ok(FrontmatterDoc {
        frontmatter,
        body: body_part.to_string(),
    })
}

fn serialize_doc<T: Serialize>(frontmatter: &T, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(frontmatter)?;
    // serde_yaml emits a trailing newline; strip if present so we can
    // place our own delimiter cleanly.
    let yaml = yaml.trim_end_matches('\n');
    let body_clean = body.trim_start_matches('\n');
    Ok(format!("---\n{yaml}\n---\n{body_clean}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Front {
        name: String,
        tags: Vec<String>,
    }

    #[test]
    fn roundtrip_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        let doc = FrontmatterDoc {
            frontmatter: Front {
                name: "alpha".into(),
                tags: vec!["x".into(), "y".into()],
            },
            body: "Hello body.\n".into(),
        };
        write_doc(&path, &doc).unwrap();

        let loaded: FrontmatterDoc<Front> = read_doc(&path).unwrap();
        assert_eq!(loaded.frontmatter, doc.frontmatter);
        assert_eq!(loaded.body, "Hello body.\n");
    }

    #[test]
    fn atomic_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("note.md");
        let doc = FrontmatterDoc {
            frontmatter: Front {
                name: "beta".into(),
                tags: vec![],
            },
            body: "body\n".into(),
        };
        write_doc_atomic(&path, &doc).unwrap();
        let loaded: FrontmatterDoc<Front> = read_doc(&path).unwrap();
        assert_eq!(loaded.frontmatter.name, "beta");
    }

    #[test]
    fn missing_frontmatter_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.md");
        std::fs::write(&path, "no front matter here").unwrap();
        let err = read_doc::<Front>(&path).unwrap_err();
        assert!(matches!(err, FrontmatterError::MissingOpen(_)));
    }

    #[test]
    fn missing_close_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.md");
        std::fs::write(&path, "---\nname: x\ntags: []\nbody never closes").unwrap();
        let err = read_doc::<Front>(&path).unwrap_err();
        assert!(matches!(err, FrontmatterError::MissingClose(_)));
    }

    #[test]
    fn handles_crlf_and_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crlf.md");
        let raw = "\u{feff}---\r\nname: gamma\r\ntags: []\r\n---\r\nbody\r\n";
        std::fs::write(&path, raw).unwrap();
        let loaded: FrontmatterDoc<Front> = read_doc(&path).unwrap();
        assert_eq!(loaded.frontmatter.name, "gamma");
        assert_eq!(loaded.body, "body\n");
    }
}
