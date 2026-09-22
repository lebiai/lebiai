//! 归属判定（[`resolve_owner`]）与每个视图的读写边界（[`ScopedMemoryStore`]）。
//!
//! The store itself stays persona-agnostic: 归属 is one filter, not a second
//! storage layout. [`resolve_owner`] is the **only** place that decides who a
//! memory belongs to; a scoped view only enforces that decision on the way in
//! and out, and reads only ever return globals plus the owner's own memories.
//!
//! Management surfaces（「它记得的」）keep using the raw store: 注入隔离，管理可见
//! (`docs/spec/personas.md` §5.4).

use std::path::PathBuf;
use std::sync::Arc;

use hermes_core::companion::tags as companion_tags;
use hermes_core::companion::zones;

use crate::memory::{normalize_owner, visible_to_any, LoadedMemory, MemoryFrontmatter, Scope};
use crate::store::{MemoryStore, Result};

/// 没写 owner 时往哪边倒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerDefault {
    /// 工具路径（`memory_save`）：「在谁的会话里说的就归谁」（规格 §5.2）。
    Session,
    /// 反思路径：关于用户本人的归用户，关于这份活的才归工位（规格 §5.3）；
    /// 模型没说清就往安全方向（全局）倒——宁可不隔离，不误隔离。
    Global,
}

/// 归属的**唯一判定点**。任何写入路径都从这里拿 owner，不许旁路自己拼。
///
/// 规则按序：
/// 1. 会话没有人物（`None` / 空 / 纯空白）→ 全局。自带角色李现 / 小文走这条——
///    调用方用 `Persona::memory_owner()` 得出 `session_owner`，判定里不认识 `builtin`。
/// 2. **会话在项目组里**（`session_owner` 是一张桌子，见 [`team_owner`]）→
///    **偏好仍归全局**（「关于你本人」在哪儿都算数），其余一律归这张桌子：
///    点名了**桌上的成员** → 归他（手艺跟着人走），
///    其余（这份活的标准 / 判不准 / 点了桌外人）→ **归组**（宁可窄，不要串味）。
/// 3. `zone ∈ {preferences, standards}`（含 `core` 等别名）**或** `tags` 里出现
///    `preference` / `prefers` / `standard` → 全局。确定性兜底，不靠模型自觉：
///    用户偏好被锁进一顶帽子 = 只在那个工位生效。
/// 4. 明确写了**本会话**的人物 id → 归该人物。
/// 5. 其余（没写 / 写了别人）→ 按 [`OwnerDefault`] 倒；永远不会落到"当前视图看不见"的
///    地方（写了别人物时不会顺着别人的 id 存下去）。
pub fn resolve_owner(
    session_owner: Option<&str>,
    asked_owner: Option<&str>,
    zone: &str,
    tags: &[String],
    default: OwnerDefault,
) -> Option<String> {
    let session = normalize_owner(session_owner)?;

    if let Some(t) = hermes_core::team::get(&session) {
        return resolve_team_owner(t, asked_owner, zone, tags);
    }

    if zones::is_preferences(zone) || zones::is_standards(zone) || tags_mark_user_level(tags) {
        return None;
    }

    if normalize_owner(asked_owner).as_deref() == Some(session.as_str()) {
        return Some(session);
    }

    match default {
        OwnerDefault::Session => Some(session),
        OwnerDefault::Global => None,
    }
}

/// 项目组会话里那一档（`docs/spec/projects.md` §4.2）：
/// 偏好 → 全局；点名桌上成员 → 他（手艺）；其余 → 组。
///
/// **只有偏好**算「关于用户本人」：`standard` 这个 zone / tag 在组里说的是
/// 「这个栏目的标准」，不是用户的偏好——今天它会漏成全局，串到每个工位上去。
fn resolve_team_owner(
    t: &hermes_core::team::Team,
    asked_owner: Option<&str>,
    zone: &str,
    tags: &[String],
) -> Option<String> {
    if zones::is_preferences(zone) || tags.iter().any(|t| companion_tags::is_preference_tag(t)) {
        return None;
    }
    if let Some(asked) = normalize_owner(asked_owner) {
        if t.member(&asked).is_some() {
            return Some(asked);
        }
    }
    Some(t.id.clone())
}

