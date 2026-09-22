//! 候选落盘的三个配方：zone 兜底、候选 → frontmatter、`Project` 落不动退回 `User`。
//!
//! 这三段曾被四处以上逐字复制（inbox 批准、CLI `persist_memory`、micro 自动接收、
//! GUI / server 的批准入口）。收在一处之后，调用方只剩**错误类型适配**
//! （`anyhow` / `GuiError` / `ApiError`）；归属判定依旧是
//! [`hermes_memory::resolve_owner`] 一个地方。

use std::path::PathBuf;

use hermes_core::companion::zones;
use hermes_memory::{
    resolve_owner, MemoryFrontmatter, MemoryStore, MemoryStoreError, OwnerDefault, Scope, Source,
};

use crate::output::MemoryCandidate;

/// `because` 落盘上限：一句话，别把整段推理塞进 frontmatter。
const BECAUSE_MAX: usize = 240;

/// zone 兜底：`trim` 后为空的落 `general`。
///
/// 模型常不填 zone，或填一串空白——两边归一，再交给 [`MemoryFrontmatter`]
/// 自己的词表（`companion::zones::normalize`）。
pub fn zone_or_general(zone: Option<&str>) -> String {
    match zone.map(str::trim).filter(|z| !z.is_empty()) {
        Some(z) => z.to_string(),
        None => zones::GENERAL.to_string(),
    }
}

/// 候选 → 落盘用的 frontmatter：zone 归一 + 判归属 + `supersedes`。
///
/// `session_owner` 是候选来源会话的工位（`SessionMeta.persona` →
/// `memory_owner_for`）。没有来源会话（跨会话的 distill、取不到会话）传 `None`。
/// 归属判定只有 [`hermes_memory::resolve_owner`] 一个地方，这里只传参。
pub fn frontmatter_for(c: &MemoryCandidate, session_owner: Option<&str>) -> MemoryFrontmatter {
    let zone = zone_or_general(Some(c.zone.as_str()));
    let owner = owner_for(c, session_owner);
    let mut fm =
        MemoryFrontmatter::new(Source::Reflection, c.confidence, c.tags.clone(), zone).owned(owner);
    fm.supersedes = c.supersedes.clone();
    // 「因为什么」跟着一起落盘：丢了它，用户就问不出「这条规矩哪来的」
    // （`docs/spec/projects.md` §5.1 规矩 2）。太长就截断——它是一句话，不是一篇文章。
    let because: String = c.rationale.trim().chars().take(BECAUSE_MAX).collect();
    fm.because = match because.trim() {
        "" => None,
        b => Some(b.to_string()),
    };
    fm
}

/// 这条候选落盘后会归谁。**只读的同一个判据**（[`resolve_owner`]），给「回执」用：
/// 用户点完头要看得见这句话落在了谁名下（`docs/spec/projects.md` §5.1 规矩 4），
/// 但这里**不算第二遍**——跟落盘走的是同一段代码。
pub fn owner_for(c: &MemoryCandidate, session_owner: Option<&str>) -> Option<String> {
    resolve_owner(
        session_owner,
        c.owner.as_deref(),
        &zone_or_general(Some(c.zone.as_str())),
        &c.tags,
        OwnerDefault::Global,
    )
}

/// 落盘：`Project` 落不动（没配项目根）时退回 `User`——模型常提 `Project`，
/// 但它并不知道用户的目录结构，宁可不隔离也不丢这条记忆。
pub fn put_with_fallback(
    store: &dyn MemoryStore,
    scope: Scope,
    fm: MemoryFrontmatter,
    body: &str,
) -> Result<PathBuf, MemoryStoreError> {
    match store.put(scope, fm.clone(), body) {
        Ok(path) => Ok(path),
        Err(e) if matches!(scope, Scope::Project) => {
            tracing::warn!(error=%e, "project scope unavailable, falling back to user");
            store.put(Scope::User, fm, body)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, FsMemoryStore};

    fn cand(zone: &str, owner: Option<&str>) -> MemoryCandidate {
        MemoryCandidate {
            fact: "一条要落盘的事实".into(),
            tags: vec![],
            zone: zone.into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "test".into(),
            supersedes: vec![],
            owner: owner.map(str::to_string),
        }
    }

    #[test]
    fn a_missing_or_blank_zone_lands_in_general() {
        assert_eq!(zone_or_general(None), "general");
        assert_eq!(zone_or_general(Some("  ")), "general");
        assert_eq!(zone_or_general(Some(" work ")), "work");
    }

    #[test]
    fn the_owner_matrix_is_resolve_owners_matrix() {
        // 没有会话工位 → 全局：候选写谁都一样（自带角色 / 无人物会话）。
        assert_eq!(
            frontmatter_for(&cand("work", Some("xiao-xie")), None).owner,
            None
        );
        // 写了本会话的工位 → 归它。
        assert_eq!(
            frontmatter_for(&cand("work", Some("xiao-xie")), Some("xiao-xie")).owner,
            Some("xiao-xie".to_string())
        );
        // 反思路径的默认方向是 Global：没写 → 落全局（宁可不隔离，不误隔离）。
        assert_eq!(
            frontmatter_for(&cand("work", None), Some("xiao-xie")).owner,
            None
        );
        // 关于用户本人的，zone 与 tag 各自都能把它拉回全局。
        assert_eq!(
            frontmatter_for(&cand("preferences", Some("xiao-xie")), Some("xiao-xie")).owner,
            None
        );
        let mut tagged = cand("work", Some("xiao-xie"));
        tagged.tags = vec!["preference".into()];
        assert_eq!(frontmatter_for(&tagged, Some("xiao-xie")).owner, None);
    }

    #[test]
    fn supersedes_and_zone_travel_with_the_candidate() {
        let mut c = cand("  ", None);
        c.supersedes = vec!["mem_old".into()];
        let fm = frontmatter_for(&c, None);
        assert_eq!(fm.source, Source::Reflection);
        assert_eq!(fm.zone, "general");
        assert_eq!(fm.supersedes, vec!["mem_old".to_string()]);
    }

    #[test]
    fn a_project_scope_without_a_root_still_writes_to_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf(), None);
        let fm = frontmatter_for(&cand("work", None), None);
        let id = fm.id.clone();

        let path = put_with_fallback(&store, Scope::Project, fm, "正文").unwrap();

        assert!(path.starts_with(dir.path()), "不许落到临时根之外");
        assert_eq!(store.get(&id).unwrap().unwrap().scope, Scope::User);
    }
}
