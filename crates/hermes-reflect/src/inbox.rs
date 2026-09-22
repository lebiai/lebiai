//! Pending-review inbox: quiet evolution queue.
//!
//! Reflection extracts candidates in the background; they land here for
//! batch review. Default UX: no modal on leave — badge + panel only.
//!
//! File: `~/.lebi-ai/pending-review.json`

use std::fs;
use std::path::PathBuf;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use hermes_memory::{MemoryStore, MemoryStoreError};
use serde::{Deserialize, Serialize};

use crate::candidate;
use crate::episode::{episode_is_self_contained, is_internal_noise_text, is_work_episode};
use crate::output::{MemoryCandidate, ReflectionOutput, SkillCandidate};

const MAX_ITEMS: usize = 100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InboxSource {
    SessionEnd,
    Micro,
    ManualReflect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxPayload {
    Memory(MemoryCandidate),
    Skill(SkillCandidate),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub source: InboxSource,
    pub fingerprint: String,
    pub payload: InboxPayload,
    /// Distill run that produced this item (not shown in the review UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 入队时所在会话的工位归属（`SessionMeta.persona` → `memory_owner_for`）。
    /// 批准时用它判落盘归属——批准路径不反查会话文件（Task 1.8b）。
    /// `None` = 没有工位 / 自带角色 / 入队时取不到会话 → 落全局。
    ///
    /// 老文件没有这个键，`default` 让它照样读进来（安全方向：落全局）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_at: Option<String>,
}

/// Marks stamped on a full-session distill so a later run can replace this
/// session's previous pending items.
#[derive(Debug, Clone, Default)]
pub struct EnqueueMark {
    pub session_id: Option<String>,
    pub distill_id: Option<String>,
    pub through_at: Option<String>,
    /// 该会话的工位归属（`hermes_core::persona::memory_owner_for`）。
    pub session_owner: Option<String>,
}

impl EnqueueMark {
    /// 零散候选的标记：只记来源工位，**不**记 `session_id`。
    ///
    /// `session_id` 不是出处，它是 [`enqueue_from_reflection_marked`] 的
    /// 「替换本会话旧条目」开关：带它入队会把同会话待审的旧条目删掉。整场 distill
    /// （session-end / manual reflect）要的就是这个替换；但 micro 每批只看最新一轮、
    /// 通常只有 1 条候选，用户当场点「记住」也只是一条片段——带上 `session_id` 等于
    /// 「用户还没审，候选就被下一批吞掉」，与「nothing is lost」相冲。
    pub fn append_only(session_owner: Option<String>) -> Self {
        Self {
            session_owner,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct InboxFile {
    items: Vec<InboxItem>,
}

const FILE: &str = "pending-review.json";

/// `pending-review.json` **在给定数据根里**。待审队列只有这一份文件：
/// micro、session-end、CLI、GUI、server 全写它（P1-5：以前 CLI 另有一条
/// `deferred.jsonl`，同机同用户两条队列各自长）。
pub fn path_in(root: &std::path::Path) -> PathBuf {
    root.join(FILE)
}

fn load_file_at(root: &std::path::Path) -> Result<InboxFile> {
    let p = path_in(root);
    if !p.exists() {
        return Ok(InboxFile::default());
    }
    let raw = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
    if raw.trim().is_empty() {
        return Ok(InboxFile::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", p.display()))
}

fn save_file_at(root: &std::path::Path, file: &InboxFile) -> Result<()> {
    let p = path_in(root);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(file)?;
    // 原子写：待审队列会被后台反思并写入，写到一半断电不该把用户点过的项弄没。
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, raw).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &p).with_context(|| format!("rename {}", p.display()))
}

fn load_file() -> Result<InboxFile> {
    load_file_at(&hermes_core::data_root())
}

fn save_file(file: &InboxFile) -> Result<()> {
    save_file_at(&hermes_core::data_root(), file)
}

fn hash_str(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn fingerprint_memory(c: &MemoryCandidate) -> String {
    // 身份 = 内容 + 归属：同一句话归两个工位是**两条**记忆，不是同一条。
    // 少了 owner 这一维，两条候选会同 id，而 id 是用户点击/删除的对象——
    // 点错行 = 落盘归属写歪（规格 §5.2 风险表第一条）。
    hash_str(&format!(
        "{}|{}|{}",
        c.fact.trim(),
        c.zone.trim(),
        c.owner.as_deref().unwrap_or("")
    ))
}

fn fingerprint_skill(c: &SkillCandidate) -> String {
    let body: String = c.body.trim().chars().take(200).collect();
    hash_str(&format!("{}|{}", c.name.trim(), body))
}

/// 去重判据的第二半：这条旧条目**会不会继续留在待审列表里**。
///
/// [`enqueue_from_reflection_marked`] 末尾的 `retain` 只删**同会话**的旧条目，
/// 而且只在 `mark.session_id` 有值时执行：
/// - `mark_session == None`（零散候选）：一个旧条目都不动 → 全部存活；
/// - `Some(m)`：`session_id != Some(m)` 的旧条目存活（零散条目也存活）。
///
/// 只有「已经在列表里、而且这次不会被替换掉」才算重复。否则本次入队会把旧条目
/// 替换掉、新条目又被误判成重复而不入队——同一会话 re-distill 出同一条记忆这条
/// 路径会变成 0 条。
fn survives_replacement(item_session: Option<&str>, mark_session: Option<&str>) -> bool {
    match mark_session {
        None => true,
        Some(m) => item_session != Some(m),
    }
}

/// Quality gate: only durable, non-noise, self-contained when episode.
pub fn memory_passes_gate(c: &MemoryCandidate) -> bool {
    let fact = c.fact.trim();
    if fact.is_empty() || fact.chars().count() < 12 {
        return false;
    }
    if is_internal_noise_text(fact) {
        return false;
    }
    if looks_like_dumped_source(fact) {
        return false;
    }
    if is_low_value_memory(fact) || hermes_memory::is_worthless_for_living(fact) {
        return false;
    }
    if is_work_episode(c) && !episode_is_self_contained(fact) {
        return false;
    }
    // Hollow template leftovers from offline fallbacks.
    if fact.contains("以当时对话中的实际操作为准")
        || fact.contains("若有文件路径以对话中写明的为准")
        || fact.contains("写入记忆时以本条为准（不依赖原会话文件）")
            && fact.contains("做法：")
            && fact.matches("做法：").count() >= 1
            && !fact.contains("outputs/")
            && !fact.contains("偏好")
    {
        // Allow only if there is a concrete path or preference signal.
        let has_path = fact.contains('/') || fact.contains(".md") || fact.contains(".docx");
        let has_pref = fact.contains("偏好") || fact.contains("习惯") || fact.contains("标准");
        if !has_path && !has_pref {
            return false;
        }
    }
    true
}

/// Long pasted clauses are materials, not a living standard.
fn looks_like_dumped_source(fact: &str) -> bool {
    if fact.chars().count() < 400 {
        return false;
    }
    let tiao = fact.matches('条').count();
    tiao >= 3 && (fact.contains("甲方") || fact.contains("乙方") || fact.contains("本合同"))
}

/// Greeting / one-shot task paste / pure URL — not worth durable memory.
fn is_low_value_memory(fact: &str) -> bool {
    let f = fact.trim();
    let lower = f.to_lowercase();
    // Pure URL-ish
    if f.starts_with("http://") || f.starts_with("https://") {
        return true;
    }
    // Greeting-only episodes
    let greet = ["你好", "您好", "hello", "hi", "嗨", "在吗", "开场寒暄"];
    if greet.iter().any(|g| f == *g || lower == *g) {
        return true;
    }
    if f.contains("开场寒暄") && f.contains("未进行任何实际工作") {
        return true;
    }
    // Template with only a raw user dump and no real method
    if f.contains("【工作情节】")
        && f.contains("以当时对话中的实际操作为准")
        && !f.contains("偏好")
        && !f.contains("标准")
        && !f.contains("outputs/")
    {
        // Title is just URL or very short task
        let first = f.lines().next().unwrap_or("");
        if first.contains("http") || first.chars().count() < 24 {
            return true;
        }
    }
    false
}

pub fn skill_passes_gate(c: &SkillCandidate) -> bool {
    !c.name.trim().is_empty() && !c.body.trim().is_empty() && c.body.trim().chars().count() >= 20
}

/// Enqueue from a reflection output. Returns how many **new** items were added.
pub fn enqueue_from_reflection(output: &ReflectionOutput, source: InboxSource) -> Result<usize> {
    enqueue_from_reflection_marked(output, source, EnqueueMark::default())
}

/// Same as [`enqueue_from_reflection`], then drop this session's older pending
/// items so a full re-distill replaces rather than stacks.
pub fn enqueue_from_reflection_marked(
    output: &ReflectionOutput,
    source: InboxSource,
    mark: EnqueueMark,
) -> Result<usize> {
    enqueue_from_reflection_marked_at(&hermes_core::data_root(), output, source, mark)
}

/// [`enqueue_from_reflection_marked`] into an explicit data root.
pub fn enqueue_from_reflection_marked_at(
    root: &std::path::Path,
    output: &ReflectionOutput,
    source: InboxSource,
    mark: EnqueueMark,
) -> Result<usize> {
    let mut file = load_file_at(root)?;
    let mut incoming: Vec<InboxItem> = Vec::new();

    for c in &output.memory_candidates {
        if !memory_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_memory(c);
        if incoming.iter().any(|i| i.fingerprint == fp)
            || file.items.iter().any(|i| {
                i.fingerprint == fp
                    && survives_replacement(i.session_id.as_deref(), mark.session_id.as_deref())
            })
        {
            continue;
        }
        incoming.push(InboxItem {
            id: format!("pend_m_{fp}"),
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Memory(c.clone()),
            distill_id: mark.distill_id.clone(),
            session_id: mark.session_id.clone(),
            session_owner: mark.session_owner.clone(),
            through_at: mark.through_at.clone(),
        });
    }

    for c in &output.skill_candidates {
        if !skill_passes_gate(c) {
            continue;
        }
        let fp = fingerprint_skill(c);
        if incoming.iter().any(|i| i.fingerprint == fp)
            || file.items.iter().any(|i| {
                i.fingerprint == fp
                    && survives_replacement(i.session_id.as_deref(), mark.session_id.as_deref())
            })
        {
            continue;
        }
        incoming.push(InboxItem {
            id: format!("pend_s_{fp}"),
            created_at: Utc::now(),
            source,
            fingerprint: fp,
            payload: InboxPayload::Skill(c.clone()),
            distill_id: mark.distill_id.clone(),
            session_id: mark.session_id.clone(),
            session_owner: mark.session_owner.clone(),
            through_at: mark.through_at.clone(),
        });
    }

    // Empty re-distill must not wipe unreviewed items from this session.
    if incoming.is_empty() {
        return Ok(0);
    }

    if let Some(sid) = mark.session_id.as_deref() {
        file.items.retain(|i| i.session_id.as_deref() != Some(sid));
    }
    let added = incoming.len();
    file.items.extend(incoming);

    if file.items.len() > MAX_ITEMS {
        let drop_n = file.items.len() - MAX_ITEMS;
        file.items.drain(0..drop_n);
    }

    save_file_at(root, &file)?;
    Ok(added)
}

/// 入队**一条**候选（micro 每批只看最新一轮，通常就 1 条）。
///
/// 走的是与整场 distill 完全相同的那道闸门：质量门槛 + 指纹去重 + 上限淘汰。
/// 质量不过关的候选**不入队**，返回 `Ok(0)` —— 「nothing is lost」指的是
/// 不静默丢弃**够格**的候选，不是把噪声也堆给用户。
pub fn enqueue_candidate(
    payload: InboxPayload,
    source: InboxSource,
    mark: EnqueueMark,
) -> Result<usize> {
    enqueue_candidate_at(&hermes_core::data_root(), payload, source, mark)
}

pub fn enqueue_candidate_at(
    root: &std::path::Path,
    payload: InboxPayload,
    source: InboxSource,
    mark: EnqueueMark,
) -> Result<usize> {
    let mut output = ReflectionOutput {
        summary: String::new(),
        skill_candidates: Vec::new(),
        memory_candidates: Vec::new(),
        conflicts: Vec::new(),
    };
    match &payload {
        InboxPayload::Memory(c) => output.memory_candidates.push(c.clone()),
        InboxPayload::Skill(c) => output.skill_candidates.push(c.clone()),
    }
    enqueue_from_reflection_marked_at(root, &output, source, mark)
}

/// Drop items that no longer pass quality gates (garbage cleanup).
pub fn prune_low_quality() -> Result<usize> {
    prune_low_quality_at(&hermes_core::data_root())
}

pub fn prune_low_quality_at(root: &std::path::Path) -> Result<usize> {
    let mut file = load_file_at(root)?;
    let before = file.items.len();
    file.items.retain(|item| match &item.payload {
        InboxPayload::Memory(c) => memory_passes_gate(c),
        InboxPayload::Skill(c) => skill_passes_gate(c),
    });
    let removed = before.saturating_sub(file.items.len());
    if removed > 0 {
        save_file_at(root, &file)?;
    }
    Ok(removed)
}

pub fn list() -> Result<Vec<InboxItem>> {
    list_at(&hermes_core::data_root())
}

pub fn list_at(root: &std::path::Path) -> Result<Vec<InboxItem>> {
    let _ = prune_low_quality_at(root);
    let mut items = load_file_at(root)?.items;
    items.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(items)
}

pub fn count() -> Result<usize> {
    let _ = prune_low_quality();
    Ok(load_file()?.items.len())
}

pub fn get(id: &str) -> Result<Option<InboxItem>> {
    Ok(load_file()?.items.into_iter().find(|i| i.id == id))
}

pub fn remove(id: &str) -> Result<bool> {
    let mut file = load_file()?;
    let before = file.items.len();
    file.items.retain(|i| i.id != id);
    if file.items.len() == before {
        return Ok(false);
    }
    save_file(&file)?;
    Ok(true)
}

/// 批准一条 inbox 记忆：判归属 → 落盘。GUI 与 server 两个门面共用这一条实现
/// （批准式落盘是主路径，每个门面各写一遍就必然有一处漏掉归属——1.8b 之前
/// GUI 与 server 两处都漏了）。CLI 的 `hermes reflect` / `review_deferred` 走
/// 自己的 `persist_memory`，但同样只把参数交给 `resolve_owner`。
///
/// 归属判定本身只有 `hermes_memory::resolve_owner` 一个地方，这里只负责把参数
/// 从**条目**里取出来：会话工位是入队时记下的（`session_owner`），不反查会话文件。
/// 返回 `Ok(None)` = **库里已经有一条同样的**：不重复落盘，但这条待审算处理完了
/// （条目仍会从队列里移除）。查重闸门在 `put` 里（P1-4），这里只把「重复」翻译成
/// 一个正常的结局，而不是一个错误 —— 用户点的是「我同意」，不是「再存一份」。
pub fn accept_memory_item(
    store: &dyn MemoryStore,
    item: &InboxItem,
    c: &MemoryCandidate,
) -> Result<Option<PathBuf>, MemoryStoreError> {
    let mut fm = candidate::frontmatter_for(c, item.session_owner.as_deref());
    // 出处写进 frontmatter：回顾时说得清这条是哪一轮、哪个会话来的。
    for (key, value) in [
        ("distill_id", item.distill_id.as_deref()),
        ("source_session", item.session_id.as_deref()),
        ("through_at", item.through_at.as_deref()),
    ] {
        if let Some(v) = value {
            fm.extra.insert(
                serde_yaml::Value::String(key.to_string()),
                serde_yaml::Value::String(v.to_string()),
            );
        }
    }
    match candidate::put_with_fallback(store, c.scope, fm, &c.fact) {
        Ok(path) => Ok(Some(path)),
        Err(MemoryStoreError::Conflict { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn clear() -> Result<()> {
    clear_at(&hermes_core::data_root())
}

pub fn clear_at(root: &std::path::Path) -> Result<()> {
    let p = path_in(root);
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, FsMemoryStore, Scope};
    use std::sync::Mutex;

    /// `pending-review.json` 的落盘目标来自进程数据根（`LEBI_DATA_DIR`），
    /// 所以碰文件的测试必须串行，并在断言后还原环境
    /// （`docs/records/20260913-reflect-write-isolation.md`：测试不许写真实数据根）。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_data_dir(f: impl FnOnce()) {
        /// 断言一旦 panic，环境必须**自己**还原——否则同二进制后面的测试会看到
        /// 上一个测试留下的脏数据根（测试失败也就变成了连环误报）。
        struct Restore {
            prev: Option<String>,
            _locked: std::sync::MutexGuard<'static, ()>,
            dir: tempfile::TempDir,
        }

        impl Drop for Restore {
            fn drop(&mut self) {
                match self.prev.take() {
                    Some(v) => std::env::set_var(hermes_core::paths::ENV_DATA_DIR, v),
                    None => std::env::remove_var(hermes_core::paths::ENV_DATA_DIR),
                }
            }
        }

        let restore = Restore {
            // 中毒也要拿到锁：前一个测试失败不该让这一个跟着变红（真假难分）。
            _locked: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            prev: std::env::var(hermes_core::paths::ENV_DATA_DIR).ok(),
            dir: tempfile::tempdir().unwrap(),
        };
        std::env::set_var(hermes_core::paths::ENV_DATA_DIR, restore.dir.path());
        f();
        drop(restore);
    }

    fn candidate(fact: &str, zone: &str, owner: Option<&str>) -> MemoryCandidate {
        MemoryCandidate {
            fact: fact.into(),
            tags: vec![],
            zone: zone.into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "test".into(),
            supersedes: vec![],
            owner: owner.map(str::to_string),
        }
    }

    fn item_with(c: MemoryCandidate, session_owner: Option<&str>) -> InboxItem {
        InboxItem {
            id: "pend_m_test".into(),
            created_at: Utc::now(),
            source: InboxSource::Micro,
            fingerprint: "fp".into(),
            payload: InboxPayload::Memory(c),
            distill_id: None,
            session_id: Some("sess-1".into()),
            session_owner: session_owner.map(str::to_string),
            through_at: None,
        }
    }

    fn owner_of(store: &dyn MemoryStore, needle: &str) -> Option<String> {
        store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains(needle))
            .unwrap_or_else(|| panic!("no memory containing {needle}"))
            .frontmatter
            .owner
    }

    fn store() -> (tempfile::TempDir, FsMemoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = FsMemoryStore::new(dir.path().to_path_buf(), None);
        (dir, s)
    }

    fn good_mem(fact: &str) -> MemoryCandidate {
        MemoryCandidate {
            fact: fact.into(),
            tags: vec!["work-episode".into()],
            zone: "work".into(),
            scope: Scope::User,
            confidence: Confidence::High,
            rationale: "t".into(),
            supersedes: vec![],
            owner: None,
        }
    }

    #[test]
    fn gate_rejects_care_nudge_and_hollow() {
        let bad = good_mem(
            "【工作情节】[lebi-AI Care] Tool work\n- 情境：本会话\n- 做法：见会话记录\n- 产出：见会话记录\n- 用户反馈/修正：无\n- 可复用点：x",
        );
        assert!(!memory_passes_gate(&bad));
        let good = good_mem(
            "【工作情节】季度复盘\n- 情境：用户要三段结构做复盘\n- 做法：先结论后证据再动作\n- 产出：outputs/retro.md\n- 用户反馈/修正：无\n- 可复用点：先结论后证据",
        );
        assert!(memory_passes_gate(&good));
        let dumped = good_mem(&format!(
            "本合同甲方与乙方约定如下。第一条 定义。第二条 付款。第三条 违约金。{}",
            "甲方应按第七条支付。乙方应在三十日内。本合同一式两份。".repeat(16)
        ));
        assert!(
            !memory_passes_gate(&dumped),
            "len={}",
            dumped.fact.chars().count()
        );
        let env = MemoryCandidate {
            fact: "本机 Python 环境已安装 python-docx 1.2.0，可用来生成 .docx".into(),
            tags: vec!["environment".into()],
            zone: "general".into(),
            scope: Scope::User,
            confidence: Confidence::Medium,
            rationale: "env".into(),
            supersedes: vec![],
            owner: None,
        };
        assert!(!memory_passes_gate(&env));
    }

    #[test]
    fn fingerprint_dedups() {
        let a = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        let b = fingerprint_memory(&good_mem("same fact body for dedup testing xx"));
        assert_eq!(a, b);
    }

    /// 工单验证点 ①（Task 1.11）：零散候选（`session_id = None`）同一指纹只许进一条。
    /// 今天的去重条件在两边都是 `None` 时**不比对**，会插两条**同 id** 条目，而
    /// `inbox_remove` 是 `retain(id != id)`——用户看到两行、点一行却删两行。
    #[test]
    fn the_same_loose_candidate_is_queued_once() {
        with_data_dir(|| {
            let batch = |fact: &str| ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(fact, "general", Some("xiao-xie"))],
                conflicts: vec![],
            };
            // 两处 micro 调用点就是这么建标记的（`append_only`：不带 `session_id`）。
            let mark = || EnqueueMark::append_only(Some("xiao-xie".into()));

            let first = enqueue_from_reflection_marked(
                &batch("林碳报告只引 IEA 的数据口径"),
                InboxSource::Micro,
                mark(),
            )
            .unwrap();
            let second = enqueue_from_reflection_marked(
                &batch("林碳报告只引 IEA 的数据口径"),
                InboxSource::Micro,
                mark(),
            )
            .unwrap();

            assert_eq!(first, 1);
            assert_eq!(second, 0, "同一条零散候选不许插第二遍");
            assert_eq!(list().unwrap().len(), 1, "待审列表里只该有一条");
        });
    }

    /// 工单验证点 ②（Task 1.11）：同一句话在两个工位各被反思出来，是**两条**记忆。
    /// 指纹不含 owner 时两条会同 id、各自带不同的 `session_owner`；用户点哪一行决定
    /// 落盘归属——点错行就把归属写歪（规格 §5.2 风险表第一条）。
    #[test]
    fn the_same_fact_in_two_workspaces_is_two_items_with_two_ids() {
        with_data_dir(|| {
            let batch = |owner: &str| ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(
                    "林碳报告只引 IEA 的数据口径",
                    "general",
                    Some(owner),
                )],
                conflicts: vec![],
            };
            for owner in ["xiao-xie", "zhang-san"] {
                let added = enqueue_from_reflection_marked(
                    &batch(owner),
                    InboxSource::Micro,
                    EnqueueMark::append_only(Some(owner.into())),
                )
                .unwrap();
                assert_eq!(added, 1, "工位 {owner} 的候选必须入队");
            }

            let items = list().unwrap();
            assert_eq!(items.len(), 2, "两个工位各一条：{items:?}");
            let ids: std::collections::HashSet<_> = items.iter().map(|i| i.id.clone()).collect();
            assert_eq!(ids.len(), 2, "同内容不同工位必须是两个不同 id：{ids:?}");
            let owners: std::collections::HashSet<_> =
                items.iter().map(|i| i.session_owner.clone()).collect();
            assert!(owners.contains(&Some("xiao-xie".to_string())));
            assert!(owners.contains(&Some("zhang-san".to_string())));
        });
    }

    /// 工单验证点 ③（Task 1.11 回归）：同一会话 re-distill 出**同一条**记忆时，
    /// `retain` 会替换掉旧条目，所以这条必须**照常入队**。若把去重改成「同指纹即跳过」，
    /// 旧条目先被 `retain` 删掉、新条目又被跳过——这条路会变成 0 条。
    #[test]
    fn a_whole_session_redistill_of_the_same_fact_is_replaced_not_dropped() {
        with_data_dir(|| {
            let batch = ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(
                    "林碳报告只引 IEA 的数据口径",
                    "general",
                    Some("xiao-xie"),
                )],
                conflicts: vec![],
            };
            let mark = || EnqueueMark {
                session_id: Some("sess-1".into()),
                session_owner: Some("xiao-xie".into()),
                ..Default::default()
            };

            let first =
                enqueue_from_reflection_marked(&batch, InboxSource::SessionEnd, mark()).unwrap();
            let second =
                enqueue_from_reflection_marked(&batch, InboxSource::SessionEnd, mark()).unwrap();

            assert_eq!(first, 1);
            assert_eq!(
                second, 1,
                "同会话同一条必须仍然入队（旧条目由 retain 替换）"
            );
            assert_eq!(list().unwrap().len(), 1, "替换之后仍只有一条");
        });
    }

    /// 工单验证点 ①：模型判对的专业口径，在用户点头那一刻不许丢成全局。
    #[test]
    fn an_approved_item_keeps_the_workspace_it_was_queued_from() {
        let (_d, store) = store();
        let item = item_with(
            candidate("林碳报告只引 IEA 的数据口径", "general", Some("xiao-xie")),
            Some("xiao-xie"),
        );
        let InboxPayload::Memory(c) = &item.payload else {
            panic!("memory payload");
        };
        accept_memory_item(&store, &item, c).unwrap();
        assert_eq!(
            owner_of(&store, "IEA").as_deref(),
            Some("xiao-xie"),
            "批准后归属必须还是入队时那个工位"
        );
    }

    /// 工单验证点 ②：Ruling 1.6-c 的偏好闸在批准路径同样生效——模型把用户偏好
    /// 挂到某工位名下，用户点同意也不许落成那个工位私有。
    #[test]
    fn a_user_preference_stays_global_even_when_the_workspace_is_stamped() {
        let (_d, store) = store();
        let mut by_zone = candidate(
            "以后交付一律用 Word 文件、放桌面",
            "preferences",
            Some("xiao-xie"),
        );
        by_zone.tags = vec!["preference".into()];
        let mut by_tag = candidate("会议纪要当天给结论，不隔夜", "general", Some("xiao-xie"));
        by_tag.tags = vec!["prefers".into()];
        let mut by_standard =
            candidate("同一篇稿件的数字都要标出处", "standards", Some("xiao-xie"));
        by_standard.tags = vec!["standard".into()];

        for c in [by_zone, by_tag, by_standard] {
            let needle = c.fact.clone();
            let item = item_with(c, Some("xiao-xie"));
            let InboxPayload::Memory(c) = &item.payload else {
                panic!("memory payload");
            };
            accept_memory_item(&store, &item, c).unwrap();
            assert_eq!(
                owner_of(&store, &needle),
                None,
                "偏好 / 标准一律落全局：{needle}"
            );
        }
    }

    /// 工单验证点 ③：取不到来源工位 → 落全局（安全方向），且不 panic。
    #[test]
    fn without_a_source_workspace_the_item_lands_globally() {
        let (_d, store) = store();
        let item = item_with(
            candidate(
                "季度复盘先给结论再给证据再给动作",
                "general",
                Some("xiao-xie"),
            ),
            None,
        );
        let InboxPayload::Memory(c) = &item.payload else {
            panic!("memory payload");
        };
        accept_memory_item(&store, &item, c).unwrap();
        assert_eq!(
            owner_of(&store, "复盘"),
            None,
            "没有来源会话就宁可落全局，不许顺着模型写的 id 存成工位私有"
        );
    }

    /// 工单验证点 ④：老文件（没有 `session_owner` 键）照样读进来 → `None`。
    #[test]
    fn an_old_pending_review_file_without_the_new_key_still_reads() {
        with_data_dir(|| {
            let old = r#"{"items":[{"id":"pend_m_old","created_at":"2026-01-02T03:04:05Z",
                "source":"micro","fingerprint":"deadbeef00000000","session_id":"sess-old",
                "payload":{"kind":"memory","fact":"旧文件里的候选，没有工位键",
                "tags":[],"zone":"general","scope":"user","confidence":"high",
                "rationale":"old","supersedes":[]}}]}"#;
            std::fs::write(path_in(&hermes_core::data_root()), old).unwrap();

            let item = get("pend_m_old").unwrap().expect("老条目必须读得出来");
            assert!(item.session_owner.is_none(), "老文件没有这个键 → None");
            assert_eq!(item.session_id.as_deref(), Some("sess-old"));
            let InboxPayload::Memory(c) = &item.payload else {
                panic!("memory payload");
            };
            assert_eq!(c.fact, "旧文件里的候选，没有工位键");
        });
    }

    /// 工单验证点 ⑤：端到端——入队（带工位）→ 批准 → 落盘归属正确。
    #[test]
    fn a_queued_candidate_lands_in_its_workspace_after_approval() {
        with_data_dir(|| {
            let output = ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(
                    "林碳报告只引 IEA 的数据口径",
                    "general",
                    Some("xiao-xie"),
                )],
                conflicts: vec![],
            };
            let added = enqueue_from_reflection_marked(
                &output,
                InboxSource::Micro,
                EnqueueMark {
                    session_id: Some("sess-1".into()),
                    session_owner: Some("xiao-xie".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(added, 1);

            let item = list().unwrap().pop().expect("刚入队的条目");
            assert_eq!(item.session_owner.as_deref(), Some("xiao-xie"));
            let InboxPayload::Memory(c) = &item.payload else {
                panic!("memory payload");
            };
            let (_d, store) = store();
            let written = accept_memory_item(&store, &item, c).unwrap();
            let written = written.expect("批准必须真落盘");
            assert!(written.exists(), "批准必须真落盘：{}", written.display());
            assert_eq!(owner_of(&store, "IEA").as_deref(), Some("xiao-xie"));
        });
    }

    /// micro 一批只看最新一轮、通常只有 1 条候选，默认又要人批准——它**必须**
    /// 只叠加、不许替换：标记带上 `session_id` 就等于「用户还没审，候选被下一批
    /// 吞掉」（这就是 `EnqueueMark::append_only` 存在的理由）。
    #[test]
    fn micro_candidates_are_not_swallowed_by_the_next_batch() {
        with_data_dir(|| {
            let batch = |fact: &str| ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(fact, "general", None)],
                conflicts: vec![],
            };
            // 两处 micro 调用点就是这么建标记的。
            let mark = || EnqueueMark::append_only(Some("xiao-xie".into()));

            enqueue_from_reflection_marked(
                &batch("上一轮留下的候选：先给结论再给证据"),
                InboxSource::Micro,
                mark(),
            )
            .unwrap();
            enqueue_from_reflection_marked(
                &batch("最新一轮的候选：复盘先给结论"),
                InboxSource::Micro,
                mark(),
            )
            .unwrap();

            let facts = facts();
            assert!(
                facts.iter().any(|f| f.contains("上一轮")),
                "上一批还没审，不许被下一批吞掉：{facts:?}"
            );
            assert!(facts.iter().any(|f| f.contains("最新一轮")));
        });
    }

    /// 反面：整场 distill（session-end / manual reflect）的标记**就是要替换**——
    /// 这正是 micro 不能跟着带 `session_id` 的原因。
    #[test]
    fn a_whole_session_distill_replaces_its_older_items() {
        with_data_dir(|| {
            let batch = |fact: &str| ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate(fact, "general", None)],
                conflicts: vec![],
            };
            let mark = || EnqueueMark {
                session_id: Some("sess-1".into()),
                session_owner: Some("xiao-xie".into()),
                ..Default::default()
            };

            enqueue_from_reflection_marked(
                &batch("整场 distill 的旧一批：先给结论再给证据"),
                InboxSource::SessionEnd,
                mark(),
            )
            .unwrap();
            enqueue_from_reflection_marked(
                &batch("整场 distill 的新一批：复盘先给结论"),
                InboxSource::SessionEnd,
                mark(),
            )
            .unwrap();

            let facts = facts();
            assert!(
                !facts.iter().any(|f| f.contains("旧一批")),
                "整场 distill 替换本会话旧条目：{facts:?}"
            );
            assert!(facts.iter().any(|f| f.contains("新一批")));
        });
    }

    /// 线格式：`None` 不许在 `pending-review.json` 里长出一个键。
    #[test]
    fn a_missing_workspace_writes_no_key() {
        with_data_dir(|| {
            let output = ReflectionOutput {
                summary: String::new(),
                skill_candidates: vec![],
                memory_candidates: vec![candidate("季度复盘先给结论再给证据", "general", None)],
                conflicts: vec![],
            };
            enqueue_from_reflection_marked(
                &output,
                InboxSource::Micro,
                EnqueueMark::append_only(None),
            )
            .unwrap();

            let raw = std::fs::read_to_string(path_in(&hermes_core::data_root())).unwrap();
            assert!(
                !raw.contains("session_owner"),
                "没有工位就别写这个键：{raw}"
            );
        });
    }

    /// 当前待审条目的正文。
    fn facts() -> Vec<String> {
        list()
            .unwrap()
            .into_iter()
            .filter_map(|i| match i.payload {
                InboxPayload::Memory(c) => Some(c.fact),
                InboxPayload::Skill(_) => None,
            })
            .collect()
    }
}
