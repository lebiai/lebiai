//! Shared install / delete logic for skills.
//!
//! Two install modes:
//!
//! - **slug**: `owner/repo@skill-name` — fetch the entire skill *directory*
//!   from GitHub's Contents API and lay it down on disk preserving
//!   `scripts/` / `references/` / `assets/`. This is the canonical form.
//! - **raw URL**: `https://…/SKILL.md` — single-file fallback. Useful when
//!   the user pastes a one-off skill link; clearly degraded (we won't fetch
//!   sibling files because we don't know what they are).
//!
//! Both modes share:
//! - frontmatter parse → re-validated against [`validate_skill_name`]
//! - `always_active` forced to `false` (remote skills don't get to inject
//!   themselves into every system prompt)
//! - quotas: ≤50 files, ≤100 KB per file, ≤5 MB total
//! - path validation for sub-paths (`..`, absolute, depth, reserved names)
//! - **transactional write**: assemble the full bundle in a sibling
//!   tempdir, then atomic-rename onto the final location. Mid-flight
//!   failures never leave a half-installed skill.
//!
//! The function is **sync** (blocking reqwest). Callers in async contexts
//! should wrap with `tokio::task::spawn_blocking`. This keeps `hermes-skills`
//! free of a tokio runtime dependency.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use hermes_store::{parse_doc_str, FrontmatterDoc};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::skill::{Scope, SkillFrontmatter};
use crate::store::{validate_skill_name, SkillStore};

/// Hard caps for any single install. Same limits apply to remote (slug /
/// URL) install and local multi-file `skill_create` extras.
pub const MAX_FILES: usize = 50;
pub const MAX_FILE_BYTES: u64 = 100 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 5 * 1024 * 1024;
/// Sub-paths inside a skill dir may be at most this many segments deep.
pub const MAX_PATH_DEPTH: usize = 6;

/// Skills that ship inside the CLI binary and are auto-installed at
/// startup. Refuse to delete or overwrite-via-install these — they'd come
/// back on the next launch anyway and the explicit error avoids confusion.
pub const BUNDLED_SKILLS: &[&str] = &["memory-palace", "skill-creator", "find-skills"];

/// What a successful install wrote to disk. Surfaced back to the agent so
/// it can confirm to the user which files landed.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub name: String,
    pub description: String,
    /// Branch / tag / commit actually used for the fetch. For a raw-URL
    /// install this is just `"url"` since refs don't apply.
    pub resolved_ref: String,
    /// Paths relative to the skill directory, always including `SKILL.md`.
    pub files_written: Vec<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DeleteOutcome {
    pub name: String,
    pub files_removed: usize,
}

/// Parse a SKILL.md document from an in-memory string. Used by the
/// auto-install path (`include_str!` of a bundled SKILL.md) and by the
/// install logic below after fetching a remote SKILL.md.
pub fn parse_skill_doc(raw: &str) -> Result<(SkillFrontmatter, String)> {
    let doc: FrontmatterDoc<SkillFrontmatter> = parse_doc_str("<skill>", raw)
        .context("parsing SKILL.md frontmatter")?;
    Ok((doc.frontmatter, doc.body))
}