/// 这个 owner 是不是一张桌子（项目组）。**判据只此一处**：会话 → 归属的转换
/// （[`hermes_core::persona::memory_owner_for`]）之后，读侧与「它记得的」都得问它
/// 才知道该显示组名还是人物名。
pub fn team_owner(owner: &str) -> Option<&'static hermes_core::team::Team> {
    hermes_core::team::get(owner)
}

/// tag 也会说「这是关于用户本人的」——模型常常只打 tag 不填 `zone`，缺了这道闸，
/// 一条只带 `["preference"]` 的用户偏好就被锁进当前工位（规格 §5.3 点名的 bug）。
///
/// 词表只有一份，住在 `companion::tags`（`hermes-reflect` 的反思归类读的是同一份）。
/// 反思路径那边是**就地改 zone**，工具路径不碰 zone（`memory_save` 标什么存什么），
/// 所以这里只读 tags、不改 frontmatter。
fn tags_mark_user_level(tags: &[String]) -> bool {
    tags.iter()
        .any(|t| companion_tags::is_preference_tag(t) || companion_tags::is_standard_tag(t))
}

pub struct ScopedMemoryStore {
    inner: Arc<dyn MemoryStore>,
    /// 这个视图看得见的那几个归属。**空 = 只看得到全局**（无人物 / 自带角色）。
    /// 组会话有两个：本项目组 + 这一轮说话的人（`docs/spec/projects.md` §4.2）。
    owners: Vec<String>,
}

impl ScopedMemoryStore {
    /// `owner` 是当前会话的人物 id；`None` = 无人物（未指定人物 / 自带角色），
    /// 此时写入落全局、读只看得到全局。
    pub fn new(inner: Arc<dyn MemoryStore>, owner: Option<String>) -> Self {
        Self {
            inner,
            owners: normalize_owner(owner.as_deref()).into_iter().collect(),
        }
    }

    /// 一组归属的视图（组会话：本项目组 + 这一轮说话的人）。顺序有意义：
    /// **第一个是「本会话自己」**，越界写入会被夹到它身上（见 [`Self::guard_owner`]）。
    pub fn with_owners(inner: Arc<dyn MemoryStore>, owners: Vec<String>) -> Self {
        let owners = owners
            .iter()
            .filter_map(|o| normalize_owner(Some(o)))
            .collect();
        Self { inner, owners }
    }

    /// 本会话自己（越界归它）。没有归属 → `None`（＝全局）。
    fn owns(&self) -> Option<&str> {
        self.owners.first().map(String::as_str)
    }

    fn can_see(&self, m: &LoadedMemory) -> bool {
        let owners: Vec<&str> = self.owners.iter().map(String::as_str).collect();
        visible_to_any(m, &owners)
    }

    fn visible(&self, mut items: Vec<LoadedMemory>) -> Vec<LoadedMemory> {
        items.retain(|m| self.can_see(m));
        items
    }

    /// 写守卫：只拦「调用方送来的 owner 落在本视图之外」这种编程错误。
    /// 方向**永远只能是更窄或相等**，绝不夹成全局——把别人物的内容变成谁都看得见，
    /// 比放错工位严重得多。归属判定不在这里，只有 [`resolve_owner`] 一处。
    fn guard_owner(&self, asked: Option<String>) -> Option<String> {
        let asked = normalize_owner(asked.as_deref());
        match (self.owns(), asked) {
            // 没写（`None` = 全局）→ 原样。
            (_, None) => None,
            // 写的就是本视图里的某一个归属（组会话里：组 或 这一轮说话的人）→ 原样。
            (_, Some(a)) if self.owners.contains(&a) => Some(a),
            // 本视图看不见的归属 → 夹回本视图自己（组会话夹回**组**，见 §4.2「判不准归组」）。
            (Some(mine), Some(a)) => {
                tracing::warn!(
                    asked_owner = %a,
                    session_owner = %mine,
                    "memory write asked for an owner outside this view; clamped to the session owner"
                );
                Some(mine.to_string())
            }
            // 本视图没有人物（自带角色 / 未指定人物）：夹成全局会把别人的内容泄漏给
            // 所有人，方向不允许变宽 → 保持更窄的那个，只告警。
            (None, Some(a)) => {
                tracing::warn!(
                    asked_owner = %a,
                    "globals-only memory view writes a persona-owned memory; keeping the narrower owner"
                );
                Some(a)
            }
        }
    }

