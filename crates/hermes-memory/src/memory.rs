//! Memory data types.
//!
//! A "memory" is a single durable fact / lesson / convention the agent has
//! decided is worth keeping across sessions. One memory = one short
//! statement. Aggregation, summarisation, and conflict resolution happen at
//! the curation layer (later), not by stuffing many points into one file.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a memory lives on disk. Mirrors `hermes_skills::Scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// `~/.lebi-ai/memories/`
    User,
    /// `./.lebi-ai/memories/`
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Distilled by the reflection pipeline.
    Reflection,
    /// Written or edited by a human.
    User,
    /// Imported from another project / agent.
    Imported,
}

/// Confidence in a memory, ordered `Low < Medium < High` (the derived
/// `Ord` follows declaration order). Used to gate reflection auto-accept
/// against a configurable minimum threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl std::str::FromStr for Confidence {
    type Err = ();

    /// Parse a case-insensitive `low` / `medium` / `high` label. Used to read
    /// the auto-accept threshold from config (stored as a string so the LLM
    /// provider crate need not depend on this one).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "low" => Ok(Confidence::Low),
            "medium" => Ok(Confidence::Medium),
            "high" => Ok(Confidence::High),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Stable identifier. Use [`MemoryFrontmatter::new`] to allocate.
    pub id: String,

    pub created: chrono::DateTime<chrono::Utc>,

    pub source: Source,

    pub confidence: Confidence,

    /// `true` ⇒ load on every session (analogous to Claude Code's `core.md`).
    /// `false` ⇒ episodic; included by relevance only.
    #[serde(default)]
    pub pinned: bool,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "default_zone")]
    pub zone: String,

    /// IDs of older memories this one replaces. Listed by [`list_active`]
    /// causes those ids to be filtered out.
    ///
    /// [`list_active`]: crate::store::MemoryStore::list_active
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,

    /// 记忆归属（人物 id / 项目组 id）。`None` = 全局：关于用户本人的偏好与标准。
    /// 有值 = 这顶帽子的专业口径，或这个栏目的标准。见 `docs/spec/personas.md` §5、
    /// `docs/spec/projects.md` §4.2。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// **因为什么**（`docs/spec/projects.md` §5.1 规矩 2：每条标准要能说出谁说的、
    /// 什么时候、因为什么）。写侧只有候选那一路填它；旧文件没有这一栏 → `None`，
    /// 不补、不猜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub because: Option<String>,

    /// **这一次写入的意图**：调用方明确在改稿 / 合并 / 解冲突，不是随手记一条。
    ///
    /// 它描述的是「这次调用」，不是「这条记忆」，所以**不落盘**
    /// （`serde(skip)`）—— 从文件里读回来的永远是从零开始的普通写入。
    /// 查重闸门看它（`store::FsMemoryStore::put`）：有意替换时放行，
    /// 因为那正是**该**写下去的时候。
    #[serde(skip)]
    pub intentional: bool,

    /// Forward-compat for fields we don't model yet.
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

fn default_zone() -> String {
    "general".to_string()
}

impl MemoryFrontmatter {
    /// Build a fresh memory with a UUIDv4-based id and `created = now`.
    pub fn new(source: Source, confidence: Confidence, tags: Vec<String>, zone: String) -> Self {
        Self {
            id: format!("mem_{}", uuid::Uuid::new_v4().simple()),
            created: chrono::Utc::now(),
            source,
            confidence,
            pinned: false,
            tags,
            zone: hermes_core::companion::zones::normalize(&zone).to_string(),
            supersedes: Vec::new(),
            owner: None,
            because: None,
            intentional: false,
            extra: serde_yaml::Mapping::new(),
        }
    }

    /// 给这条记忆打归属标签。`None` = 全局。
    ///
    /// 该传什么由 [`crate::resolve_owner`] 决定（归属的唯一判定点）；调用方不要
    /// 自己拼字符串。
    pub fn owned(mut self, owner: Option<String>) -> Self {
        self.owner = owner;
        self
    }

    /// 声明「这一条是有意写下去的」（改稿 / 合并 / 解冲突）。见 [`Self::intentional`]。
    pub fn intentional(mut self) -> Self {
        self.intentional = true;
        self
    }
}

#[derive(Debug, Clone)]
pub struct LoadedMemory {
    pub frontmatter: MemoryFrontmatter,
    pub body: String,
    pub source_path: PathBuf,
    pub scope: Scope,
}

