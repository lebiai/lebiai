//! Memory tools: search, save, delete, and distill episodic memories, plus the
//! Memory Palace navigation that reads the same store (`palace_*`).

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use hermes_memory::{
    distill::{find_clusters, DEFAULT_THRESHOLD},
    load_effectiveness, resolve_owner, Confidence, MemoryFrontmatter, MemoryStore,
    MemoryStoreError, OwnerDefault, Scope, Source,
};
use serde::Deserialize;

/// 由 [`dispatch`] 路由的记忆工具名。`BuiltinToolHost` 与 `PersonaToolHost`
/// 共用这一张表——记忆面只允许有一个路由表。
///
/// **palace 三件套也在这张表里**：它们读的就是同一份记忆，另开一张表 = 多一个
/// 能绕过 `ScopedMemoryStore` 的读面。漏一个就是一个泄漏口
/// （`docs/records/20260914-personas.md` Task 1.10b 的端到端证据）。
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "memory_search"
            | "memory_save"
            | "memory_delete"
            | "memory_distill"
            | "palace_zones"
            | "palace_read_zone"
            | "palace_recall"
    )
}

/// 路由一次记忆工具调用。`None` = 不是记忆工具，调用方继续往下走。
///
/// `session_owner` 是当前会话的人物 id（无人物 / 自带角色 = `None`），
/// 由调用方从 `Persona::memory_owner()` 得来；它是写入归属的唯一来源，
/// 判定本身只发生在 [`resolve_owner`] 里。
pub async fn dispatch(
    store: &dyn MemoryStore,
    name: &str,
    args: serde_json::Value,
    session_owner: Option<&str>,
) -> Option<Result<ToolCallOutcome>> {
    match name {
        "memory_search" => Some(run(store, args).await),
        "memory_save" => Some(save_run(store, args, session_owner).await),
        "memory_delete" => Some(delete_run(store, args).await),
        "memory_distill" => Some(distill_run(store, args).await),
        // 读面，与 memory_search 同源：只想让它们看得见 `store` 允许看见的那些。
        // 归属与可见性判定不在这里——`store` 已经是收窄过的那一层。
        "palace_zones" => Some(crate::palace::zones_run(store).await),
        "palace_read_zone" => Some(crate::palace::read_zone_run(store, args).await),
        "palace_recall" => Some(crate::palace::recall_run(store, args).await),
        _ => None,
    }
}

// --- memory_search ---

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    5
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "memory_search".into(),
        description: "Search your episodic memories for information relevant to a query. Returns the most relevant memory bodies.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What to search for"},
                "limit": {"type": "integer", "description": "Max results (default 5)"}
            },
            "required": ["query"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(store: &dyn MemoryStore, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: SearchArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_search: bad args: {e}")))?;

    let hits = store
        .search(&a.query, a.limit)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_search: {e}")))?;

    if hits.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!("no memories matching: {}", a.query),
            is_error: false,
        });
    }

    let out = hits
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let tags = if m.frontmatter.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", m.frontmatter.tags.join(", "))
            };
            format!(
                "{}. (id={} scope={:?} conf={:?}{tags}) {}",
                i + 1,
                m.frontmatter.id,
                m.scope,
                m.frontmatter.confidence,
                m.body.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

// --- memory_save ---

#[derive(Deserialize)]
struct SaveArgs {
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_zone")]
    zone: String,
    /// Existing memory ids this one replaces. Listed by `list_active` causes
    /// those ids to be filtered out (they stay on disk for audit). Used by
    /// the `memory_distill` flow to collapse near-duplicates into one.
    #[serde(default)]
    supersedes: Vec<String>,
}

fn default_zone() -> String {
    "general".to_string()
}

pub fn save_spec() -> ToolSpec {
    ToolSpec {
        name: "memory_save".into(),
        description: "Save a piece of knowledge or insight for future reference. \
            Use this when you discover something worth remembering: a useful approach, \
            a user preference, a working solution, or a lesson learned from a mistake. \
            Memories persist across conversations."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "The insight or knowledge to remember"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags for retrieval (e.g. ['weather', 'api'])"},
                "zone": {"type": "string", "description": "Memory zone: preferences (how they like to work / identity), standards (quality bar), work (reusable work episodes), general (default). Old names core/episode/project:* are accepted and folded.", "default": "general"},
                "supersedes": {"type": "array", "items": {"type": "string"}, "description": "Existing memory ids this one replaces (used to merge near-duplicates from memory_distill). Superseded memories stay on disk but drop out of the active set."}
            },
            "required": ["content"]
        }),
        // Normal knowledge write — open by default; memory_delete stays gated.
        requires_confirmation: false,
    }
}