    /// 退役名单也走可见性，与 `delete` 对称：别人的 id 不许从这里退役。
    ///
    /// `list_active` 的退役过滤是**全库**的（`store.rs`），塞一个看不见的 id 进来
    /// 会把它对所有人退役——B 工位的记忆从 A 工位消失，包括 GUI 管理页，文件却还在。
    ///
    /// 看不见 / 不存在 / 读不动 → **静默丢弃 + `warn`，不报错**：报错等于回答
    /// 「这个 id 存在与否」，那本身就是探针。宁可少退役一条，不跨边界。
    fn guard_supersedes(&self, ids: Vec<String>) -> Vec<String> {
        ids.into_iter()
            .filter(|id| match self.inner.get(id) {
                Ok(Some(m)) => self.can_see(&m),
                Ok(None) => {
                    tracing::warn!(id = %id, "supersede target not found; dropped");
                    false
                }
                Err(e) => {
                    tracing::warn!(id = %id, error = %e, "supersede target unreadable; dropped");
                    false
                }
            })
            .collect()
    }
}

impl MemoryStore for ScopedMemoryStore {
    fn list(&self) -> Result<Vec<LoadedMemory>> {
        Ok(self.visible(self.inner.list()?))
    }

    fn list_active(&self) -> Result<Vec<LoadedMemory>> {
        Ok(self.visible(self.inner.list_active()?))
    }

    fn list_pinned(&self) -> Result<Vec<LoadedMemory>> {
        Ok(self.visible(self.inner.list_pinned()?))
    }

    fn get(&self, id: &str) -> Result<Option<LoadedMemory>> {
        Ok(self.inner.get(id)?.filter(|m| self.can_see(m)))
    }

    fn put(&self, scope: Scope, mut frontmatter: MemoryFrontmatter, body: &str) -> Result<PathBuf> {
        // 不打标：`None` 就是全局，原样落盘。归属判定只有 resolve_owner 一处，
        // 否则「模型没写 → 全局」（§5.3）会被这里改写成「→ 会话私有」。
        //
        // 「有意写下去」的意图要在**守卫之前**记下来：守卫可能把本视图看不见的
        // supersedes id 全部丢掉，但用户/模型想替换这件事并没有因此变成「随手记一条」。
        // 意图丢了，查重闸门（P1-4）就会把这次写入误判成重复。
        if !frontmatter.supersedes.is_empty() {
            frontmatter.intentional = true;
        }
        frontmatter.owner = self.guard_owner(frontmatter.owner);
        frontmatter.supersedes = self.guard_supersedes(frontmatter.supersedes);
        self.inner.put(scope, frontmatter, body)
    }

    fn delete(&self, scope: Scope, id: &str) -> Result<bool> {
        // 看不见的 id 不是错误，只是没删到——上层提示语不该说「删除失败」。
        match self.inner.get(id)? {
            Some(m) if self.can_see(&m) => self.inner.delete(scope, id),
            _ => Ok(false),
        }
    }

    /// 先过滤再排序，不是「取 top-k 再过滤」——否则本人物的记忆会被别人的挤掉。
    ///
    /// 降级说明：本视图的检索走 TF-IDF（[`crate::relevance::search_memories`]），
    /// `FsMemoryStore::search` 里的 `embed` 语义分支**未接**（今天没有 crate 启用
    /// `embed` feature，无实际影响）。将来启用时，语义检索必须在**过滤后**的集合上
    /// 做，而不是先搜全库再过滤。
    fn search(&self, query: &str, k: usize) -> Result<Vec<LoadedMemory>> {
        let active = self.list_active()?;
        Ok(crate::relevance::search_memories(&active, query, k)
            .into_iter()
            .cloned()
            .collect())
    }