impl LoadedMemory {
    pub fn id(&self) -> &str {
        &self.frontmatter.id
    }
}

/// 空串 / 纯空白不算归属（Ruling 1.4 / 1.5-d）：`None` = 全局是 §5.1 的原话。
/// 首尾空白一并去掉——读侧是精确匹配，`Some("xiao-xie ")` 与 `Some("xiao-xie")`
/// 必须是同一个人物，否则那条记忆对**所有**视图都不可见（文件还在、不报错、
/// 也没人知道）。
///
/// 这是**唯一的归一化实现**，读（[`visible_to`]）写（`resolve_owner` /
/// `ScopedMemoryStore`）两侧共用，不许再抄一份。
pub(crate) fn normalize_owner(owner: Option<&str>) -> Option<String> {
    owner
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
}

/// 注入隔离：全局 + 本人物的可见；别人的一条不给
/// （`docs/spec/personas.md` §5.2；管理页不受此限——「注入隔离，管理可见」）。
///
/// `owner` 是**调用方（观察者）的人物 id**，即"当前会话是谁"，**不是**记忆的归属方
/// ——归属看的是 `m.frontmatter.owner`。`None` = 当前无人物（未指定人物 / 自带角色
/// 的调用方），此时**只看得到全局记忆**：归属为 `None` 的记忆谁都看得见，
/// 归属有值则只对同一个人物可见。
///
/// 两侧都过 [`normalize_owner`]：设置 owner 只能走 `resolve_owner`；这里的归一化
/// 是**防御性**的，不是第二个判定点。
pub fn visible_to(m: &LoadedMemory, owner: Option<&str>) -> bool {
    match normalize_owner(m.frontmatter.owner.as_deref()) {
        None => true,
        Some(owned) => Some(owned.as_str()) == normalize_owner(owner).as_deref(),
    }
}

/// [`visible_to`] 的切片版本，保持入参顺序，供上下文装配直接使用。
///
/// `owner` 语义同 [`visible_to`]：调用方（当前会话）的人物 id；
/// `None` = 无人物，结果里只留全局记忆。
pub fn filter_visible<'a>(items: &'a [LoadedMemory], owner: Option<&str>) -> Vec<&'a LoadedMemory> {
    items.iter().filter(|m| visible_to(m, owner)).collect()
}

/// 这是第几版：沿 `supersedes` 链往前数（1 = 没见过更早的那一版）。
///
/// 版本**不是**一个状态字段，就是链本身（`docs/spec/projects.md` §5.1 规矩 2）。
/// 所以删掉新版，上一版自己回到 active——「退回上一版」不需要第二个机制。
/// 链断了（旧版被删过）就停在能数到的地方；有环也停（不许死循环）。
pub fn version_of(m: &LoadedMemory, all: &[LoadedMemory]) -> usize {
    let by_id: HashMap<&str, &LoadedMemory> = all.iter().map(|x| (x.id(), x)).collect();
    let mut seen: HashSet<&str> = HashSet::from([m.id()]);
    let mut cursor = m;
    let mut version = 1;
    while let Some(prev_id) = cursor.frontmatter.supersedes.first() {
        let Some(prev) = by_id.get(prev_id.as_str()) else {
            break;
        };
        if !seen.insert(prev.id()) {
            break;
        }
        version += 1;
        cursor = prev;
    }
    version
}

/// 一组归属的可见面（`docs/spec/projects.md` §4.2）：**任一命中即可见**。
///
/// 项目组会话要同时看见「本项目组」和「这一轮说话的人」——手艺与组规是两条轴，
/// 用单 owner 的接口就得二选一。判据仍是 [`visible_to`]，这里只加一层「或」：
/// 不许在别处再写一遍「全局永远可见」这条规矩。
pub fn visible_to_any(m: &LoadedMemory, owners: &[&str]) -> bool {
    match owners.first() {
        // 空 = 没有归属（无人物 / 自带角色）：与单 owner 的 `None` 同义，**只看得到全局**。
        // 少了这一支，空集会变成「谁都看不见」——全局记忆会凭空消失。
        None => visible_to(m, None),
        Some(_) => owners.iter().any(|o| visible_to(m, Some(o))),
    }
}

/// [`filter_visible`] 的一组归属版本，保持入参顺序。
pub fn filter_visible_any<'a>(items: &'a [LoadedMemory], owners: &[&str]) -> Vec<&'a LoadedMemory> {
    items.iter().filter(|m| visible_to_any(m, owners)).collect()
}