pub async fn save_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
    session_owner: Option<&str>,
) -> Result<ToolCallOutcome> {
    let a: SaveArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_save: bad args: {e}")))?;

    if a.content.trim().is_empty() {
        return Ok(ToolCallOutcome {
            content: "memory_save: content must not be empty".into(),
            is_error: true,
        });
    }

    let gate = hermes_reflect::MemoryCandidate {
        fact: a.content.clone(),
        tags: a.tags.clone(),
        zone: a.zone.clone(),
        scope: hermes_memory::Scope::User,
        confidence: hermes_memory::Confidence::High,
        rationale: String::new(),
        supersedes: a.supersedes.clone(),
        owner: None,
    };
    if !hermes_reflect::memory_passes_gate(&gate) {
        return Ok(ToolCallOutcome {
            content: "memory_save refused: this is not a lasting rule \
                      (empty shell, session dump, or environment note)."
                .into(),
            is_error: true,
        });
    }

    let fm = MemoryFrontmatter::new(Source::User, Confidence::High, a.tags, a.zone);
    // 「在谁的会话里说的就归谁」（规格 §5.2）。判定只有 `resolve_owner` 一处；
    // `memory_save` 的入参没有 owner 字段，模型不该自己填，所以 `asked_owner = None`
    // ——永远走规则 4 的 `Session` 方向。zone 兜底（规则 2）把用户偏好挡回全局。
    let owner = resolve_owner(
        session_owner,
        None,
        &fm.zone,
        &fm.tags,
        OwnerDefault::Session,
    );
    let mut fm = fm.owned(owner);
    let id = fm.id.clone();
    // `MemoryFrontmatter::new` initialises `supersedes` to empty; honour the
    // caller's list if provided (the distill flow uses this to retire the
    // members of a merged cluster).
    fm.supersedes = a.supersedes;

    // 查重由 `put` 自己把关（P1-4）——这里只负责把 `Conflict` 翻译成模型看得懂的话。
    match store.put(Scope::User, fm, &a.content) {
        Ok(path) => Ok(ToolCallOutcome {
            content: format!("Saved memory {id} → {}", path.display()),
            is_error: false,
        }),
        Err(MemoryStoreError::Conflict {
            existing_id,
            similarity,
        }) => {
            // 查重看的是**全库**（`ScopedMemoryStore` 有意透传，见 scoped.rs）：
            // 命中的那条可能是本视图看不见的别人的记忆。id 与相似度都是「它存在」
            // 的探针，所以看不见时只回一句通用的拒绝，不留任何线索。
            let visible = store
                .get(&existing_id)
                .map_err(|e| hermes_core::Error::ToolHost(format!("memory_save: {e}")))?
                .is_some();
            Ok(ToolCallOutcome {
                content: if visible {
                    format!(
                        "memory_save refused: too similar to existing memory {existing_id} \
                         (similarity {similarity:.2}). Use memory_search to review it, or \
                         call memory_save with supersedes=[\"{existing_id}\"] to replace it."
                    )
                } else {
                    "memory_save refused: too similar to an existing memory. \
                     Try rephrasing."
                        .to_string()
                },
                is_error: true,
            })
        }
        Err(e) => Ok(ToolCallOutcome {
            content: format!("memory_save failed: {e}"),
            is_error: true,
        }),
    }
}

// --- memory_delete ---

#[derive(Deserialize)]
struct DeleteArgs {
    id: String,
}

pub fn delete_spec() -> ToolSpec {
    ToolSpec {
        name: "memory_delete".into(),
        description: "Delete an outdated or incorrect memory by its ID. \
            Use memory_search first to find the ID."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "The memory ID to delete (e.g. mem_abc123)"}
            },
            "required": ["id"]
        }),
        requires_confirmation: true,
    }
}

pub async fn delete_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: DeleteArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_delete: bad args: {e}")))?;

    match store.delete(Scope::User, &a.id) {
        Ok(true) => Ok(ToolCallOutcome {
            content: format!("Deleted memory: {}", a.id),
            is_error: false,
        }),
        Ok(false) => Ok(ToolCallOutcome {
            content: format!("Memory not found: {}", a.id),
            is_error: true,
        }),
        Err(e) => Ok(ToolCallOutcome {
            content: format!("memory_delete failed: {e}"),
            is_error: true,
        }),
    }
}

