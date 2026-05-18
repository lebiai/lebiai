//! Filesystem-backed skill store.
//!
//! Layout under each scope root:
//! ```text
//! <root>/
//!   <skill-name>/
//!     SKILL.md
//!     scripts/        # optional, untouched by this store
//! ```
//!
//! `<root>` is `~/.small-rust-hermes/skills/` for [`Scope::User`] and
//! `./.small-rust-hermes/skills/` for [`Scope::Project`]. Project-scoped
//! skills override user-scoped skills with the same name.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::skill::{LoadedSkill, Scope, SkillFrontmatter};
use hermes_store::{FrontmatterDoc, FrontmatterError};

const SKILL_FILE: &str = "SKILL.md";

#[derive(Debug, Error)]
pub enum SkillStoreError {
    #[error("invalid skill name {0:?}: must match [A-Za-z0-9_-]+ and contain no path separators")]
    InvalidName(String),

    #[error(transparent)]
    Frontmatter(#[from] FrontmatterError),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, SkillStoreError>;

pub trait SkillStore: Send + Sync {
    /// All skills, with project scope taking precedence on name collisions.
    fn list(&self) -> Result<Vec<LoadedSkill>>;

    /// Look up a single skill by name. Project beats user on collision.
    fn get(&self, name: &str) -> Result<Option<LoadedSkill>>;

    /// Persist a skill to the chosen scope. Returns the written path.
    fn put(&self, scope: Scope, frontmatter: SkillFrontmatter, body: &str) -> Result<PathBuf>;

    /// Remove a skill. Returns true if it existed.
    fn delete(&self, scope: Scope, name: &str) -> Result<bool>;
}

/// Filesystem implementation backed by two scope roots.
pub struct FsSkillStore {
    user_root: PathBuf,
    project_root: Option<PathBuf>,
}

impl FsSkillStore {
    pub fn new(user_root: PathBuf, project_root: Option<PathBuf>) -> Self {
        Self {
            user_root,
            project_root,
        }
    }

    /// Conventional locations: `~/.small-rust-hermes/skills` (user) and
    /// `./.small-rust-hermes/skills` (project, if a `./.small-rust-hermes`
    /// directory exists).
    pub fn standard() -> Result<Self> {
        let user = standard_user_root()?;
        let project = standard_project_root();
        Ok(Self::new(user, project))
    }

    fn root_for(&self, scope: Scope) -> Option<&Path> {
        match scope {
            Scope::User => Some(&self.user_root),
            Scope::Project => self.project_root.as_deref(),
        }
    }

    fn skill_path(&self, scope: Scope, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        let root = self.root_for(scope).ok_or_else(|| {
            SkillStoreError::Config("project scope requested but no project root configured".into())
        })?;
        Ok(root.join(name).join(SKILL_FILE))
    }

    fn load_one(path: &Path, scope: Scope) -> Result<LoadedSkill> {
        let doc: FrontmatterDoc<SkillFrontmatter> = hermes_store::read_doc(path)?;
        Ok(LoadedSkill {
            frontmatter: doc.frontmatter,
            body: doc.body,
            source: path.to_path_buf(),
            scope,
        })
    }

    fn list_scope(root: &Path, scope: Scope) -> Result<Vec<LoadedSkill>> {
        if !root.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(root).map_err(|source| SkillStoreError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| SkillStoreError::Io {
                path: root.to_path_buf(),
                source,
            })?;
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let skill_md = dir.join(SKILL_FILE);
            if !skill_md.is_file() {
                continue;
            }
            match Self::load_one(&skill_md, scope) {
                Ok(s) => out.push(s),
                Err(e) => {
                    tracing::warn!(path = %skill_md.display(), error = %e, "skipping malformed skill");
                }
            }
        }
        Ok(out)
    }
}

impl SkillStore for FsSkillStore {
    fn list(&self) -> Result<Vec<LoadedSkill>> {
        let mut user = Self::list_scope(&self.user_root, Scope::User)?;
        let project = match &self.project_root {
            Some(root) => Self::list_scope(root, Scope::Project)?,
            None => Vec::new(),
        };
        // Project overrides user on name collision.
        let project_names: std::collections::HashSet<&str> =
            project.iter().map(|s| s.name()).collect();
        user.retain(|s| !project_names.contains(s.name()));
        user.extend(project);
        user.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(user)
    }

    fn get(&self, name: &str) -> Result<Option<LoadedSkill>> {
        validate_name(name)?;
        if let Some(root) = &self.project_root {
            let p = root.join(name).join(SKILL_FILE);
            if p.is_file() {
                return Ok(Some(Self::load_one(&p, Scope::Project)?));
            }
        }
        let p = self.user_root.join(name).join(SKILL_FILE);
        if p.is_file() {
            return Ok(Some(Self::load_one(&p, Scope::User)?));
        }
        Ok(None)
    }

