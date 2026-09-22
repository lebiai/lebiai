//! Per-persona view of the tool surface.
//!
//! The built-in host is built once per process and shared by every session, so
//! it cannot know which persona is talking. This wrapper is built **per turn**
//! (cheap — it only holds an `Arc` to the inner host) and routes the memory
//! tools through a [`ScopedMemoryStore`] so that reads only return 全局 + 本人物的
//! memories (`docs/spec/personas.md` §5.2) and writes land with the session's
//! owner. 收窄面 = [`memory::handles`] 那张唯一路由表，**含 palace 三件套**
//! （`palace_zones` / `palace_read_zone` / `palace_recall` 读的是同一份记忆：
//! 漏一个就是一个能看见别人记忆的口）。Everything else falls through untouched:
//! this is a view, deliberately **not** a second host — a second host would be a
//! second place to keep the tool surface in sync.

use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};
use hermes_memory::{MemoryStore, ScopedMemoryStore};

use crate::memory;

pub struct PersonaToolHost {
    inner: Arc<dyn ToolHost>,
    store: Option<Arc<dyn MemoryStore>>,
    /// 这个视图看得见的那几个归属（第一个 = 本会话自己）。空 = 无人物 / 自带角色。
    /// 组会话有两个：本项目组 + 这一轮说话的人（`docs/spec/projects.md` §4.2）。
    owners: Vec<String>,
}

impl PersonaToolHost {
    /// `owner` 是当前会话的人物 id；`None` = 无人物 / 自带角色（自带角色的
    /// 「记忆算全局」由 `Persona::memory_owner()` 给出，这里不认识 `builtin`）。
    ///
    /// `store` **必须**是 `inner` 建时用的同一份 store（同一个 `Arc`）。给了另一份，
    /// 记忆工具就会静默落在另一个库上——查得到、写得进，却和这个会话的其余部分
    /// 不是一个世界，而且没有任何东西会报错。
    pub fn new(
        inner: Arc<dyn ToolHost>,
        store: Option<Arc<dyn MemoryStore>>,
        owner: Option<String>,
    ) -> Self {
        Self::with_owners(inner, store, owner.into_iter().collect())
    }

    /// 一组归属（组会话：本项目组 + 这一轮说话的人）。顺序有意义：**第一个是本会话
    /// 自己**——写入归属判不出来时会夹到它身上（组夹回组，§4.2「判不准归组」）。
    pub fn with_owners(
        inner: Arc<dyn ToolHost>,
        store: Option<Arc<dyn MemoryStore>>,
        owners: Vec<String>,
    ) -> Self {
        Self {
            inner,
            store,
            owners,
        }
    }

    /// 本会话自己（`resolve_owner` 的 `session_owner`）。空 = 全局。
    fn session_owner(&self) -> Option<&str> {
        self.owners.first().map(String::as_str)
    }
}