// --- memory_distill (read-only report) ---

#[derive(Deserialize)]
struct DistillArgs {
    /// Cosine-similarity threshold for "near-duplicate". Genuine rewordings
    /// cluster around 0.55–0.65; unrelated facts score below ~0.1.
    #[serde(default = "default_threshold")]
    threshold: f64,
}

fn default_threshold() -> f64 {
    DEFAULT_THRESHOLD
}

/// `memory_distill`: scan the active memory store for near-duplicate clusters
/// and return a **read-only report**. Does NOT write or delete anything.
///
/// The report tells the LLM which memories overlap and which one would
/// survive. To actually merge, the LLM must ask the user which clusters to
/// collapse, then call `memory_save` with the survivor's body and a
/// `supersedes` list of every member id in that cluster. This keeps the write
/// behind the existing `memory_save` confirmation gate — no silent rewrites.
pub fn distill_spec() -> ToolSpec {
    ToolSpec {
        name: "memory_distill".into(),
        description: "Find near-duplicate memories that could be merged into one. \
            Returns a read-only report of clusters (which memories overlap, which \
            would survive, similarity score). Does NOT change anything. After showing \
            the report to the user and getting confirmation, merge a cluster by calling \
            memory_save with the survivor's content and supersedes set to every member \
            id in that cluster. Use when the user asks to tidy/整理/consolidate memories."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "threshold": {"type": "number", "description": "Cosine-similarity threshold for near-duplicate (default 0.55). Lower = more clusters, higher = fewer.", "default": 0.55}
            },
            "required": []
        }),
        requires_confirmation: false,
    }
}