/// Validate a path segment relative to a skill directory. Caller-friendly
/// errors so the agent can correct itself.
pub fn validate_relative_path(p: &str) -> Result<()> {
    if p.is_empty() {
        bail!("relative path is empty");
    }
    if p.starts_with('/') || p.starts_with('\\') {
        bail!("relative path {p:?} is absolute; must be relative to the skill directory");
    }
    if p.contains('\0') {
        bail!("relative path {p:?} contains a NUL byte");
    }
    // Reject SKILL.md collisions — those go through the `body` channel.
    let lower = p.to_ascii_lowercase();
    if lower == "skill.md" {
        bail!("use the `body` field for SKILL.md, not `extra_files`");
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts.len() > MAX_PATH_DEPTH {
        bail!(
            "relative path {p:?} is too deep ({} segments, max {})",
            parts.len(),
            MAX_PATH_DEPTH
        );
    }
    for part in &parts {
        if part.is_empty() {
            bail!("relative path {p:?} has an empty segment");
        }
        if *part == "." || *part == ".." {
            bail!("relative path {p:?} contains a `.` / `..` segment");
        }
        if part.contains('\\') {
            bail!("relative path {p:?} contains a backslash; use forward slashes");
        }
        // No hidden segments — keeps `.git`, `.env`, `.ssh` etc. out by accident.
        if part.starts_with('.') {
            bail!("relative path {p:?} has a hidden segment {part:?}");
        }
    }
    Ok(())
}

/// Install a skill from a `source` string. Sync — wrap in
/// `tokio::task::spawn_blocking` from async contexts.
pub fn install_from_source(
    store: &dyn SkillStore,
    source: &str,
    overwrite: bool,
    git_ref: Option<&str>,
) -> Result<InstallOutcome> {
    let client = build_client()?;
    let bundle = if source.starts_with("http://") || source.starts_with("https://") {
        fetch_raw_url(&client, source)?
    } else {
        let (owner, repo, slug) = parse_slug(source)?;
        let resolved_ref = git_ref.unwrap_or("main").to_string();
        fetch_slug_bundle(&client, &owner, &repo, &slug, &resolved_ref)?
    };
    commit_bundle(store, bundle, overwrite)
}

/// Delete a locally-installed skill by name. Refuses bundled meta-skills.
pub fn delete_skill(store: &dyn SkillStore, name: &str) -> Result<DeleteOutcome> {
    if BUNDLED_SKILLS.contains(&name) {
        bail!(
            "skill {name:?} ships with the CLI and auto-reinstalls at launch — \
             nothing useful to delete"
        );
    }
    validate_skill_name(name).map_err(|e| anyhow!("invalid name {name:?}: {e}"))?;

    let loaded = store
        .get(name)
        .map_err(|e| anyhow!("looking up {name:?}: {e}"))?
        .ok_or_else(|| anyhow!("no skill named {name:?} installed"))?;

    let scope = loaded.scope;
    let dir = loaded
        .source
        .parent()
        .ok_or_else(|| anyhow!("skill {name:?} has no parent directory; corrupt install?"))?
        .to_path_buf();
    let files_removed = count_files(&dir);

    let existed = store
        .delete(scope, name)
        .map_err(|e| anyhow!("deleting {name:?}: {e}"))?;
    if !existed {
        // Race: get() saw it but delete() didn't. Still report as deleted-or-gone.
        return Ok(DeleteOutcome {
            name: name.to_string(),
            files_removed: 0,
        });
    }
    Ok(DeleteOutcome {
        name: name.to_string(),
        files_removed,
    })
}

// ===== internals ============================================================

/// Everything we need to land on disk for one install: the SKILL.md plus
/// any extra files keyed by their relative path inside the skill dir.
struct StagedBundle {
    /// `always_active` is already forced to false here.
    frontmatter: SkillFrontmatter,
    body: String,
    /// `(relative_path, bytes)`. Does NOT include SKILL.md.
    extras: Vec<(String, Vec<u8>)>,
    /// Reported back in [`InstallOutcome::resolved_ref`].
    resolved_ref: String,
}

fn build_client() -> Result<Client> {
    Client::builder()
        .user_agent(format!(
            "hermes-skills/{} (+https://github.com/anthropics/claude-code)",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(20))
        .build()
        .context("building reqwest client")
}

/// Parse `owner/repo@skill-name`. We don't allow `@ref` baked in — passing
/// `git_ref` separately keeps things explicit.
fn parse_slug(source: &str) -> Result<(String, String, String)> {
    let (repo_part, slug) = source.split_once('@').ok_or_else(|| {
        anyhow!(
            "source {source:?}: expected `owner/repo@skill-name` (or a full https:// URL)"
        )
    })?;
    let (owner, repo) = repo_part
        .split_once('/')
        .ok_or_else(|| anyhow!("source {source:?}: owner/repo part is missing `/`"))?;
    if owner.is_empty() || repo.is_empty() || slug.is_empty() {
        bail!("source {source:?}: empty owner / repo / slug");
    }
    // GitHub repo names allow alnum + `.` + `_` + `-`; we mirror that for owner.
    fn ok_component(s: &str) -> bool {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    }
    if !ok_component(owner) || !ok_component(repo) || !ok_component(slug) {
        bail!(
            "source {source:?}: owner/repo/slug must be alphanumeric (with `.`, `_`, `-`)"
        );
    }
    Ok((owner.to_string(), repo.to_string(), slug.to_string()))
}

/// Single-file fallback: fetch the URL as a raw SKILL.md.
fn fetch_raw_url(client: &Client, url: &str) -> Result<StagedBundle> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("GET {url}: HTTP {status}");
    }
    let bytes = resp.bytes().with_context(|| format!("reading {url}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        bail!(
            "SKILL.md at {url} is {} bytes (max {})",
            bytes.len(),
            MAX_FILE_BYTES
        );
    }
    let raw = std::str::from_utf8(&bytes)
        .with_context(|| format!("SKILL.md at {url} is not UTF-8"))?;
    let (mut fm, body) = parse_skill_doc(raw)?;
    fm.always_active = false;
    Ok(StagedBundle {
        frontmatter: fm,
        body,
        extras: Vec::new(),
        resolved_ref: "url".into(),
    })
}

