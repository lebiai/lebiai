//! Skill tools: list, read, read bundled files, create.
//!
//! These implement the Activation and Execution stages of the
//! [Agent Skills](https://agentskills.io) Progressive Disclosure model.
//! Discovery (a name/description index of every installed skill) is injected
//! into the system prompt by `crates/hermes-cli/src/commands/context.rs`.
//! When the LLM sees a description that matches the task at hand, it calls
//! `skill_read(name)` to load the full body — and optionally
//! `skill_read_file(name, path)` to pull bundled scripts or references.
//!
//! `skill_create` is the direct-write counterpart of `propose_skill`: it
//! materialises a skill immediately under `~/.small-rust-hermes/skills/`,
//! intended for cases where the user says "save this as a skill" in the
//! current conversation. `propose_skill` keeps its place for reflection-time
//! distillation that goes through the approval UI.

use std::path::{Path, PathBuf};

use hermes_core::{Error, Result, ToolCallOutcome, ToolSpec};
use hermes_skills::{Scope, SkillFrontmatter, SkillStore};
use serde::Deserialize;
use serde_yaml::Mapping;

// --- skill_list -------------------------------------------------------------

pub fn list_spec() -> ToolSpec {
    ToolSpec {
        name: "skill_list".into(),
        description: "List every installed skill with its name and description. \
            Use this when the user asks what skills you have, or when the \
            system-prompt index has been truncated and you need to scan the \
            full set. Returns name, scope, and description — not the body. \
            Call `skill_read` to load a specific skill's full instructions \
            before acting on it."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        requires_confirmation: false,
    }
}

pub async fn list_run(store: &dyn SkillStore) -> Result<ToolCallOutcome> {
    let skills = store
        .list()
        .map_err(|e| Error::ToolHost(format!("skill_list: {e}")))?;
    if skills.is_empty() {
        return Ok(ToolCallOutcome {
            content: "No skills installed. Use `skill_create` to add one, \
                or place a SKILL.md under ~/.small-rust-hermes/skills/<name>/."
                .into(),
            is_error: false,
        });
    }
    let mut out = format!("{} skill(s) installed:\n", skills.len());
    for s in &skills {
        out.push_str(&format!(
            "- {} (scope={:?}): {}\n",
            s.frontmatter.name, s.scope, s.frontmatter.description
        ));
    }
    Ok(ToolCallOutcome {
        content: out.trim_end().to_string(),
        is_error: false,
    })
}

// --- skill_read -------------------------------------------------------------

#[derive(Deserialize)]
struct ReadArgs {
    name: String,
}

pub fn read_spec() -> ToolSpec {
    ToolSpec {
        name: "skill_read".into(),
        description: "Load the full instructions of a skill. Call this the \
            moment a skill's description (from the system-prompt index or \
            `skill_list`) matches the task you are about to perform — \
            BEFORE you start acting on the task. The returned body tells \
            you how to execute the skill; follow it. The response also lists \
            any bundled files (scripts, references, assets) that you can \
            fetch via `skill_read_file`."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name as shown in the index, e.g. \"officecli\"."
                }
            },
            "required": ["name"]
        }),
        requires_confirmation: false,
    }
}

pub async fn read_run(
    store: &dyn SkillStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: ReadArgs = serde_json::from_value(args)
        .map_err(|e| Error::ToolHost(format!("skill_read: bad args: {e}")))?;

    let skill = match store
        .get(&a.name)
        .map_err(|e| Error::ToolHost(format!("skill_read: {e}")))?
    {
        Some(s) => s,
        None => {
            return Ok(ToolCallOutcome {
                content: format!(
                    "skill_read: no skill named {:?}. Call `skill_list` to see what's available.",
                    a.name
                ),
                is_error: true,
            });
        }
    };

    let bundled = enumerate_bundled_files(&skill.source);

    let mut out = format!(
        "# {} (scope={:?})\n\n_{}_\n\n---\n\n{}",
        skill.frontmatter.name,
        skill.scope,
        skill.frontmatter.description,
        skill.body.trim()
    );
    if !bundled.is_empty() {
        out.push_str(
            "\n\n---\n\n## Bundled files\nCall `skill_read_file` with the `name` above and one of these paths:\n",
        );
        for rel in bundled {
            out.push_str(&format!("- {}\n", rel.display()));
        }
    }
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

/// Enumerate files inside the skill directory besides `SKILL.md`. Paths are
/// returned relative to the skill directory.
fn enumerate_bundled_files(skill_md_path: &Path) -> Vec<PathBuf> {
    let dir = match skill_md_path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path == skill_md_path {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(dir) {
            out.push(rel.to_path_buf());
        }
    }
    out.sort();
    out
}

// --- skill_read_file --------------------------------------------------------

#[derive(Deserialize)]
struct ReadFileArgs {
    name: String,
    path: String,
}

pub fn read_file_spec() -> ToolSpec {
    ToolSpec {
        name: "skill_read_file".into(),
        description: "Read a bundled file from a skill's directory (scripts, \
            references, templates, assets). The file list for each skill is \
            included in the output of `skill_read`. Paths are relative to the \
            skill directory; traversal (`..`, absolute paths) is rejected."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Skill name."},
                "path": {"type": "string", "description": "Relative path inside the skill dir, e.g. \"scripts/list_zones.py\"."}
            },
            "required": ["name", "path"]
        }),
        requires_confirmation: false,
    }
}