    /// 透传给 inner：**看到的是全库**，所以理论上可能因为别的人物的一条近似记忆
    /// 而拒掉本次写入。这是有意的取舍——宁可让模型换个说法，也不要让它把这份活
    /// 的口径写错位置（写歪的归属不会自己跑回来）。
    fn check_near_duplicate(&self, body: &str, threshold: f64) -> Result<()> {
        self.inner.check_near_duplicate(body, threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Confidence, Source};
    use crate::store::FsMemoryStore;
    use crate::DEFAULT_DEDUP_THRESHOLD;
    use tempfile::tempdir;

    fn store_with(dir: &std::path::Path) -> Arc<dyn MemoryStore> {
        Arc::new(FsMemoryStore::new(dir.to_path_buf(), None))
    }

    /// 关掉写入查重的 store：只有**故意**要造近重复条目的测试才用它
    /// （比如验证「排序前先按归属过滤」需要五条几乎一样的记忆）。
    fn store_without_dedup(dir: &std::path::Path) -> Arc<dyn MemoryStore> {
        Arc::new(FsMemoryStore::new(dir.to_path_buf(), None).with_dedup_threshold(0.0))
    }

    fn put(store: &dyn MemoryStore, owner: Option<&str>, body: &str) {
        let fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into())
            .owned(owner.map(str::to_string));
        store.put(Scope::User, fm, body).unwrap();
    }

    fn bodies(store: &dyn MemoryStore) -> Vec<String> {
        store
            .list_active()
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect()
    }