#[async_trait]
impl ToolHost for PersonaToolHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        self.inner.list_tools().await
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
        if let Some(store) = self.store.as_ref().filter(|_| memory::handles(name)) {
            // 读面：过滤在 `ScopedMemoryStore` 内；写面：`self.owner` 是归属判断的
            // 输入，判定本身仍只在 `resolve_owner` 一处。
            let scoped = ScopedMemoryStore::with_owners(store.clone(), self.owners.clone());
            return memory::dispatch(&scoped, name, args, self.session_owner())
                .await
                .unwrap_or_else(|| Err(Error::ToolHost(format!("unknown memory tool: {name}"))));
        }
        self.inner.call(name, args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{
        Confidence, FsMemoryStore, LoadedMemory, MemoryFrontmatter, Scope, Source,
    };
    use tempfile::tempdir;

    /// Raw (unscoped) store + a host that routes memory tools as `xiao-xie`.
    fn persona_host(
        owner: Option<&str>,
    ) -> (tempfile::TempDir, Arc<dyn MemoryStore>, PersonaToolHost) {
        let dir = tempdir().unwrap();
        let store: Arc<dyn MemoryStore> =
            Arc::new(FsMemoryStore::new(dir.path().to_path_buf(), None));
        let inner: Arc<dyn ToolHost> = Arc::new(
            crate::BuiltinToolHost::new(dir.path().to_path_buf()).with_memory_store(store.clone()),
        );
        let host = PersonaToolHost::new(inner, Some(store.clone()), owner.map(str::to_string));
        (dir, store, host)
    }

    fn seed(store: &dyn MemoryStore, owner: Option<&str>, body: &str) -> String {
        let fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into())
            .owned(owner.map(str::to_string));
        let id = fm.id.clone();
        store.put(Scope::User, fm, body).unwrap();
        id
    }

    fn find(store: &dyn MemoryStore, needle: &str) -> Option<LoadedMemory> {
        store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains(needle))
    }

    #[tokio::test]
    async fn a_persona_session_cannot_reach_another_personas_memory() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));
        seed(store.as_ref(), None, "交付要 Word 放桌面");
        seed(
            store.as_ref(),
            Some("wang-hai-yan"),
            "标题不夸张，别用夸张形容词",
        );
        seed(
            store.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的公开口径",
        );

        let hits = host
            .call(
                "memory_search",
                serde_json::json!({"query": "标题", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(
            hits.content.contains("no memories matching"),
            "搜不到别的工位的口径: {}",
            hits.content
        );

        // 自己的看得见——少了这一半，「谁都没人物」也能让上面那条绿。
        let mine = host
            .call(
                "memory_search",
                serde_json::json!({"query": "IEA", "limit": 5}),
            )
            .await
            .unwrap();
        // 断言正文而不是 "IEA"：查空时工具会把 query 原样回显进
        // "no memories matching: IEA"，只比 query 会让这条永远绿。
        assert!(
            mine.content.contains("林碳报告只引 IEA"),
            "本人物的记忆必须搜得到: {}",
            mine.content
        );

        // 本人物自己的 + 全局的看得到。
        let globals = host
            .call(
                "memory_search",
                serde_json::json!({"query": "交付", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(globals.content.contains("Word"), "{}", globals.content);

        // 写入自动打标：没写 owner，落盘归当前人物（规格 §5.2）。
        let saved = host
            .call(
                "memory_save",
                serde_json::json!({"content": "林碳配额口径一律用 2026 版"}),
            )
            .await
            .unwrap();
        assert!(!saved.is_error, "{}", saved.content);
        let landed = find(store.as_ref(), "2026 版").expect("saved");
        assert_eq!(landed.frontmatter.owner.as_deref(), Some("xiao-xie"));
    }

    /// 泄漏钉（Task 1.10 端到端抓到的真泄漏，1.10b 修）：palace 读面必须和
    /// `memory_search` 走**同一张**路由表，否则人物会话能读别人的记忆。
    #[tokio::test]
    async fn palace_reads_are_scoped_to_the_session_persona() {
        // 命中会往数据根记一条 `accessed`（`palace.rs`）——本用例整条都套着临时
        // 数据根：断言红了也照样不碰用户的盘（见 `crate::test_env`）。
        let _root = crate::test_env::temp_data_root();
        let dir = tempdir().unwrap();
        let store: Arc<dyn MemoryStore> =
            Arc::new(FsMemoryStore::new(dir.path().to_path_buf(), None));
        let inner: Arc<dyn ToolHost> = Arc::new(
            crate::BuiltinToolHost::new(dir.path().to_path_buf()).with_memory_store(store.clone()),
        );
        let wang = PersonaToolHost::new(
            inner.clone(),
            Some(store.clone()),
            Some("wang-hai-yan".into()),
        );
        let xie = PersonaToolHost::new(inner.clone(), Some(store.clone()), Some("xiao-xie".into()));

        seed(
            store.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的公开口径",
        );
        seed(store.as_ref(), None, "交付一律给 Word 放桌面");

        // 别人物的会话：三件套都看不见那条。args 用真形状（read_zone 要 zone，
        // recall 要 topic）。
        let wang_out = [
            (
                "palace_zones",
                wang.call("palace_zones", serde_json::json!({}))
                    .await
                    .unwrap(),
            ),
            (
                "palace_read_zone",
                wang.call("palace_read_zone", serde_json::json!({"zone": "general"}))
                    .await
                    .unwrap(),
            ),
            (
                "palace_recall",
                wang.call("palace_recall", serde_json::json!({"topic": "IEA"}))
                    .await
                    .unwrap(),
            ),
        ];
        for (label, out) in wang_out {
            assert!(
                !out.content.contains("林碳报告只引 IEA"),
                "{label} 读到了别人物的记忆: {}",
                out.content
            );
        }

        // 本人物：三件套都看得见（少了这一半，「谁都读不到」也能让上面那条绿）。
        let mine = xie
            .call("palace_recall", serde_json::json!({"topic": "IEA"}))
            .await
            .unwrap();
        assert!(
            mine.content.contains("林碳报告只引 IEA"),
            "本人物的记忆必须 recall 得到: {}",
            mine.content
        );
        let zone = xie
            .call("palace_read_zone", serde_json::json!({"zone": "general"}))
            .await
            .unwrap();
        assert!(
            zone.content.contains("林碳报告只引 IEA"),
            "本人物的 memory 必须在自己看得见的 zone 里: {}",
            zone.content
        );

        // 回归钉：无人物（内置宿主）不收窄——它是全量视图，行为与接人物之前一样。
        let builtin = inner
            .call("palace_recall", serde_json::json!({"topic": "IEA"}))
            .await
            .unwrap();
        assert!(
            builtin.content.contains("林碳报告只引 IEA"),
            "无人物宿主必须仍看得见全部（别把这条路径也收窄了）: {}",
            builtin.content
        );
    }

    #[tokio::test]
    async fn preferences_and_core_zones_stay_global_in_a_persona_session() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));

        // 「core」是 preferences 的旧别名，两条都必须在人物会话里落全局。
        let cases = [
            ("preferences", "用户偏好：交付一律用 Word 放桌面，别给 PDF"),
            ("core", "用户习惯：每周一先过一遍本周要推进的三件事"),
        ];
        for (zone, body) in cases {
            let out = host
                .call(
                    "memory_save",
                    serde_json::json!({"content": body, "zone": zone}),
                )
                .await
                .unwrap();
            assert!(!out.is_error, "{zone}: {}", out.content);
        }

        let active = store.list_active().unwrap();
        assert_eq!(active.len(), 2, "both saves must land");
        for m in &active {
            assert_eq!(
                m.frontmatter.owner, None,
                "用户偏好被锁进一顶帽子 = 只在那个工位生效（zone {:?}）",
                m.frontmatter.zone
            );
        }
    }

    #[tokio::test]
    async fn the_builtin_host_without_a_persona_saves_global() {
        let dir = tempdir().unwrap();
        let store: Arc<dyn MemoryStore> =
            Arc::new(FsMemoryStore::new(dir.path().to_path_buf(), None));
        let host =
            crate::BuiltinToolHost::new(dir.path().to_path_buf()).with_memory_store(store.clone());

        let out = host
            .call(
                "memory_save",
                serde_json::json!({"content": "无人物会话里存的知识一律算全局"}),
            )
            .await
            .unwrap();
        assert!(!out.is_error, "{}", out.content);
        let landed = find(store.as_ref(), "无人物会话").expect("saved");
        assert_eq!(landed.frontmatter.owner, None);
    }

    #[tokio::test]
    async fn non_memory_tools_fall_through_to_the_real_host() {
        let (_dir, _store, host) = persona_host(Some("xiao-xie"));
        let out = host
            .call("think", serde_json::json!({"thought": "x"}))
            .await
            .unwrap();
        assert!(
            out.content.contains("Thought recorded"),
            "非记忆工具必须原样透传——这不是第二个 host: {}",
            out.content
        );
    }

    #[tokio::test]
    async fn deleting_an_invisible_memory_reports_not_found() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));
        let other_id = seed(
            store.as_ref(),
            Some("wang-hai-yan"),
            "标题不夸张，别用夸张形容词",
        );
        let mine_id = seed(
            store.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的公开口径",
        );

        let out = host
            .call("memory_delete", serde_json::json!({"id": other_id}))
            .await
            .unwrap();
        assert!(
            out.content.contains("Memory not found"),
            "看不见的 id 是「没删到」，不是「删除失败」: {}",
            out.content
        );
        assert!(
            store.get(&other_id).unwrap().is_some(),
            "别的工位的记忆必须原地不动"
        );

        // 自己的删得掉——证明上一条是「本视图看不见」，不是「这条路径干脆没接」。
        let own = host
            .call("memory_delete", serde_json::json!({"id": mine_id}))
            .await
            .unwrap();
        assert!(own.content.contains("Deleted memory"), "{}", own.content);
        assert!(store.get(&mine_id).unwrap().is_none());
    }

    #[tokio::test]
    async fn a_near_duplicate_of_an_invisible_memory_leaks_nothing() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));
        let wang_id = seed(
            store.as_ref(),
            Some("wang-hai-yan"),
            "新闻稿标题不夸张，别用感叹号",
        );

        let out = host
            .call(
                "memory_save",
                serde_json::json!({"content": "新闻稿标题不夸张，别用感叹号！"}),
            )
            .await
            .unwrap();
        assert!(out.is_error, "近重复必须被拒: {}", out.content);
        assert!(
            !out.content.contains(&wang_id) && !out.content.contains("mem_"),
            "别人的 id 是「它存在」的探针: {}",
            out.content
        );
        assert!(
            !out.content.contains("similarity") && !out.content.contains("supersed"),
            "相似度与 supersedes 建议同样是探针: {}",
            out.content
        );
        assert!(
            !out.content.chars().any(|c| c.is_ascii_digit()),
            "一个数字都不许留（相似度就藏在这里面）: {}",
            out.content
        );

        // 正面控制：看得见的近重复照旧给足信息（id + 相似度 + 替换建议），别把功能
        // 改成一味的沉默。
        seed(store.as_ref(), None, "交付要 Word 放桌面，别给 PDF");
        let visible = host
            .call(
                "memory_save",
                serde_json::json!({"content": "交付要 Word 放桌面，别给 PDF 文件"}),
            )
            .await
            .unwrap();
        assert!(visible.is_error, "{}", visible.content);
        assert!(
            visible.content.contains("mem_")
                && visible.content.contains("similarity")
                && visible.content.contains("supersedes"),
            "看得见的那条不必藏着: {}",
            visible.content
        );
    }

    #[tokio::test]
    async fn superseding_an_invisible_memory_does_not_retire_it() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));
        let wang_id = seed(
            store.as_ref(),
            Some("wang-hai-yan"),
            "新闻稿标题不夸张，别用感叹号",
        );

        let out = host
            .call(
                "memory_save",
                serde_json::json!({
                    "content": "新闻稿标题克制一点，不用感叹号",
                    "supersedes": [wang_id],
                }),
            )
            .await
            .unwrap();
        assert!(
            !out.is_error,
            "丢弃看不见的 id 不该让写入失败: {}",
            out.content
        );

        // 别的工位那条哪儿都还在：原始 store（= GUI 管理页看的那份）、它自己的视图。
        // 注意无人物视图**看不到**它（归属有值 → 只对同一个人物可见），那不是退役，
        // 所以「没被退役」的判据是原始 store 的 active 列表。
        assert!(store.get(&wang_id).unwrap().is_some());
        assert!(
            store
                .list_active()
                .unwrap()
                .iter()
                .any(|m| m.frontmatter.id == wang_id),
            "退役过滤是全库的——盘上、管理页里它都必须还在 active"
        );
        let wang = ScopedMemoryStore::new(store.clone(), Some("wang-hai-yan".into()));
        assert!(
            wang.list_active()
                .unwrap()
                .iter()
                .any(|m| m.frontmatter.id == wang_id),
            "海燕自己必须还看得见它"
        );
        // 原本就看不见它的视图（含无人物）当然还是看不见——那不是退役，是隔离
        for (label, viewer) in [("林碳", Some("xiao-xie")), ("无人物", None)] {
            let view = ScopedMemoryStore::new(store.clone(), viewer.map(str::to_string));
            assert!(
                !view
                    .list_active()
                    .unwrap()
                    .iter()
                    .any(|m| m.frontmatter.id == wang_id),
                "{label} 视图本来就不该看见它"
            );
        }
        // 被丢掉的 id 不落在盘上
        let saved = store
            .list()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("克制一点"))
            .expect("saved");
        assert!(saved.frontmatter.supersedes.is_empty());

        // 正面控制：自己的 id 照常退役
        let mine_id = seed(
            store.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的公开口径",
        );
        let own = host
            .call(
                "memory_save",
                serde_json::json!({
                    "content": "林碳报告只引 IEA 的原始数据口径",
                    "supersedes": [mine_id],
                }),
            )
            .await
            .unwrap();
        assert!(!own.is_error, "{}", own.content);
        let mine = store
            .list()
            .unwrap()
            .into_iter()
            .find(|m| m.frontmatter.id == mine_id)
            .expect("still on disk");
        assert!(
            !store
                .list_active()
                .unwrap()
                .iter()
                .any(|m| m.frontmatter.id == mine_id),
            "自己的那条必须退出 active"
        );
        assert!(mine.frontmatter.supersedes.is_empty());
    }

    #[tokio::test]
    async fn a_preference_tag_alone_keeps_the_save_global() {
        let (_dir, store, host) = persona_host(Some("xiao-xie"));

        // 模型很自然会只打 tag 不填 zone（zone 缺省 = general）
        for (tags, body) in [
            (vec!["preference"], "交付一律用 Word 放桌面，别给 PDF"),
            (vec!["prefers"], "每周一先过一遍本周要推进的三件事"),
            (vec!["Preference"], "写周报先给结论，再给过程，别铺垫"),
            (vec!["standard"], "引用数据必须给到原始出处"),
        ] {
            let out = host
                .call(
                    "memory_save",
                    serde_json::json!({"content": body, "tags": tags}),
                )
                .await
                .unwrap();
            assert!(!out.is_error, "{tags:?}: {}", out.content);
        }
        let active = store.list_active().unwrap();
        assert_eq!(active.len(), 4);
        for m in &active {
            assert_eq!(
                m.frontmatter.owner, None,
                "只打 tag 的用户偏好被锁进了一顶帽子（tags {:?}）",
                m.frontmatter.tags
            );
        }

        // 正面控制：无关 tag 照常归工位
        let out = host
            .call(
                "memory_save",
                serde_json::json!({
                    "content": "这周先把配额口径的三份材料对齐",
                    "tags": ["news"],
                }),
            )
            .await
            .unwrap();
        assert!(!out.is_error, "{}", out.content);
        let work = find(store.as_ref(), "配额口径的三份材料").expect("saved");
        assert_eq!(work.frontmatter.owner.as_deref(), Some("xiao-xie"));
    }
}