    fn put(&self, scope: Scope, frontmatter: SkillFrontmatter, body: &str) -> Result<PathBuf> {
        if frontmatter.name.is_empty() {
            return Err(SkillStoreError::InvalidName(frontmatter.name.clone()));
        }
        let path = self.skill_path(scope, &frontmatter.name)?;
        let doc = FrontmatterDoc {
            frontmatter,
            body: body.to_string(),
        };
        hermes_store::write_doc_atomic(&path, &doc)?;
        Ok(path)
    }

    fn delete(&self, scope: Scope, name: &str) -> Result<bool> {
        validate_name(name)?;
        let root = self.root_for(scope).ok_or_else(|| {
            SkillStoreError::Config("project scope requested but no project root configured".into())
        })?;
        let dir = root.join(name);
        if !dir.exists() {
            return Ok(false);
        }
        std::fs::remove_dir_all(&dir).map_err(|source| SkillStoreError::Io {
            path: dir.clone(),
            source,
        })?;
        Ok(true)
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SkillStoreError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// `~/.small-rust-hermes/skills`.
pub fn standard_user_root() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| SkillStoreError::Config("could not resolve $HOME".into()))?;
    Ok(home.join(".small-rust-hermes").join("skills"))
}

/// `./.small-rust-hermes/skills` if the project dir exists; else `None`.
pub fn standard_project_root() -> Option<PathBuf> {
    let p = PathBuf::from(".small-rust-hermes").join("skills");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Mapping;

    fn fm(name: &str, desc: &str) -> SkillFrontmatter {
        SkillFrontmatter {
            name: name.to_string(),
            description: desc.to_string(),
            triggers: vec!["foo".into(), "bar".into()],
            version: Some("0.1.0".into()),
            license: None,
            always_active: false,
            extra: Mapping::new(),
        }
    }

    #[test]
    fn put_get_list_delete_user_scope() {
        let user = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(user.path().to_path_buf(), None);

        let path = store.put(Scope::User, fm("alpha", "first"), "Body of alpha.\n").unwrap();
        assert!(path.ends_with("alpha/SKILL.md"));

        let loaded = store.get("alpha").unwrap().expect("alpha should exist");
        assert_eq!(loaded.name(), "alpha");
        assert_eq!(loaded.frontmatter.description, "first");
        assert_eq!(loaded.body.trim(), "Body of alpha.");
        assert_eq!(loaded.scope, Scope::User);

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name(), "alpha");

        assert!(store.delete(Scope::User, "alpha").unwrap());
        assert!(store.get("alpha").unwrap().is_none());
        assert!(!store.delete(Scope::User, "alpha").unwrap());
    }

    #[test]
    fn project_overrides_user() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(
            user.path().to_path_buf(),
            Some(project.path().to_path_buf()),
        );

        store
            .put(Scope::User, fm("shared", "from user"), "user body")
            .unwrap();
        store
            .put(Scope::Project, fm("shared", "from project"), "project body")
            .unwrap();
        store
            .put(Scope::User, fm("only-user", "u"), "u body")
            .unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 2);

        let shared = store.get("shared").unwrap().unwrap();
        assert_eq!(shared.scope, Scope::Project);
        assert_eq!(shared.frontmatter.description, "from project");
    }

    #[test]
    fn rejects_bad_names() {
        let user = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(user.path().to_path_buf(), None);

        for bad in ["..", "../etc", "a/b", ".hidden", "", "with space", "weird?"] {
            let err = store.put(Scope::User, fm(bad, "x"), "y").unwrap_err();
            assert!(matches!(err, SkillStoreError::InvalidName(_)), "name {bad:?} should be rejected, got {err:?}");
        }
    }

    #[test]
    fn preserves_unknown_frontmatter_keys() {
        let user = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(user.path().to_path_buf(), None);

        let mut extra = Mapping::new();
        extra.insert(
            serde_yaml::Value::String("model".into()),
            serde_yaml::Value::String("claude-haiku-4-5-20251001".into()),
        );

        let mut fm = fm("with-extras", "test");
        fm.extra = extra;
        store.put(Scope::User, fm, "body").unwrap();

        let loaded = store.get("with-extras").unwrap().unwrap();
        assert_eq!(
            loaded.frontmatter.extra.get(serde_yaml::Value::String("model".into())),
            Some(&serde_yaml::Value::String("claude-haiku-4-5-20251001".into()))
        );
    }

    #[test]
    fn project_scope_requires_project_root() {
        let user = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(user.path().to_path_buf(), None);

        let err = store.put(Scope::Project, fm("x", "y"), "z").unwrap_err();
        assert!(matches!(err, SkillStoreError::Config(_)));
    }

    #[test]
    fn empty_root_returns_empty_list() {
        let user = tempfile::tempdir().unwrap();
        let store = FsSkillStore::new(user.path().join("does-not-exist"), None);
        let listed = store.list().unwrap();
        assert!(listed.is_empty());
    }
}