/// [`filter_visible`] 的 owned 版：调用方要把「这个视图看得见的那一份」当
/// `&[LoadedMemory]` 传下去时用它（整理主题卡要把可见集合喂给模型）。判定仍只有
/// [`visible_to`] 一处。
pub fn visible_owned(active: &[LoadedMemory], owner: Option<&str>) -> Vec<LoadedMemory> {
    filter_visible(active, owner).into_iter().cloned().collect()
}

/// [`visible_owned`] 的一组归属版本。
pub fn visible_owned_any(active: &[LoadedMemory], owners: &[&str]) -> Vec<LoadedMemory> {
    filter_visible_any(active, owners)
        .into_iter()
        .cloned()
        .collect()
}

/// 注入面要的那一份：`(active, 其中的 pinned)`。
///
/// 「提示词注入 / 显示面 / 子代理看到的是同一份」那条线就靠它——它是这条轴上
/// **唯一**的装配函数，CLI 与 GUI 都从这里拿，谁也不许自己写一遍（写两遍就会
/// 有一天只改一处，某个人物开始看见别人的记忆）。
/// 判据仍只有 [`visible_to`] 一处（经 [`visible_to_any`]），本函数只负责切出这一份。
pub fn views_for_any(
    active: &[LoadedMemory],
    owners: &[&str],
) -> (Vec<LoadedMemory>, Vec<LoadedMemory>) {
    let view = visible_owned_any(active, owners);
    let pinned = view
        .iter()
        .filter(|m| m.frontmatter.pinned)
        .cloned()
        .collect();
    (view, pinned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本 = `supersedes` 链的长度；链断了/成环也不许死循环。
    #[test]
    fn version_counts_the_supersedes_chain() {
        let mut first = mem(None, "先这么干");
        let mut second = mem(None, "改成这么干");
        second.frontmatter.supersedes = vec![first.frontmatter.id.clone()];
        let mut third = mem(None, "又改回来");
        third.frontmatter.supersedes = vec![second.frontmatter.id.clone()];
        let all = vec![first.clone(), second.clone(), third.clone()];

        assert_eq!(version_of(&first, &all), 1, "没见过更早的 = 第 1 版");
        assert_eq!(version_of(&second, &all), 2);
        assert_eq!(version_of(&third, &all), 3);

        // 旧版被删过（链断）→ 停在能数到的地方，不报错
        assert_eq!(version_of(&second, &[second.clone()]), 2 - 1);
        // 环 → 停（不许死循环）
        first.frontmatter.supersedes = vec![third.frontmatter.id.clone()];
        let cyc = vec![first.clone(), third.clone()];
        assert!(version_of(&third, &cyc) >= 1);
    }

    /// 「因为什么」跟着候选落盘（§5.1 规矩 2）；旧文件没有这一栏就是 `None`。
    #[test]
    fn because_is_optional_and_never_invented() {
        let m = mem(None, "空文件照旧");
        assert_eq!(m.frontmatter.because, None);
        let parsed: MemoryFrontmatter = serde_yaml::from_str(
            "id: mem_x\ncreated: 2026-09-17T00:00:00Z\nsource: reflection\nconfidence: high\n",
        )
        .expect("老文件必须还能读");
        assert_eq!(parsed.because, None, "缺这一栏不许报错、不许编");
    }

    #[test]
    fn confidence_is_ordered_low_to_high() {
        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
        // The auto-accept gate is `candidate >= threshold`.
        assert!(Confidence::High >= Confidence::Medium);
        assert!(Confidence::Medium >= Confidence::Medium);
        assert!(Confidence::Low < Confidence::Medium);
    }

    #[test]
    fn confidence_parses_case_insensitively() {
        assert_eq!("low".parse(), Ok(Confidence::Low));
        assert_eq!("Medium".parse(), Ok(Confidence::Medium));
        assert_eq!("HIGH".parse(), Ok(Confidence::High));
        assert_eq!("  medium  ".parse(), Ok(Confidence::Medium));
        assert_eq!("".parse::<Confidence>(), Err(()));
        assert_eq!("bogus".parse::<Confidence>(), Err(()));
    }

    /// 测试 helper：造一条带归属的记忆。
    fn mem(owner: Option<&str>, body: &str) -> LoadedMemory {
        LoadedMemory {
            frontmatter: MemoryFrontmatter::new(
                Source::User,
                Confidence::High,
                vec![],
                "general".into(),
            )
            .owned(owner.map(str::to_string)),
            body: body.to_string(),
            source_path: std::path::PathBuf::from("<test>"),
            scope: Scope::User,
        }
    }

    #[test]
    fn injection_is_isolated_but_globals_travel() {
        let items = vec![
            mem(None, "交付要 Word 放桌面"),
            mem(Some("xiao-xie"), "林碳报告只引 IEA"),
            mem(Some("wang-hai-yan"), "标题不夸张"),
        ];
        let ids = |o: Option<&str>| -> Vec<String> {
            filter_visible(&items, o)
                .iter()
                .map(|m| m.body.clone())
                .collect()
        };
        assert_eq!(
            ids(Some("xiao-xie")),
            vec!["交付要 Word 放桌面", "林碳报告只引 IEA"]
        );
        assert_eq!(
            ids(Some("wang-hai-yan")),
            vec!["交付要 Word 放桌面", "标题不夸张"]
        );
        assert_eq!(
            ids(None),
            vec!["交付要 Word 放桌面"],
            "无人物 / 自带角色只看全局"
        );
    }

    #[test]
    fn a_padded_owner_is_the_same_persona_and_blank_is_global() {
        // 尾空格 / 前导空格：读侧精确匹配会把 `"xiao-xie "` 变成「对所有视图都不可见」
        // ——文件还在、不报错、也没人知道。归一化读侧也要做（Ruling 1.5-d）。
        let items = vec![
            mem(Some("xiao-xie "), "林碳报告只引 IEA"),
            mem(Some("  wang-hai-yan"), "标题不夸张"),
            mem(Some("   "), "交付要 Word 放桌面"),
            mem(Some(""), "输出一律给 md"),
        ];
        let ids = |o: Option<&str>| -> Vec<String> {
            filter_visible(&items, o)
                .iter()
                .map(|m| m.body.clone())
                .collect()
        };
        let globals = vec!["交付要 Word 放桌面", "输出一律给 md"];
        let mut xie = vec!["林碳报告只引 IEA"];
        xie.extend(globals.clone());
        let mut haiyan = vec!["标题不夸张"];
        haiyan.extend(globals.clone());

        assert_eq!(ids(Some("xiao-xie")), xie, "尾空格与干净 id 是同一顶帽子");
        assert_eq!(ids(Some(" xiao-xie ")), xie, "看的人自己带空格也算同一顶");
        assert_eq!(ids(Some("wang-hai-yan")), haiyan);
        assert_eq!(ids(None), globals, "空串 / 纯空白 = 全局（§5.1）");
        assert!(
            visible_to(&items[0], Some("xiao-xie")),
            "尾空格那条读得回来"
        );
        assert!(!visible_to(&items[0], None), "有值就不是全局");
    }

    #[test]
    fn frontmatter_without_owner_key_parses_as_global() {
        // 老文件（本轴引入前写出）没有 owner 键，必须仍然读得进来且算全局。
        let yaml = "id: mem_legacy01\ncreated: 2026-01-01T00:00:00Z\nsource: user\nconfidence: high\nzone: general\n";
        let fm: MemoryFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.owner, None);

        // 用刚解析出来的 fm 造记忆，才能锁住"老文件读进来算全局"这层。
        let legacy = LoadedMemory {
            frontmatter: fm,
            body: "老记忆".to_string(),
            source_path: std::path::PathBuf::from("<legacy>"),
            scope: Scope::User,
        };
        assert!(visible_to(&legacy, None), "无人物也看得见全局记忆");
        assert!(
            visible_to(&legacy, Some("xiao-xie")),
            "全局记忆进入每个人物会话"
        );
    }

    #[test]
    fn owner_defaults_to_global_and_round_trips() {
        let fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into());
        assert_eq!(fm.owner, None, "没有 owner = 全局，已有记忆零迁移");

        let yaml = serde_yaml::to_string(&fm).unwrap();
        assert!(!yaml.contains("owner"), "全局记忆不该写出 owner 键");

        let scoped = fm.owned(Some("xiao-xie".into()));
        assert_eq!(scoped.owner.as_deref(), Some("xiao-xie"));
        let yaml = serde_yaml::to_string(&scoped).unwrap();
        assert!(yaml.contains("xiao-xie"));
        let back: MemoryFrontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.owner.as_deref(), Some("xiao-xie"));
    }
}