#[derive(Deserialize, Debug)]
struct ContentsEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    size: u64,
    download_url: Option<String>,
    #[serde(default)]
    sha: Option<String>,
}

fn fetch_slug_bundle(
    client: &Client,
    owner: &str,
    repo: &str,
    slug: &str,
    git_ref: &str,
) -> Result<StagedBundle> {
    let root_path = format!("skills/{slug}");
    let mut all_files: Vec<ContentsEntry> = Vec::new();
    walk_contents(client, owner, repo, &root_path, git_ref, &mut all_files)?;

    if all_files.is_empty() {
        bail!(
            "no files found at {owner}/{repo}:{root_path}@{git_ref} — is the slug correct?"
        );
    }
    if all_files.len() > MAX_FILES {
        bail!(
            "{} files at {owner}/{repo}:{root_path} (max {})",
            all_files.len(),
            MAX_FILES
        );
    }
    let total: u64 = all_files.iter().map(|e| e.size).sum();
    if total > MAX_TOTAL_BYTES {
        bail!("total size {total} bytes exceeds cap {MAX_TOTAL_BYTES}");
    }
    for e in &all_files {
        if e.size > MAX_FILE_BYTES {
            bail!(
                "{} is {} bytes (per-file cap {MAX_FILE_BYTES})",
                e.path,
                e.size
            );
        }
    }

    // Locate SKILL.md (must exist at the slug root).
    let skill_md = all_files
        .iter()
        .find(|e| e.path == format!("{root_path}/SKILL.md"))
        .ok_or_else(|| {
            anyhow!(
                "no SKILL.md at {root_path}/ — not a valid Agent Skills directory"
            )
        })?
        .clone_self();

    // Download each file.
    let mut skill_md_raw: Option<String> = None;
    let mut extras: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in &all_files {
        let rel = entry
            .path
            .strip_prefix(&format!("{root_path}/"))
            .ok_or_else(|| anyhow!("path {} is not under {root_path}/", entry.path))?;

        let dl = entry
            .download_url
            .as_deref()
            .ok_or_else(|| anyhow!("entry {} has no download_url", entry.path))?;
        let body = http_get_bytes(client, dl)
            .with_context(|| format!("downloading {dl}"))?;
        if body.len() as u64 > MAX_FILE_BYTES {
            bail!(
                "{} body is {} bytes (cap {MAX_FILE_BYTES}); refused to keep partial install",
                entry.path,
                body.len()
            );
        }
        if entry.path == skill_md.path {
            let s = String::from_utf8(body)
                .with_context(|| format!("SKILL.md at {} is not UTF-8", entry.path))?;
            skill_md_raw = Some(s);
        } else {
            validate_relative_path(rel)
                .with_context(|| format!("bad sub-path {rel:?} inside skill"))?;
            extras.push((rel.to_string(), body));
        }
    }

    let raw =
        skill_md_raw.ok_or_else(|| anyhow!("internal: SKILL.md not downloaded after walk"))?;
    let (mut fm, body) = parse_skill_doc(&raw)?;
    fm.always_active = false;
    Ok(StagedBundle {
        frontmatter: fm,
        body,
        extras,
        resolved_ref: git_ref.to_string(),
    })
}