pub async fn distill_run(
    store: &dyn MemoryStore,
    args: serde_json::Value,
) -> Result<ToolCallOutcome> {
    let a: DistillArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_distill: bad args: {e}")))?;

    let active = store
        .list_active()
        .map_err(|e| hermes_core::Error::ToolHost(format!("memory_distill: {e}")))?;
    if active.len() < 2 {
        return Ok(ToolCallOutcome {
            content: format!("Only {} active memory — nothing to distill.", active.len()),
            is_error: false,
        });
    }

    let eff = load_effectiveness().unwrap_or_default();
    let clusters = find_clusters(&active, a.threshold, Some(&eff));
    if clusters.is_empty() {
        return Ok(ToolCallOutcome {
            content: format!(
                "No near-duplicate clusters at threshold {:.2} ({} active memories scanned).",
                a.threshold,
                active.len()
            ),
            is_error: false,
        });
    }

    let total_superseded: usize = clusters
        .iter()
        .map(|c| c.superseded_ids(&active).len())
        .sum();
    let protected = clusters.iter().any(|c| c.protected);

    let mut out = String::new();
    out.push_str(&format!(
        "Found {} cluster(s); {} memory(ies) could be merged{}.\n\n",
        clusters.len(),
        total_superseded,
        if protected {
            " (some protected — review only)"
        } else {
            ""
        }
    ));
    out.push_str("This is a READ-ONLY report. To merge a cluster, call memory_save with the survivor's content and supersedes = every member id in that cluster.\n\n");

    for (n, c) in clusters.iter().enumerate() {
        let marker = if c.protected {
            " [PROTECTED — do not auto-merge]"
        } else {
            ""
        };
        out.push_str(&format!(
            "cluster {} · {} members · similarity {:.2}{}\n",
            n,
            c.members.len(),
            c.max_score,
            marker
        ));
        let survivor = &active[c.survivor_idx];
        out.push_str(&format!(
            "  KEEP: {} — {}\n",
            survivor.frontmatter.id,
            one_line(&survivor.body)
        ));
        for id in c.superseded_ids(&active) {
            if let Some(m) = active.iter().find(|x| x.frontmatter.id == id) {
                out.push_str(&format!(
                    "    merge: {} — {}\n",
                    m.frontmatter.id,
                    one_line(&m.body)
                ));
            }
        }
        // List ALL member ids so the LLM can pass them straight to memory_save.
        let all_members: Vec<&str> = c
            .members
            .iter()
            .filter_map(|&i| active.get(i).map(|m| m.frontmatter.id.as_str()))
            .collect();
        out.push_str(&format!(
            "  → to merge: memory_save(content=\"...\", supersedes=[{}])\n\n",
            all_members
                .iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

fn one_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::FsMemoryStore;
    use tempfile::tempdir;

    /// Build a fresh FsMemoryStore backed by a temp dir.
    fn fresh_store() -> (tempfile::TempDir, FsMemoryStore) {
        let dir = tempdir().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf(), None);
        (dir, store)
    }

    /// 一个**显式关掉查重**的 store：只有「故意要造一堆近似条目」的测试才该用它
    /// （例如蒸馏的聚类检测 —— 它的输入就是近重复）。生产写入永远走带闸门的 store。
    fn fresh_store_without_dedup() -> (tempfile::TempDir, FsMemoryStore) {
        let dir = tempdir().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf(), None).with_dedup_threshold(0.0);
        (dir, store)
    }

    /// `handles` 与 `dispatch` 的分派臂是同文件里的两张名单，会漂移：名字进了
    /// `handles` 却没进 `dispatch` → 调用方拿到 `None`，最后报「unknown memory
    /// tool」。这条测试把两张名单钉在一起。
    #[tokio::test]
    async fn the_handles_table_and_the_dispatch_arms_cannot_drift() {
        // 循环里的 `memory_save` 会往这份 store 里放一条，palace 读随后就会命中
        // ——命中要往数据根记 `accessed`（`palace.rs`），所以整条用例套临时数据根。
        let _root = crate::test_env::temp_data_root();
        let (_dir, store) = fresh_store();
        let args = |name: &str| match name {
            "memory_search" => serde_json::json!({"query": "anything"}),
            "memory_save" => serde_json::json!({"content": "漂移守卫：一条够长的长期规则"}),
            "memory_delete" => serde_json::json!({"id": "mem_missing"}),
            "palace_read_zone" => serde_json::json!({"zone": "general"}),
            "palace_recall" => serde_json::json!({"topic": "anything"}),
            _ => serde_json::json!({}),
        };

        for name in [
            "memory_search",
            "memory_save",
            "memory_delete",
            "memory_distill",
            "palace_zones",
            "palace_read_zone",
            "palace_recall",
        ] {
            assert!(handles(name), "{name} 必须在路由表里");
            assert!(
                dispatch(&store, name, args(name), None).await.is_some(),
                "{name} 进了 handles 却没有 dispatch 臂——调用方会拿到 None"
            );
        }

        // 反向：不属于记忆面的工具必须落到 None——这张表只管记忆与 palace
        // 导航，文件 / 技能 / 网络工具留在各自的臂里。
        assert!(!handles("read"));
        assert!(
            dispatch(&store, "read", serde_json::json!({"path": "x"}), None)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn distill_run_reports_a_cluster_for_near_duplicates() {
        let (_dir, store) = fresh_store_without_dedup();
        // Two near-duplicates + one unrelated.
        let m1 = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        let id1 = m1.id.clone();
        store
            .put(
                Scope::User,
                m1,
                "The user prefers vim as their primary editor",
            )
            .unwrap();
        let m2 = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        store
            .put(Scope::User, m2, "User prefers vim as the primary editor")
            .unwrap();
        let m3 = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        store
            .put(Scope::User, m3, "The build server is named ci-prod-07")
            .unwrap();

        let args = serde_json::json!({"threshold": 0.55});
        let out = distill_run(&store, args).await.unwrap();
        assert!(!out.is_error);
        // Exactly one cluster, the unrelated build-server memory is not in it.
        assert!(
            out.content.contains("Found 1 cluster(s)"),
            "got: {}",
            out.content
        );
        assert!(
            out.content.contains(&id1),
            "survivor/merged members must list id1"
        );
        assert!(!out.content.contains("ci-prod-07"));
        // Report must carry the "how to merge" instruction referencing memory_save.
        assert!(out.content.contains("memory_save"));
        assert!(out.content.contains("supersedes"));
    }

    #[tokio::test]
    async fn distill_run_flags_protected_clusters() {
        let (_dir, store) = fresh_store_without_dedup();
        let m1 = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "core".into());
        store
            .put(
                Scope::User,
                m1,
                "The user is a software architect who prefers vim",
            )
            .unwrap();
        let m2 = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "core".into());
        store
            .put(
                Scope::User,
                m2,
                "The user is a software architect that prefers vim",
            )
            .unwrap();

        let args = serde_json::json!({"threshold": 0.55});
        let out = distill_run(&store, args).await.unwrap();
        assert!(
            out.content.contains("PROTECTED"),
            "preferences-zone cluster must be flagged: {}",
            out.content
        );
    }

    #[tokio::test]
    async fn distill_run_with_few_memories_returns_clean_message() {
        let (_dir, store) = fresh_store();
        let m = MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        store.put(Scope::User, m, "only one memory").unwrap();

        let out = distill_run(&store, serde_json::json!({})).await.unwrap();
        assert!(out.content.contains("nothing to distill"));
    }

    #[tokio::test]
    async fn save_run_with_supersedes_retires_the_old_memory() {
        let (_dir, store) = fresh_store();
        // An existing memory.
        let old =
            MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        let old_id = old.id.clone();
        store.put(Scope::User, old, "old fact").unwrap();
        // Sanity: it's active before.
        assert!(store
            .list_active()
            .unwrap()
            .iter()
            .any(|m| m.frontmatter.id == old_id));

        // Save a new memory that supersedes it.
        let args = serde_json::json!({
            "content": "new merged fact",
            "supersedes": [old_id]
        });
        // 归属不是本测试的主题：无人物会话 → 全局（跟今天的生产路径一致）。
        let out = save_run(&store, args, None).await.unwrap();
        assert!(!out.is_error, "{}", out.content);

        // The old memory must now be filtered OUT of the active set.
        let active = store.list_active().unwrap();
        assert!(
            !active.iter().any(|m| m.frontmatter.id == old_id),
            "superseded memory must drop out of active set"
        );
        // The new memory must be present.
        assert!(active.iter().any(|m| m.body.trim() == "new merged fact"));
        // And the old one is still on disk for audit (list() vs list_active()).
        assert!(store
            .list()
            .unwrap()
            .iter()
            .any(|m| m.frontmatter.id == old_id));
    }

    #[tokio::test]
    async fn save_run_refuses_worthless_shell() {
        let (_dir, store) = fresh_store();
        // 归属不是本测试的主题：无人物会话 → 全局（跟今天的生产路径一致）。
        let out = save_run(&store, serde_json::json!({"content": "hi"}), None)
            .await
            .unwrap();
        assert!(out.is_error, "{}", out.content);
        assert!(store.list_active().unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_run_without_supersedes_is_backward_compatible() {
        let (_dir, store) = fresh_store();
        let args = serde_json::json!({"content": "plain lasting rule about how they write titles"});
        // 归属不是本测试的主题：无人物会话 → 全局（跟今天的生产路径一致）。
        let out = save_run(&store, args, None).await.unwrap();
        assert!(!out.is_error);
        assert_eq!(store.list_active().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn save_run_rejects_near_duplicate_without_supersedes() {
        let (_dir, store) = fresh_store();
        let first = serde_json::json!({
            "content": "The user prefers vim as their primary editor"
        });
        // 归属不是本测试的主题：无人物会话 → 全局（跟今天的生产路径一致）。
        let out1 = save_run(&store, first, None).await.unwrap();
        assert!(!out1.is_error, "{}", out1.content);

        let second = serde_json::json!({
            "content": "User prefers vim as the primary editor"
        });
        // 同上：第三条断言的是查重，不是归属。
        let out2 = save_run(&store, second, None).await.unwrap();
        assert!(
            out2.is_error,
            "expected near-dup refusal, got: {}",
            out2.content
        );
        assert!(
            out2.content.contains("too similar") || out2.content.contains("supersedes"),
            "{}",
            out2.content
        );
        assert_eq!(store.list_active().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn save_run_allows_near_duplicate_when_superseding() {
        let (_dir, store) = fresh_store();
        let old =
            MemoryFrontmatter::new(Source::User, Confidence::Medium, vec![], "general".into());
        let old_id = old.id.clone();
        store
            .put(
                Scope::User,
                old,
                "The user prefers vim as their primary editor",
            )
            .unwrap();

        let args = serde_json::json!({
            "content": "User prefers vim as the primary editor",
            "supersedes": [old_id]
        });
        // 归属不是本测试的主题：无人物会话 → 全局（跟今天的生产路径一致）。
        let out = save_run(&store, args, None).await.unwrap();
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(store.list_active().unwrap().len(), 1);
    }
}