pub async fn read_file_run(
    store: &dyn SkillStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: ReadFileArgs = serde_json::from_value(args)
        .map_err(|e| Error::ToolHost(format!("skill_read_file: bad args: {e}")))?;

    if a.path.is_empty()
        || a.path.starts_with('/')
        || a.path.contains("..")
        || a.path.contains('\\')
    {
        return Ok(ToolCallOutcome {
            content: format!("skill_read_file: invalid path {:?}", a.path),
            is_error: true,
        });
    }

    let skill = match store
        .get(&a.name)
        .map_err(|e| Error::ToolHost(format!("skill_read_file: {e}")))?
    {
        Some(s) => s,
        None => {
            return Ok(ToolCallOutcome {
                content: format!("skill_read_file: no skill named {:?}", a.name),
                is_error: true,
            });
        }
    };
    let dir = match skill.source.parent() {
        Some(p) => p,
        None => {
            return Ok(ToolCallOutcome {
                content: "skill_read_file: cannot resolve skill directory".into(),
                is_error: true,
            });
        }
    };
    let target = dir.join(&a.path);

    let canon_dir = std::fs::canonicalize(dir)
        .map_err(|e| Error::ToolHost(format!("skill_read_file: canonicalize: {e}")))?;
    let canon_target = match std::fs::canonicalize(&target) {
        Ok(p) => p,
        Err(e) => {
            return Ok(ToolCallOutcome {
                content: format!("skill_read_file: cannot read {}: {e}", target.display()),
                is_error: true,
            });
        }
    };
    if !canon_target.starts_with(&canon_dir) {
        return Ok(ToolCallOutcome {
            content: format!(
                "skill_read_file: path {:?} escapes the skill directory",
                a.path
            ),
            is_error: true,
        });
    }

    match std::fs::read_to_string(&canon_target) {
        Ok(s) => Ok(ToolCallOutcome {
            content: s,
            is_error: false,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("skill_read_file: {e}"),
            is_error: true,
        }),
    }
}

// --- skill_create -----------------------------------------------------------

#[derive(Deserialize)]
struct CreateArgs {
    name: String,
    description: String,
    body: String,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    always_active: bool,
    #[serde(default)]
    overwrite: bool,
}

pub fn create_spec() -> ToolSpec {
    ToolSpec {
        name: "skill_create".into(),
        description: "Create a new skill on disk under \
            ~/.small-rust-hermes/skills/<name>/SKILL.md. Use this when the \
            user explicitly asks to save a workflow as a skill (\"save this \
            as a skill called X\", \"remember how to do this for next time\"). \
            The `description` is what future-you sees in the discovery index — \
            write it so the activation decision (\"should I read the body?\") is \
            obvious from the description alone. The `body` is the full \
            instructions loaded only on activation. Returns an error if a \
            skill with the same name exists; pass `overwrite: true` to replace."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Skill name (letters, digits, dash, underscore)."},
                "description": {"type": "string", "description": "One-sentence summary shown in the discovery index. Make activation decisions obvious."},
                "body": {"type": "string", "description": "Full instructions (Markdown). Loaded only when the skill is activated."},
                "triggers": {"type": "array", "items": {"type": "string"}, "description": "Optional hint tokens; kept for forward compatibility, not required."},
                "always_active": {"type": "boolean", "description": "If true, the body is injected into every system prompt. Default false."},
                "overwrite": {"type": "boolean", "description": "Allow replacing an existing skill with the same name. Default false."}
            },
            "required": ["name", "description", "body"]
        }),
        requires_confirmation: true,
    }
}