fn walk_contents(
    client: &Client,
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
    out: &mut Vec<ContentsEntry>,
) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={git_ref}"
    );
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        bail!("GET {url}: HTTP {status}; body: {body}");
    }
    let entries: Vec<ContentsEntry> = resp
        .json()
        .with_context(|| format!("parsing contents at {url}"))?;
    for e in entries {
        match e.kind.as_str() {
            "file" => out.push(e),
            "dir" => {
                walk_contents(client, owner, repo, &e.path, git_ref, out)?;
                if out.len() > MAX_FILES {
                    bail!("too many files in {owner}/{repo}:{path} (cap {MAX_FILES})");
                }
            }
            other => bail!(
                "unsupported entry type {:?} at {} — symlinks and submodules are not allowed",
                other,
                e.path
            ),
        }
    }
    Ok(())
}

fn http_get_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("GET {url}: HTTP {status}");
    }
    Ok(resp
        .bytes()
        .with_context(|| format!("reading {url}"))?
        .to_vec())
}

/// Commit a fully-validated bundle to disk. Sequence:
///   1. validate name
///   2. resolve target dir; bail if exists && !overwrite
///   3. create parent dir
///   4. stage everything inside a sibling tempdir
///   5. (if overwrite & target exists) remove old target
///   6. atomic rename tempdir → target
///
/// Step 4 is the slow / failure-prone part and happens BEFORE we destroy
/// the existing skill, so an overwrite that fails mid-flight leaves the
/// prior install intact.
fn commit_bundle(
    store: &dyn SkillStore,
    bundle: StagedBundle,
    overwrite: bool,
) -> Result<InstallOutcome> {
    let StagedBundle {
        frontmatter,
        body,
        extras,
        resolved_ref,
    } = bundle;

    if BUNDLED_SKILLS.contains(&frontmatter.name.as_str()) && !overwrite {
        bail!(
            "skill {:?} is bundled with the CLI; pass overwrite=true to replace \
             (it will reinstall from bundle at next launch)",
            frontmatter.name
        );
    }
    validate_skill_name(&frontmatter.name)
        .map_err(|e| anyhow!("frontmatter name {:?}: {e}", frontmatter.name))?;

    let target_dir = store
        .skill_dir(Scope::User, &frontmatter.name)
        .map_err(|e| anyhow!("resolving target dir: {e}"))?;
    if target_dir.exists() && !overwrite {
        bail!(
            "skill {:?} already installed at {} — pass overwrite=true to replace",
            frontmatter.name,
            target_dir.display()
        );
    }

    let parent = target_dir
        .parent()
        .ok_or_else(|| anyhow!("target {} has no parent", target_dir.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("ensuring parent dir {}", parent.display()))?;

    // tempdir alongside the final location → same filesystem → atomic rename.
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(parent)
        .context("creating staging tempdir")?;

    let mut files_written: Vec<String> = Vec::with_capacity(1 + extras.len());
    let mut total_bytes: u64 = 0;

    // SKILL.md
    let skill_md_path = staging.path().join("SKILL.md");
    let doc = FrontmatterDoc {
        frontmatter: frontmatter.clone(),
        body: body.clone(),
    };
    hermes_store::write_doc(&skill_md_path, &doc).context("writing SKILL.md to staging")?;
    let skill_md_size = std::fs::metadata(&skill_md_path)
        .map(|m| m.len())
        .unwrap_or(0);
    if skill_md_size > MAX_FILE_BYTES {
        bail!(
            "rendered SKILL.md is {} bytes (cap {MAX_FILE_BYTES})",
            skill_md_size
        );
    }
    total_bytes += skill_md_size;
    files_written.push("SKILL.md".into());

    // Extras
    for (rel, bytes) in &extras {
        validate_relative_path(rel)
            .with_context(|| format!("staging extra file {rel:?}"))?;
        let dst = staging.path().join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&dst, bytes)
            .with_context(|| format!("writing {}", dst.display()))?;
        total_bytes += bytes.len() as u64;
        files_written.push(rel.clone());
    }

    if total_bytes > MAX_TOTAL_BYTES {
        bail!("total install size {total_bytes} bytes exceeds cap {MAX_TOTAL_BYTES}");
    }

    // Last-second: clear the old install if overwriting. The staging dir
    // is fully formed; the window between this remove and the rename below
    // is small.
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)
            .with_context(|| format!("removing prior install at {}", target_dir.display()))?;
    }

    // Atomic commit. `keep()` consumes the TempDir so its Drop won't try
    // to rm the (now-renamed-away) path.
    let staged_path = staging.keep();
    std::fs::rename(&staged_path, &target_dir).with_context(|| {
        format!(
            "renaming staging {} → {}",
            staged_path.display(),
            target_dir.display()
        )
    })?;

    files_written.sort();
    Ok(InstallOutcome {
        name: frontmatter.name,
        description: frontmatter.description,
        resolved_ref,
        files_written,
        total_bytes,
    })
}