    fn id_of(store: &dyn MemoryStore, needle: &str) -> String {
        store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains(needle))
            .unwrap_or_else(|| panic!("没找到含 {needle:?} 的记忆"))
            .frontmatter
            .id
    }

    #[test]
    fn a_scoped_view_writes_no_tag_and_cannot_read_others() {
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());
        // 正文必须 ≥ 8 字：更短的会被 `list_active` 的空壳过滤掉（既有规则）。
        put(raw.as_ref(), None, "交付时文档要 Word 放桌面");
        put(
            raw.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的原始数据",
        );
        put(
            raw.as_ref(),
            Some("wang-hai-yan"),
            "新闻稿标题不夸张别用感叹号",
        );

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        let seen: Vec<String> = xie
            .list_active()
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        assert_eq!(seen.len(), 2, "看得见的只有全局 + 自己");

        // put 不打标：`None` 就是全局，原样落盘（判定点只有 resolve_owner）。
        // 打标会把「模型没写 → 全局」（§5.3）改写成「→ 会话私有」。
        put(&xie, None, "林碳配额口径用 2026 版");
        let raw_bodies = bodies(raw.as_ref());
        assert!(raw_bodies.contains(&"林碳配额口径用 2026 版".to_string()));
        let landed = raw
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("2026 版"))
            .unwrap();
        assert_eq!(
            landed.frontmatter.owner.as_deref(),
            None,
            "None 原样落全局，不被 store 打标"
        );
        assert_eq!(
            xie.list_active().unwrap().len(),
            3,
            "全局那条本视图也看得见"
        );

        // 检索不跨人物
        let hits: Vec<String> = xie
            .search("标题", 5)
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        assert!(hits.is_empty(), "王海燕的记忆不能从小谢这儿搜出来");

        // 删不到别人的
        let other = raw
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("标题不夸张"))
            .unwrap();
        assert!(!xie.delete(Scope::User, &other.frontmatter.id).unwrap());
        assert_eq!(bodies(raw.as_ref()).len(), 4, "别人的一条都不能被删掉");
    }

    #[test]
    fn reading_another_personas_memory_is_impossible_by_id_too() {
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());
        put(raw.as_ref(), None, "交付时文档要 Word 放桌面");
        put(
            raw.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的原始数据",
        );
        put(
            raw.as_ref(),
            Some("wang-hai-yan"),
            "新闻稿标题不夸张别用感叹号",
        );
        let mut pinned =
            MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into())
                .owned(Some("wang-hai-yan".into()));
        pinned.pinned = true;
        raw.put(Scope::User, pinned, "海燕钉住的那条记忆内容")
            .unwrap();

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        assert!(xie
            .get(&id_of(raw.as_ref(), "标题不夸张"))
            .unwrap()
            .is_none());
        assert!(xie
            .get(&id_of(raw.as_ref(), "林碳报告只引 IEA"))
            .unwrap()
            .is_some());
        assert!(xie
            .get(&id_of(raw.as_ref(), "交付时文档要 Word"))
            .unwrap()
            .is_some());

        let pinned_bodies: Vec<String> = xie
            .list_pinned()
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        assert!(
            !pinned_bodies.iter().any(|b| b.contains("海燕钉住的那条")),
            "钉住也不能跨人物冒出来"
        );

        // 自己的（含全局）删得到，返回 true 而不是 false
        let own = id_of(raw.as_ref(), "林碳报告只引 IEA");
        assert!(xie.delete(Scope::User, &own).unwrap());
        assert_eq!(bodies(raw.as_ref()).len(), 3);
    }

    #[test]
    fn search_filters_before_ranking_so_my_own_memory_is_not_pushed_out() {
        let dir = tempdir().unwrap();
        let raw = store_without_dedup(dir.path());
        for i in 0..5 {
            put(
                raw.as_ref(),
                Some("wang-hai-yan"),
                &format!("新闻稿标题别夸张 第 {i} 稿的标题 标题 标题"),
            );
        }
        put(raw.as_ref(), Some("xiao-xie"), "标题里别用感叹号也别用问号");

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        let hits: Vec<String> = xie
            .search("标题", 1)
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        assert_eq!(
            hits,
            vec!["标题里别用感叹号也别用问号".to_string()],
            "先过滤再排序：别人的高分条目挤不掉我的"
        );

        // 无人物时只看全局，别人的一条也搜不出来
        put(raw.as_ref(), None, "交付时文档要 Word 放桌面");
        let nobody = ScopedMemoryStore::new(raw.clone(), None);
        let global_hits: Vec<String> = nobody
            .search("交付", 5)
            .unwrap()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        assert_eq!(global_hits, vec!["交付时文档要 Word 放桌面".to_string()]);
    }

    #[test]
    fn a_blank_owner_is_no_owner_so_nothing_lands_invisible() {
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());

        // 纯空白的人物 id = 没指定人物
        let nobody = ScopedMemoryStore::new(raw.clone(), Some("   ".into()));
        put(&nobody, None, "交付时文档要 Word 放桌面");

        // 调用方显式填了空串也算没填（归一成 `None` = 全局），而不是留个
        // 「谁都看不见」的空串
        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        put(&xie, Some(""), "林碳报告只引 IEA 的原始数据");

        let all = raw.list_active().unwrap();
        let global = all.iter().find(|m| m.body.contains("Word")).unwrap();
        assert_eq!(global.frontmatter.owner, None, "空 owner = 全局");
        let owned = all.iter().find(|m| m.body.contains("IEA")).unwrap();
        assert_eq!(
            owned.frontmatter.owner, None,
            "空串当没写：原样落全局，而不是留个谁都看不见的空串"
        );

        // 没有任何一条落在「谁都看不见」的空串归属上
        assert!(all
            .iter()
            .all(|m| m.frontmatter.owner.as_deref() != Some("")));
        assert_eq!(nobody.list_active().unwrap().len(), 2, "两条全局都看得见");
        assert_eq!(xie.list_active().unwrap().len(), 2, "全局 + 自己的");
    }

    #[test]
    fn builtin_persona_sessions_naturally_write_globals() {
        // 自带角色（李现 / 小文）是地基不是专业分工：它们的 memory_owner() 是 None，
        // 所以 `new(_, owner)` 天然全局落盘。这里用 persona 定义的真值，不硬编码 None,
        // 免得以后有人把 id 直接传进来。
        let dir = tempdir().unwrap();
        let raw = store_without_dedup(dir.path());
        for id in ["li-xian", "xiao-wen"] {
            let owner = hermes_core::persona::get(id)
                .unwrap_or_else(|| panic!("自带角色 {id} 必须在定义表里"))
                .memory_owner()
                .map(str::to_string);
            let view = ScopedMemoryStore::new(raw.clone(), owner);
            put(&view, None, &format!("在 {id} 那儿教的通用规矩"));
        }

        let all = raw.list_active().unwrap();
        assert_eq!(all.len(), 2);
        assert!(
            all.iter().all(|m| m.frontmatter.owner.is_none()),
            "自带角色的会话不产生专业分工的归属"
        );

        // 所有工位都遵守，别人物的会话也看得见
        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        assert_eq!(xie.list_active().unwrap().len(), 2);
    }

    #[test]
    fn a_padded_owner_written_around_the_guard_still_reads_back() {
        // 审查实测出的静默失败路径：`owner: 'xiao-xie '` 的记录文件在磁盘上、不报错、
        // 却对所有视图不可见。这里绕过守卫直接落盘（老文件 / 别的写入路径），钉住读侧。
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("padded.md"),
            "---\nid: mem_padded0001\ncreated: 2026-01-01T00:00:00Z\nsource: user\n\
             confidence: high\nzone: work\nowner: 'xiao-xie '\n---\n林碳报告只引 IEA 的原始数据\n",
        )
        .unwrap();
        let raw = store_with(dir.path());
        put(raw.as_ref(), Some("   "), "交付时文档要 Word 放桌面");

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        assert!(
            bodies(&xie).iter().any(|b| b.contains("IEA")),
            "尾空格的归属仍归林碳，必须读得回来"
        );
        assert!(
            xie.get(&id_of(raw.as_ref(), "IEA")).unwrap().is_some(),
            "get 也不能被空格卡住"
        );

        let wang = ScopedMemoryStore::new(raw.clone(), Some("wang-hai-yan".into()));
        assert_eq!(
            bodies(&wang),
            vec!["交付时文档要 Word 放桌面".to_string()],
            "别人的一条都不给；纯空白归属算全局"
        );
    }

    #[test]
    fn the_dedup_gate_sees_the_whole_store_by_design() {
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());
        put(
            raw.as_ref(),
            Some("wang-hai-yan"),
            "新闻稿标题不夸张，别用感叹号",
        );

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        // 已知取舍：查重看的是全库，别人物的近似记忆会拦下我的写入。宁可让模型换个
        // 说法，也不要让它把这份活的口径写错位置。
        assert!(xie
            .check_near_duplicate("新闻稿标题不夸张，别用感叹号", DEFAULT_DEDUP_THRESHOLD)
            .is_err());
        // 不近似就正常放行
        assert!(xie
            .check_near_duplicate("林碳配额口径用 2026 版", DEFAULT_DEDUP_THRESHOLD)
            .is_ok());
    }

    /// 组里那一档（`docs/spec/projects.md` §4.2）：**标准归组、手艺归点名的人、
    /// 判不准归组、偏好仍归全局**。
    ///
    /// 「标准归组」今天会红：`zone=standards` 曾一律落全局——组里的口径就那么漏进了
    /// 每一个工位（串味；比放错工位严重）。
    #[test]
    fn on_a_team_standards_belong_to_the_team_and_craft_to_the_named_member() {
        const TEAM: &str = "caifu-zaozhidao";

        // 这份活的标准 → 归组
        for zone in ["standards", "standard", "Standards"] {
            assert_eq!(
                resolve_owner(Some(TEAM), None, zone, &[], OwnerDefault::Global),
                Some(TEAM.to_string()),
                "组里的 {zone} 归组，不许漏成全局"
            );
        }
        // 判不准（没写 owner、也没写 zone）→ 归组（宁可窄，不要串味）
        assert_eq!(
            resolve_owner(Some(TEAM), None, "general", &[], OwnerDefault::Global),
            Some(TEAM.to_string())
        );
        // 点名桌上的成员 → 手艺归他
        assert_eq!(
            resolve_owner(
                Some(TEAM),
                Some("wang-hai-yan"),
                "work",
                &[],
                OwnerDefault::Global
            ),
            Some("wang-hai-yan".to_string()),
            "组里点名的人，手艺跟着他走"
        );
        assert_eq!(
            resolve_owner(
                Some(TEAM),
                Some("wang-hai-yan"),
                "standards",
                &[],
                OwnerDefault::Global
            ),
            Some("wang-hai-yan".to_string()),
            "只有「关于用户本人」才拦得住；标准也可以是某个人的口径"
        );
        // 点了桌外的人 / 桌外的人写了组 → 都归组
        assert_eq!(
            resolve_owner(
                Some(TEAM),
                Some("xiao-jin"),
                "work",
                &[],
                OwnerDefault::Global
            ),
            Some(TEAM.to_string()),
            "桌外人点不出来（这个版本不认识他）→ 归组"
        );
        assert_eq!(
            resolve_owner(
                Some("xiao-jin"),
                Some(TEAM),
                "work",
                &[],
                OwnerDefault::Session
            ),
            Some("xiao-jin".to_string()),
            "工位会话里写了个组 id，不该把记忆挂到组上去"
        );
        // 关于用户本人的 → 全局（在组里也一样）
        for zone in ["preferences", "preference", "core"] {
            assert_eq!(
                resolve_owner(Some(TEAM), None, zone, &[], OwnerDefault::Global),
                None,
                "组里的 {zone} 仍是关于用户的偏好 → 全局"
            );
        }
        let preference_tag = vec!["preference".to_string()];
        assert_eq!(
            resolve_owner(
                Some(TEAM),
                None,
                "general",
                &preference_tag,
                OwnerDefault::Global
            ),
            None
        );
        // `standard` 这个 tag 在组里说的是「这个栏目的标准」→ 归组（不是用户偏好）
        let standard_tag = vec!["standard".to_string()];
        assert_eq!(
            resolve_owner(
                Some(TEAM),
                None,
                "general",
                &standard_tag,
                OwnerDefault::Global
            ),
            Some(TEAM.to_string())
        );
    }

    #[test]
    fn resolve_owner_only_isolates_when_the_session_has_a_persona() {
        // 规则 1：会话没有人物 → 全局。自带角色走这条，真值来自 Persona::memory_owner()
        for id in ["li-xian", "xiao-wen"] {
            let owner = hermes_core::persona::get(id).unwrap().memory_owner();
            assert_eq!(
                resolve_owner(owner, None, "work", &[], OwnerDefault::Session),
                None
            );
        }
        assert_eq!(
            resolve_owner(None, Some("xiao-xie"), "work", &[], OwnerDefault::Session),
            None
        );
        assert_eq!(
            resolve_owner(Some("  "), None, "work", &[], OwnerDefault::Session),
            None
        );

        // 规则 3：明确写了本会话的人物 → 归该人物（空白照同一个人算）
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                Some(" xiao-xie "),
                "work",
                &[],
                OwnerDefault::Global
            ),
            Some("xiao-xie".to_string())
        );
    }

    #[test]
    fn preferences_and_standards_never_belong_to_a_garrison() {
        // 规则 2 优先于规则 4：别名与大小写都走 zones::normalize，偏好与标准永远全局
        for zone in [
            "preferences",
            "preference",
            "core",
            "PREFERENCES",
            "standards",
            "standard",
            "Standards",
        ] {
            assert_eq!(
                resolve_owner(Some("xiao-xie"), None, zone, &[], OwnerDefault::Session),
                None,
                "zone {zone} 必须落全局"
            );
            assert_eq!(
                resolve_owner(
                    Some("xiao-xie"),
                    Some("xiao-xie"),
                    zone,
                    &[],
                    OwnerDefault::Session
                ),
                None,
                "zone {zone}：模型自己写了本人物也不许隔离"
            );
        }
        // 工作和通用区照常归工位
        assert_eq!(
            resolve_owner(Some("xiao-xie"), None, "work", &[], OwnerDefault::Session),
            Some("xiao-xie".to_string())
        );
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                None,
                "general",
                &[],
                OwnerDefault::Session
            ),
            Some("xiao-xie".to_string())
        );
    }

    #[test]
    fn a_preference_tag_alone_also_lands_global() {
        // 模型常常只打 tag 不填 zone（zone 缺省 = general）——少了这道闸，一条用户偏好
        // 就被锁进当前工位。判据与 `hermes-reflect/src/episode.rs` 对齐。
        for tag in [
            "preference",
            "Preference",
            "prefers",
            "PREFERS",
            "standard",
            "Standard",
        ] {
            assert_eq!(
                resolve_owner(
                    Some("xiao-xie"),
                    None,
                    "general",
                    &[tag.to_string()],
                    OwnerDefault::Session
                ),
                None,
                "tag {tag} 必须落全局"
            );
            // 反思路径（默认全局）不受影响，但同样不许被锁进工位
            assert_eq!(
                resolve_owner(
                    Some("xiao-xie"),
                    Some("xiao-xie"),
                    "work",
                    &[tag.to_string()],
                    OwnerDefault::Session
                ),
                None,
                "tag {tag}：模型自己写了本人物也不许隔离"
            );
        }

        // 正面控制：无关 tag + general 照常归工位，别把整条闸焊死
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                None,
                "general",
                &["news".to_string(), "formats".to_string()],
                OwnerDefault::Session
            ),
            Some("xiao-xie".to_string())
        );
        // 大小写不同的普通 tag 也不该被误判（`preference-list` 不是 preference）
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                None,
                "general",
                &["preference-list".to_string()],
                OwnerDefault::Session
            ),
            Some("xiao-xie".to_string())
        );
    }

    #[test]
    fn the_two_write_paths_differ_only_in_the_default_direction() {
        // 同一输入、两种方向：§5.2 工具路径归工位，§5.3 反思路径归全局
        assert_eq!(
            resolve_owner(Some("xiao-xie"), None, "work", &[], OwnerDefault::Session),
            Some("xiao-xie".to_string())
        );
        assert_eq!(
            resolve_owner(Some("xiao-xie"), None, "work", &[], OwnerDefault::Global),
            None
        );

        // 写了别人物：两条路径都不许落到「当前视图看不见」的地方
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                Some("wang-hai-yan"),
                "work",
                &[],
                OwnerDefault::Session
            ),
            Some("xiao-xie".to_string()),
            "工具路径夹回当前人物"
        );
        assert_eq!(
            resolve_owner(
                Some("xiao-xie"),
                Some("wang-hai-yan"),
                "work",
                &[],
                OwnerDefault::Global
            ),
            None,
            "反思路径丢回全局"
        );
    }

    #[test]
    fn the_write_guard_clamps_to_the_views_own_persona_never_to_global() {
        // 守卫的告警是 `tracing::warn!`，没有可观测的返回值；这里钉住可观测的那一面
        // （落盘归属）。日志本身靠 code review 保证（本 crate 的 dev-deps 没有捕获器，
        // 本轮也不允许改 Cargo.toml）。
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());
        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));

        // 编程错误：在本视图里塞别人物的 owner
        put(&xie, Some("wang-hai-yan"), "林碳配额口径用 2026 版");
        let landed = raw
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("2026 版"))
            .unwrap();
        assert_eq!(
            landed.frontmatter.owner.as_deref(),
            Some("xiao-xie"),
            "夹回本视图的 owner，不是全局"
        );

        // 不泄漏：别的视图看不见它，本视图看得见
        let wang = ScopedMemoryStore::new(raw.clone(), Some("wang-hai-yan".into()));
        assert!(wang.list_active().unwrap().is_empty(), "没塞进别人的口袋");
        assert_eq!(xie.list_active().unwrap().len(), 1);

        // 同视图的 owner 原样保留（反思路径自己判好了再写）
        put(&xie, Some("xiao-xie"), "林碳报告只引 IEA 的原始数据");
        let own = raw
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("IEA"))
            .unwrap();
        assert_eq!(own.frontmatter.owner.as_deref(), Some("xiao-xie"));
    }

    #[test]
    fn superseding_an_invisible_memory_never_retires_it() {
        let dir = tempdir().unwrap();
        let raw = store_with(dir.path());
        put(
            raw.as_ref(),
            Some("wang-hai-yan"),
            "海燕：新闻稿标题不夸张，别用感叹号",
        );
        let wang_id = id_of(raw.as_ref(), "标题不夸张");
        put(raw.as_ref(), None, "全局：交付要 Word 放桌面，别给 PDF");
        let global_id = id_of(raw.as_ref(), "Word 放桌面");

        let xie = ScopedMemoryStore::new(raw.clone(), Some("xiao-xie".into()));
        let mut fm =
            MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into())
                .owned(Some("xiao-xie".into()));
        fm.supersedes = vec![wang_id.clone(), global_id.clone()];
        xie.put(Scope::User, fm, "林碳合并稿：配额口径统一用 2026 版")
            .unwrap();

        // 别人的 id 被静默丢弃：写没失败，但那条记忆对**所有**视图都还 active
        assert!(
            raw.list_active()
                .unwrap()
                .iter()
                .any(|m| m.frontmatter.id == wang_id),
            "跨工位退役必须无效——`list_active` 的退役过滤是全库的"
        );
        // 正面控制：看得见的 id 照常退役，文件留在磁盘上
        assert!(
            !raw.list_active()
                .unwrap()
                .iter()
                .any(|m| m.frontmatter.id == global_id),
            "看得见的 id 必须照常退役"
        );
        assert!(raw.get(&global_id).unwrap().is_some());
        // 丢掉的 id 不写进 frontmatter：不在盘上留「我退役过一个看不见的 id」的痕迹
        let saved = raw
            .list()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains("2026 版"))
            .unwrap();
        assert_eq!(saved.frontmatter.supersedes, vec![global_id]);
    }
}