pub async fn create_run(
    store: &dyn SkillStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: CreateArgs = serde_json::from_value(args)
        .map_err(|e| Error::ToolHost(format!("skill_create: bad args: {e}")))?;

    if a.name.trim().is_empty() || a.description.trim().is_empty() || a.body.trim().is_empty() {
        return Ok(ToolCallOutcome {
            content: "skill_create: name, description, and body must all be non-empty.".into(),
            is_error: true,
        });
    }

    if !a.overwrite {
        if let Ok(Some(existing)) = store.get(&a.name) {
            return Ok(ToolCallOutcome {
                content: format!(
                    "skill_create: a skill named {:?} already exists at {} (scope={:?}). \
                    Pass overwrite=true to replace it, or pick a new name.",
                    a.name,
                    existing.source.display(),
                    existing.scope
                ),
                is_error: true,
            });
        }
    }

    let frontmatter = SkillFrontmatter {
        name: a.name.clone(),
        description: a.description,
        triggers: a.triggers,
        version: None,
        license: None,
        always_active: a.always_active,
        extra: Mapping::new(),
    };

    match store.put(Scope::User, frontmatter, &a.body) {
        Ok(path) => Ok(ToolCallOutcome {
            content: format!("Saved skill {} → {}", a.name, path.display()),
            is_error: false,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("skill_create failed: {e}"),
            is_error: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_skills::FsSkillStore;
    use tempfile::tempdir;

    fn fresh_store() -> (tempfile::TempDir, FsSkillStore) {
        let dir = tempdir().unwrap();
        let store = FsSkillStore::new(dir.path().to_path_buf(), None);
        (dir, store)
    }

    #[tokio::test]
    async fn list_then_create_then_read_roundtrips() {
        let (_dir, store) = fresh_store();

        let listed = list_run(&store).await.unwrap();
        assert!(listed.content.contains("No skills installed"));

        let created = create_run(
            &store,
            serde_json::json!({
                "name": "demo",
                "description": "demo skill for tests",
                "body": "# Steps\n1. do thing\n2. do other thing"
            }),
        )
        .await
        .unwrap();
        assert!(!created.is_error, "create should succeed: {created:?}");

        let listed = list_run(&store).await.unwrap();
        assert!(listed.content.contains("demo"));
        assert!(listed.content.contains("demo skill for tests"));

        let read = read_run(&store, serde_json::json!({"name": "demo"}))
            .await
            .unwrap();
        assert!(!read.is_error);
        assert!(read.content.contains("# demo"));
        assert!(read.content.contains("do thing"));
    }

    #[tokio::test]
    async fn create_refuses_to_overwrite_without_flag() {
        let (_dir, store) = fresh_store();

        create_run(
            &store,
            serde_json::json!({
                "name": "dup",
                "description": "first",
                "body": "v1"
            }),
        )
        .await
        .unwrap();

        let second = create_run(
            &store,
            serde_json::json!({
                "name": "dup",
                "description": "second",
                "body": "v2"
            }),
        )
        .await
        .unwrap();
        assert!(second.is_error);
        assert!(second.content.contains("already exists"));

        let forced = create_run(
            &store,
            serde_json::json!({
                "name": "dup",
                "description": "second",
                "body": "v2",
                "overwrite": true
            }),
        )
        .await
        .unwrap();
        assert!(!forced.is_error);
    }

    #[tokio::test]
    async fn read_unknown_skill_is_an_error_with_a_hint() {
        let (_dir, store) = fresh_store();
        let out = read_run(&store, serde_json::json!({"name": "missing"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("skill_list"));
    }

    #[tokio::test]
    async fn read_file_rejects_traversal() {
        let (_dir, store) = fresh_store();
        create_run(
            &store,
            serde_json::json!({
                "name": "demo",
                "description": "x",
                "body": "y"
            }),
        )
        .await
        .unwrap();

        for bad in ["../../../etc/passwd", "/etc/passwd", "..\\foo"] {
            let out = read_file_run(
                &store,
                serde_json::json!({"name": "demo", "path": bad}),
            )
            .await
            .unwrap();
            assert!(out.is_error, "path {bad:?} should be rejected, got {out:?}");
        }
    }

    #[tokio::test]
    async fn read_lists_bundled_files() {
        let (dir, store) = fresh_store();
        create_run(
            &store,
            serde_json::json!({
                "name": "demo",
                "description": "x",
                "body": "y"
            }),
        )
        .await
        .unwrap();
        // Drop a script alongside SKILL.md.
        let scripts_dir = dir.path().join("demo").join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("run.sh"), "echo hi\n").unwrap();

        let read = read_run(&store, serde_json::json!({"name": "demo"}))
            .await
            .unwrap();
        assert!(read.content.contains("Bundled files"));
        assert!(read.content.contains("scripts/run.sh"));

        let file = read_file_run(
            &store,
            serde_json::json!({"name": "demo", "path": "scripts/run.sh"}),
        )
        .await
        .unwrap();
        assert!(!file.is_error);
        assert_eq!(file.content.trim(), "echo hi");
    }
}