fn count_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

// `ContentsEntry: Clone` would be nice but `serde::Deserialize` doesn't
// auto-derive it for us with the rename attrs; this little helper is
// cheaper than annotating + propagating Clone bounds through the API.
impl ContentsEntry {
    fn clone_self(&self) -> Self {
        ContentsEntry {
            name: self.name.clone(),
            path: self.path.clone(),
            kind: self.kind.clone(),
            size: self.size,
            download_url: self.download_url.clone(),
            sha: self.sha.clone(),
        }
    }
}

// ===== tests ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slug_accepts_well_formed() {
        let (o, r, s) = parse_slug("anthropics/skills@skill-creator").unwrap();
        assert_eq!(o, "anthropics");
        assert_eq!(r, "skills");
        assert_eq!(s, "skill-creator");
    }

    #[test]
    fn parse_slug_rejects_missing_pieces() {
        assert!(parse_slug("anthropics/skills").is_err());
        assert!(parse_slug("@x").is_err());
        assert!(parse_slug("a/b@").is_err());
        assert!(parse_slug("a@b").is_err());
        assert!(parse_slug("a/b@c/d").is_err()); // slash in slug
    }

    #[test]
    fn validate_relative_path_blocks_obvious_attacks() {
        validate_relative_path("scripts/run.sh").unwrap();
        validate_relative_path("references/style.md").unwrap();
        validate_relative_path("a/b/c/d/e/f").unwrap(); // depth 6 OK

        assert!(validate_relative_path("").is_err());
        assert!(validate_relative_path("/etc/passwd").is_err());
        assert!(validate_relative_path("..").is_err());
        assert!(validate_relative_path("../etc/passwd").is_err());
        assert!(validate_relative_path("scripts/../etc").is_err());
        assert!(validate_relative_path("a/b/c/d/e/f/g").is_err()); // depth 7
        assert!(validate_relative_path("SKILL.md").is_err());
        assert!(validate_relative_path("skill.md").is_err());
        assert!(validate_relative_path(".hidden/file").is_err());
        assert!(validate_relative_path("dir\\file").is_err());
        assert!(validate_relative_path("good//bad").is_err()); // empty segment
    }

    #[test]
    fn parse_skill_doc_extracts_frontmatter_and_body() {
        let raw = r#"---
name: demo
description: a demo skill
always_active: false
---
This is the body.
With multiple lines.
"#;
        let (fm, body) = parse_skill_doc(raw).unwrap();
        assert_eq!(fm.name, "demo");
        assert_eq!(fm.description, "a demo skill");
        assert!(body.contains("This is the body."));
        assert!(body.contains("multiple lines"));
    }

    #[test]
    fn parse_skill_doc_rejects_missing_frontmatter() {
        let raw = "no frontmatter here";
        assert!(parse_skill_doc(raw).is_err());
    }

    #[test]
    fn commit_bundle_writes_skill_md_and_extras() {
        use crate::store::FsSkillStore;
        use serde_yaml::Mapping;

        let dir = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(dir.path().to_path_buf(), None);

        let fm = SkillFrontmatter {
            name: "test-skill".into(),
            description: "a test".into(),
            triggers: vec![],
            version: None,
            license: None,
            always_active: true, // gets forced false in install paths, but commit takes as-is
            extra: Mapping::new(),
        };
        let bundle = StagedBundle {
            frontmatter: fm,
            body: "## Body\n\nstep 1\n".into(),
            extras: vec![
                (
                    "scripts/run.sh".into(),
                    b"#!/bin/sh\necho hi\n".to_vec(),
                ),
                (
                    "references/anatomy.md".into(),
                    b"# Anatomy\n\nstuff\n".to_vec(),
                ),
            ],
            resolved_ref: "main".into(),
        };

        let outcome = commit_bundle(&store, bundle, false).unwrap();
        assert_eq!(outcome.name, "test-skill");
        assert_eq!(outcome.resolved_ref, "main");
        assert!(outcome.files_written.contains(&"SKILL.md".to_string()));
        assert!(outcome
            .files_written
            .contains(&"scripts/run.sh".to_string()));
        assert!(outcome
            .files_written
            .contains(&"references/anatomy.md".to_string()));
        assert!(outcome.total_bytes > 0);

        // Read it back through the trait.
        let loaded = store.get("test-skill").unwrap().unwrap();
        assert_eq!(loaded.frontmatter.name, "test-skill");
        assert!(loaded.body.contains("step 1"));

        let script = std::fs::read_to_string(
            dir.path()
                .join("test-skill")
                .join("scripts")
                .join("run.sh"),
        )
        .unwrap();
        assert!(script.contains("echo hi"));
    }

    #[test]
    fn commit_bundle_refuses_overwrite_without_flag() {
        use crate::store::FsSkillStore;
        use serde_yaml::Mapping;

        let dir = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(dir.path().to_path_buf(), None);
        let make = |body: &str| StagedBundle {
            frontmatter: SkillFrontmatter {
                name: "dup".into(),
                description: "d".into(),
                triggers: vec![],
                version: None,
                license: None,
                always_active: false,
                extra: Mapping::new(),
            },
            body: body.into(),
            extras: vec![],
            resolved_ref: "main".into(),
        };

        commit_bundle(&store, make("v1"), false).unwrap();
        let err = commit_bundle(&store, make("v2"), false).unwrap_err();
        assert!(err.to_string().contains("already installed"));
        commit_bundle(&store, make("v2"), true).unwrap();
        assert!(store.get("dup").unwrap().unwrap().body.contains("v2"));
    }

    #[test]
    fn commit_bundle_refuses_bundled_skill_without_overwrite() {
        use crate::store::FsSkillStore;
        use serde_yaml::Mapping;

        let dir = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(dir.path().to_path_buf(), None);
        let bundle = StagedBundle {
            frontmatter: SkillFrontmatter {
                name: "skill-creator".into(),
                description: "evil".into(),
                triggers: vec![],
                version: None,
                license: None,
                always_active: false,
                extra: Mapping::new(),
            },
            body: "payload".into(),
            extras: vec![],
            resolved_ref: "main".into(),
        };
        let err = commit_bundle(&store, bundle, false).unwrap_err();
        assert!(err.to_string().contains("bundled with the CLI"));
    }

    #[test]
    fn delete_refuses_bundled() {
        use crate::store::FsSkillStore;
        let dir = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(dir.path().to_path_buf(), None);
        for name in BUNDLED_SKILLS {
            let err = delete_skill(&store, name).unwrap_err();
            assert!(
                err.to_string().contains("ships with the CLI"),
                "expected bundled-skill error for {name}, got: {err}"
            );
        }
    }
}
