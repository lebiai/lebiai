//! Persona (人物/工位) definitions — compiled in, product-owned, never user data.
//!
//! A persona is a **hat, not a second personality**: it narrows what the
//! companion takes on and how it talks, never how it treats the user
//! (`docs/spec/personas.md` §6.3).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonaKind {
    /// 专职：越界硬拦 + 指路 + 可一键切。
    Dedicated,
    /// 兜底：不拦——"其他人物不干的"就是它的活。
    Fallback,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Persona {
    pub id: String,
    pub name: String,
    /// 别人指路时怎么称呼它（`name` 之外的简称）。**下线之后这份名单就是禁区**：
    /// 活的定义正文里不许再出现——`no_live_station_points_at_a_retired_one` 只查
    /// `name` 会空转（指路写的是简称，不是全名；2026-09-16 实测全绿但漏了 7 处）。
    #[serde(default)]
    pub aka: Vec<String>,
    pub role: String,
    pub voice: String,
    /// 这个工位能广告的技能（规格 §6.1②）。**没写 = 不收窄**（今天的行为）；
    /// 写了 = 只广告名单里这些，外加内置元技能；写成空名单 = 只剩内置元技能。
    /// `Option` 而不是空 `Vec`，是因为「没声明」和「声明为空」必须是两回事——
    /// 用空 `Vec` 表达前者会让「谁都没绑技能」那天所有技能一起消失。
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    pub kind: PersonaKind,
    /// 自带角色：谁都有（不进授权码），且其会话产生的记忆算全局。
    #[serde(default)]
    pub builtin: bool,
    /// 暂时下线：定义留在仓库里备查，但不进名册（侧栏没有、授权码发不出、设置页看不到）。
    /// 恢复 = 删掉定义文件里的这一行；不删文件是为了「暂时」真的可逆。
    #[serde(default)]
    pub retired: bool,
    pub empty_hint: String,
    pub version: String,
    #[serde(skip)]
    pub body: String,
}

impl Persona {
    /// 会话里产生的记忆归谁。自带角色是地基不是专业分工 → 一律全局。
    pub fn memory_owner(&self) -> Option<&str> {
        if self.builtin {
            None
        } else {
            Some(self.id.as_str())
        }
    }

    pub fn is_fallback(&self) -> bool {
        self.kind == PersonaKind::Fallback
    }
}

/// 定义文件。数组是**手写**的，漏登记 = 静默不发车（我们以为上了，客户端没这人），
/// 所以 `every_definition_file_is_registered_in_sources` 盯着目录与数组两边。
const SOURCES: [(&str, &str); 12] = [
    // 自带（谁都有，不进授权码）——顺序即侧栏「工位」里自带那一组的顺序。
    ("personas/li-xian.md", include_str!("personas/li-xian.md")),
    ("personas/xiao-wen.md", include_str!("personas/xiao-wen.md")),
    (
        "personas/da-dao-yan.md",
        include_str!("personas/da-dao-yan.md"),
    ),
    // 授权（进授权码的 `personas` 名单）。
    (
        "personas/wang-hai-yan.md",
        include_str!("personas/wang-hai-yan.md"),
    ),
    (
        "personas/sao-di-seng.md",
        include_str!("personas/sao-di-seng.md"),
    ),
    (
        "personas/lv-lao-shi.md",
        include_str!("personas/lv-lao-shi.md"),
    ),
    (
        "personas/xiao-song.md",
        include_str!("personas/xiao-song.md"),
    ),
    ("personas/yu-tian.md", include_str!("personas/yu-tian.md")),
    ("personas/xiao-yu.md", include_str!("personas/xiao-yu.md")),
    // 已下线：定义**必须留在 SOURCES 里**，否则「暂时下线」就变成「再也回不来」
    // （`a_retired_persona_is_off_the_roster_but_keeps_its_definition` 盯着这一条）。
    ("personas/xiao-le.md", include_str!("personas/xiao-le.md")),
    ("personas/xiao-xie.md", include_str!("personas/xiao-xie.md")),
    ("personas/xiao-jin.md", include_str!("personas/xiao-jin.md")),
];

/// 切分在 [`crate::frontmatter::split`]——人物与项目组共用那一处。
fn parse(path: &str, raw: &str) -> Persona {
    let (yaml, body) = crate::frontmatter::split(path, raw);
    let mut p: Persona =
        serde_yaml::from_str(&yaml).unwrap_or_else(|e| panic!("{path}: frontmatter 解析失败：{e}"));
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| panic!("{path}: 取不到文件名 stem"));
    assert_eq!(
        p.id,
        stem,
        "{path}: 文件名 stem（{stem}）必须等于 id（{id}）——授权码引用的是 id",
        id = p.id
    );
    p.body = body.trim().to_string();
    p
}

/// 别的 crate 的测试要造人物样本时走这里——**复用同一个 `parse`**，
/// 不允许各写各的 YAML 解析（那份判据只能有一处）。
#[doc(hidden)]
pub fn parse_for_test(path: &str, raw: &str) -> Persona {
    parse(path, raw)
}

pub fn all() -> &'static [Persona] {
    static ALL: OnceLock<Vec<Persona>> = OnceLock::new();
    ALL.get_or_init(|| {
        SOURCES
            .iter()
            .map(|(path, raw)| parse(path, raw))
            .filter(|p| !p.retired)
            .collect()
    })
}

pub fn get(id: &str) -> Option<&'static Persona> {
    all().iter().find(|p| p.id == id)
}

/// 会话的两条轴 → 记忆归属。**这条转换只许有这一处**，别在每个调用点各写一遍。
///
/// - 项目组会话（`session_team` 有值）→ 归**那个组**：组里的口径只在组内可见，
///   不许以「全局」的身份灌进每个人的工位会话（`docs/spec/projects.md` §4.2
///   「宁可窄，不要串味」）。
/// - 否则看人物：没人物 / 人物 id 不认识 / 人物是自带角色 → `None`（= 全局）。
///
/// `persona` 与 `team` 互斥（[`crate::session::SessionMeta`]）；万一两个都给，以组为准。
pub fn memory_owner_for(
    session_persona: Option<&str>,
    session_team: Option<&str>,
) -> Option<String> {
    if let Some(team) = session_team {
        return crate::team::get(team).map(|t| t.id.clone());
    }
    let persona = get(session_persona?)?;
    persona.memory_owner().map(str::to_string)
}

/// 会话的三条轴 → **这个视图看得见的那几个归属**（`docs/spec/projects.md` §4.2）。
///
/// - 项目组会话 → `[这个组, 这一轮说话的人]`：组规与**这个人的手艺**是两条轴，
///   接棒的人上桌时，他的手艺得跟着来；别人的一条都不给（注入隔离不变）。
///   `speaker` 只在他真是这张桌子上的人时才算——不许凭空长出一个组外的人。
/// - 其余（工位 / 无人物 / 自由对话）→ `memory_owner_for` 那一个值（空 = 只看全局）。
///
/// **顺序有意义**：第一个是「本会话自己」，越界写入会夹到它身上（组会话夹回组）。
/// 「会话 → 归属」的转换只有 [`memory_owner_for`] / 这里两处，且这里委托给它。
pub fn memory_owners_for(
    session_persona: Option<&str>,
    session_team: Option<&str>,
    speaker: Option<&str>,
) -> Vec<String> {
    let own = memory_owner_for(session_persona, session_team);
    let Some(team) = session_team.and_then(crate::team::get) else {
        return own.into_iter().collect();
    };
    let mut owners: Vec<String> = vec![team.id.clone()];
    let on_table = speaker.filter(|s| team.member(s).is_some() && *s != team.id);
    if let Some(s) = on_table {
        owners.push(s.to_string());
    }
    owners
}

pub fn ids() -> Vec<&'static str> {
    all().iter().map(|p| p.id.as_str()).collect()
}

pub fn builtins() -> impl Iterator<Item = &'static Persona> {
    all().iter().filter(|p| p.builtin)
}

/// 「本机其他工位」那一节。**名册是硬约束**：模型手上没有真名单时，会从唯一
/// 看得见的名字（技能索引里的 `xiao-wang` 这类）里挑一个顶上，于是把技能名当成
/// 工位指给用户（2026-09-15 实测）。给它名单，它就没得编。
fn roster_block(others: &[&Persona]) -> String {
    let mut out = String::from("## 本机其他工位\n");
    if others.is_empty() {
        out.push_str("这台机器上目前只有你这一个工位。\n");
    } else {
        for o in others {
            out.push_str(&format!("- {} · {}\n", o.name, o.role));
        }
    }
    out.push_str(
        "\n越界的活要指路时，**只能从这份名单里挑**，并且说出名字。\
         名单里没有对得上的工位，就说「这个我这儿没有对应的工位」，\
         **不许自己编一个工位名字**——用户会照着去找，然后找不到。\n",
    );
    out
}

/// 稳定层文本。调用方负责把它放在搭子协议**之后**（规格 §6.1）。
///
/// `others` = **本机开着的其他工位**（[`open`] 去掉自己）。传全量在册名单会把用户
/// 根本没有的工位指给他，等于没指；传空则模型只能拒绝、不能指路。
pub fn block(p: &Persona, others: &[&Persona]) -> String {
    format!(
        "## 你现在是谁\n\
         你是 **{name}**——{role}。\n\
         你的职责边界写在下面「我干什么 / 我不干什么」里，以那里为准。\n\
         说话方式：{voice}\n\n\
         {body}\n\n\
         {roster}\n\
         ## 人设红线\n\
         上面写的是「怎么说」和「干什么」，不是「怎么待用户」。\
         关系基调与交互结构（接住 → 说清张力 → 给选项 → 由用户定）\
         与乐彼AI 的身份协议完全一致，不得因为戴了这顶帽子就改掉。\n",
        name = p.name,
        role = p.role,
        voice = p.voice,
        body = p.body.trim(),
        roster = roster_block(others)
    )
}

/// 这一轮**谁在说**——唯一判据（`docs/spec/projects.md` §5）。
///
/// - **项目组会话**：接棒的人（`holder`，从会话事件流回放出来）；没交过棒、
///   或报了个不在这张桌子上的人 → **第一棒**接（采；不许凭空长出一个组外的人）。
/// - **工位会话**：他本人。
/// - 都不是（自由对话 / 渠道会话）：`None`。
///
/// 「谁在说」只此一处：界面上的名字、提示词里的人物块、说话人标签，读的都是它。
pub fn speaker_for(
    persona: Option<&str>,
    team: Option<&str>,
    holder: Option<&str>,
) -> Option<&'static Persona> {
    if let Some(id) = team {
        let t = crate::team::get(id)?;
        let on_table = holder.filter(|h| t.member(h).is_some()).and_then(get);
        return Some(on_table.unwrap_or_else(|| {
            get(crate::pipeline::fallback_holder(t)).unwrap_or_else(|| t.interface_persona())
        }));
    }
    get(persona?)
}

/// 人物开关的落盘位置（自带角色不占文件，这里只存**选择**）。
pub fn prefs_path() -> PathBuf {
    crate::paths::data_path("personas.json")
}

/// 这个 id 能由用户选择去留吗？自带角色永远在（不占文件），未授权的 id 不认。
pub fn is_selectable(id: &str, licensed: &BTreeSet<String>) -> bool {
    get(id).is_some_and(|p| !p.builtin) && licensed.contains(id)
}

/// 设置里勾了要显示的工位 id。文件不存在 / 读不动 / 形状不对 → 空（只剩自带）。
/// 未知 id、未授权 id、自带角色都丢弃：注册表 + 授权码是真源，这个文件只是选择。
pub fn selected_ids_at(path: &Path, licensed: &BTreeSet<String>) -> BTreeSet<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    let Ok(prefs) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return BTreeSet::new();
    };
    prefs
        .get("enabled")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|id| is_selectable(id, licensed))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 本机**开着的**工位（侧栏上真有的那几个）：自带 ∪ 授权名单里勾选过的。
/// 顺序 = [`all`] 顺序（侧栏分组顺序的源头）。
///
/// 指路只能用这份名单（规格 §2.3）。它与 GUI 的 `list_personas` 读同一个文件、
/// 同一个判据——侧栏有谁，名册里就有谁。
pub fn open_at(path: &Path, licensed: &BTreeSet<String>) -> Vec<&'static Persona> {
    let selected = selected_ids_at(path, licensed);
    all()
        .iter()
        .filter(|p| p.builtin || selected.contains(&p.id))
        .collect()
}

/// 生产入口：本机授权码 + 用户的选择 → 开着的工位。
/// 授权文件读不出来（试用期 / 没发过码）→ 只剩自带，与侧栏一致。
pub fn open() -> Vec<&'static Persona> {
    let licensed: BTreeSet<String> = crate::license::load_status()
        .map(|s| s.personas.into_iter().collect())
        .unwrap_or_default();
    open_at(&prefs_path(), &licensed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「谁在说」= 工位是他本人 / 组里是接棒的人 / 都没交过是**第一棒（采集）**。
    /// 一条判据、四处读它——所以这条断言必须钉死。
    #[test]
    fn the_baton_says_who_is_speaking() {
        assert_eq!(
            speaker_for(Some("wang-hai-yan"), None, None).unwrap().id,
            "wang-hai-yan",
            "工位会话：说话的就是这个人"
        );
        assert_eq!(
            speaker_for(None, Some("caifu-zaozhidao"), None).unwrap().id,
            "wang-hai-yan",
            "组里没交过棒：第一棒采"
        );
        assert_eq!(
            speaker_for(None, Some("caifu-zaozhidao"), Some("xiao-song"))
                .unwrap()
                .id,
            "xiao-song",
            "交了棒：接棒的人说"
        );
        assert_eq!(
            speaker_for(None, Some("caifu-zaozhidao"), Some("nobody"))
                .unwrap()
                .id,
            "wang-hai-yan",
            "报了个不在桌上的人：回落第一棒，不许凭空长人"
        );
        assert_eq!(
            speaker_for(None, Some("caifu-zaozhidao"), Some("xiao-xie"))
                .unwrap()
                .id,
            "wang-hai-yan",
            "已下线的角色不在桌上，交给他等于没人接"
        );
        assert!(speaker_for(None, None, None).is_none());
        assert!(speaker_for(None, Some("nobody"), None).is_none());
    }

    #[test]
    fn every_definition_parses_and_ids_are_unique() {
        let all = all();
        assert!(all.len() >= 4, "两个自带 + 至少两个专属");
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "人物 id 必须唯一");
        for p in all {
            assert!(
                !p.name.is_empty() && !p.role.is_empty(),
                "{} 缺 name/role",
                p.id
            );
            assert!(!p.body.trim().is_empty(), "{} 正文是空的", p.id);
        }
    }

    #[test]
    fn builtins_are_always_available_and_write_globally() {
        let lx = get("li-xian").expect("工具人李现在必须存在");
        assert!(lx.builtin && lx.kind == PersonaKind::Dedicated);
        assert_eq!(lx.memory_owner(), None, "自带角色会话产生的记忆算全局");
        let dy = get("da-dao-yan").expect("大导演必须存在");
        assert_eq!(dy.name, "大导演");
        assert_eq!(dy.kind, PersonaKind::Dedicated, "大导演是专职，越界要拦");
        assert!(dy.builtin, "大导演是自带角色：谁都有、不进授权码");
        assert_eq!(dy.memory_owner(), None, "自带的会话记忆算全局");
        assert_eq!(builtins().count(), 3, "李现、小文、大导演是自带");
    }

    #[test]
    fn a_retired_persona_is_off_the_roster_but_keeps_its_definition() {
        for id in ["xiao-le", "xiao-xie", "xiao-jin"] {
            assert!(get(id).is_none(), "下线的工位不许再出现在名册里：{id}");
            assert!(
                !ids().contains(&id),
                "ids() 是授权码与侧栏的真源，下线的 id 不能留在里面：{id}"
            );
            let raw = SOURCES
                .iter()
                .find(|(path, _)| *path == format!("personas/{id}.md").as_str())
                .unwrap_or_else(|| panic!("{id} 的定义文件仍在 SOURCES 里登记，否则回不来"))
                .1;
            assert!(
                raw.contains("retired: true"),
                "暂时下线靠 `retired: true`；文件不在 = 恢复要翻历史，不算「暂时」：{id}"
            );
        }
    }

    #[test]
    fn the_roster_has_the_six_licensed_roles_plus_the_clerk() {
        for (id, name) in [
            ("wang-hai-yan", "情报王海燕"),
            ("sao-di-seng", "产业专家扫地僧"),
            ("lv-lao-shi", "主编吕老师"),
            ("xiao-song", "记者小宋"),
            ("yu-tian", "编辑雨天"),
            ("xiao-yu", "主播小雨"),
        ] {
            let p = get(id).unwrap_or_else(|| panic!("{id} 必须存在"));
            assert_eq!(p.name, name);
            assert_eq!(p.kind, PersonaKind::Dedicated);
            assert!(!p.builtin, "{id} 是授权角色，不进自带");
            assert_eq!(p.memory_owner(), Some(id), "{id} 的会话记忆归自己");
        }

        let wen = get("xiao-wen").expect("资料员小文必须存在");
        assert_eq!(wen.name, "资料员小文");
        assert_eq!(wen.kind, PersonaKind::Dedicated, "小文是专职，越界要拦");
        assert!(wen.builtin, "小文是必备角色：谁都有、不进授权码");
        assert_eq!(wen.memory_owner(), None, "自带的会话记忆算全局");
    }

    /// 2026-09-21「素材带 ID + 索引，接力只传 ID」这条链路的四个人物怎么取料。
    ///
    /// **要治的病是「同一份东西被反复读」，不是「读」。** 实测老版本把同一份成品读了 16 次、
    /// 7.9 万字符（`docs/records/20260921-id-index-handoff.md` §0-fp）。2026-09-21 当天一度
    /// 把「整读」一刀禁掉，结果吕老师只能拿索引那半句话判 180 条——判断依据被砍掉了。
    /// 现在的口径（`docs/records/20260921-editor-reads-it-all.md`）：
    /// - **吕老师判全量 → 整读一次成品**，禁的是**重复**整读；
    /// - **索引是给小宋取料、给人翻目录的**，不是吕老师的筛面；
    /// - **ID 是纯 ASCII 的**（`<YYYYMMDD>-<lane>-<NNN>`）——机器和接力棒用的东西不掺中文。
    #[test]
    fn the_pipeline_hands_over_ids_and_only_the_editor_reads_it_all() {
        let wang = get("wang-hai-yan").expect("情报王海燕必须存在");
        for needle in [
            "index-<日期>.md",
            "ID 不进成品正文",
            "<YYYYMMDD>-<lane>-<NNN>",
            "macro|industry|market",
            "L<行号>",
        ] {
            assert!(
                wang.body.contains(needle),
                "王海燕必须落索引、给稳定 ASCII ID（缺「{needle}」）：\n{}",
                wang.body
            );
        }
        // 索引不是吕老师的筛面：王海燕契约里不许再说「吕老师筛选靠索引」。
        assert!(
            !wang.body.contains("吕老师筛选、小宋取料"),
            "王海燕契约不许再写「吕老师筛选靠索引」——吕老师整读成品：\n{}",
            wang.body
        );

        let lv = get("lv-lao-shi").expect("吕老师必须存在");
        // 判全量 → 整读一次；取料/复核走 `material_read`（行号与偏移由引擎算）。
        for needle in [
            "整读一次成品",
            "读完不回头",
            "线索」列我不拿来判进",
            "index-<日期>.md",
            "<ID> ｜ <来源>",
            "<为什么播它>",
            "有几条就写几组",
            "宏观 → 产业动态 → 资本市场",
            "material_read",
            "ID 一律写全",
            "给用户的话",
        ] {
            assert!(
                lv.body.contains(needle),
                "吕老师必须整读一次成品、判完全量、只交 ID（缺「{needle}」）：\n{}",
                lv.body
            );
        }
        // 反着盯一条：2026-09-21 中午那版把「整读」一刀禁掉，等于把判断依据砍了。
        assert!(
            !lv.body.contains("禁止整读"),
            "吕老师不许再被禁止整读——他判的是全量，禁的是重复读：\n{}",
            lv.body
        );
        // 索引是**拿 ID 和行号**的地方，不是判断依据；也不许它变回「先读索引就够了」。
        assert!(
            lv.body.contains("索引只给我 ID 和行号") || lv.body.contains("只为了拿两样东西"),
            "吕老师契约要说清索引只是 ID/行号的来源：\n{}",
            lv.body
        );
        // 新闻/理由两行是**给人看的**，也是**小宋要核的**：他拿 ID 回原文，核不上就是
        // 这两行把料带脏了。所以数字必须能指回成品那一条，指不出就不写。
        for needle in ["素材外", "指不出"] {
            assert!(
                lv.body.contains(needle),
                "吕老师的新闻/理由两行不许带素材外的事实/数字（缺「{needle}」）：\n{}",
                lv.body
            );
        }
        let song = get("xiao-song").expect("小宋必须存在");
        for needle in [
            "index-<日期>.md",
            "material_read",
            "不许整读",
            "ID ｜ 来源",
            "标题行",
            "公告池",
            "先给人看，再落盘",
            "没点头不许写文件",
            "给用户的话",
        ] {
            assert!(
                song.body.contains(needle),
                "小宋要按 ID 取料、写标题行 + 三栏目 + 公告池、先给人看再落盘（缺「{needle}」）：\n{}",
                song.body
            );
        }
        // 秒数是小宋定的，所以他不再从选题单里读秒数。
        assert!(
            !song.body.contains("序号｜ID｜秒数｜角度"),
            "小宋的选题单只剩 ID 与角度，没有秒数：\n{}",
            song.body
        );

        let yu = get("xiao-yu").expect("小雨必须存在");
        assert!(
            yu.body.contains("只读小宋交的那一份成稿")
                && yu.body.contains("不读 `index-<日期>.md`"),
            "小雨只许读成稿那一份，不许伸手进素材与索引：\n{}",
            yu.body
        );
        for needle in [
            "不点头，不落稿",
            "默认 2 分钟",
            "装不下就停下来问",
            "加到多久",
            "给用户的话",
        ] {
            assert!(
                yu.body.contains(needle),
                "小雨要默认 2 分钟、装不下先问用户加到多久、不点头不落稿、说人话（缺「{needle}」）：\n{}",
                yu.body
            );
        }
        // 2026-09-22：旧口径把口播稿钉成「成稿的 ±10%」，用户裁决换成「默认 2 分钟，
        // 装不下问用户加到多久」。旧口径不许留。
        assert!(
            !yu.body.contains("±10%"),
            "小雨的时长不许再被钉成成稿的 ±10%——默认 2 分钟、装不下问用户：\n{}",
            yu.body
        );
    }

    #[test]
    fn wang_hai_yan_hands_over_a_gradable_brief_not_thirty_titles() {
        let p = get("wang-hai-yan").expect("情报王海燕必须存在");
        assert!(p.body.contains("不设条数上限"), "不许再人为截成 30 条");
        // 2026-09-20「治新瓶颈」：墙钟 = 最慢那一条子代理，所以要摊平成分块并发；
        // 2026-09-21：块数从七缩到六 —— 原来的「1a · 宏观-官方」整块撤销（那六个官方站
        // 都在用户定稿的删除名单里，见 docs/records/20260921-source-list-unify.md）。
        // 整卷正文只在子代理那边生成一次，主循环一个字都不写。
        // 见 docs/records/20260920-collection-critical-path.md。
        for block in [
            "1-宏观",
            "2a-产业-创投科技",
            "2b-产业-综合",
            "3a-资本-监管公告",
            "3b-资本-媒体",
            "3c-资本-平台",
        ] {
            assert!(p.body.contains(block), "六块缺了「{block}」：\n{}", p.body);
        }
        assert!(
            p.body.contains("一条回复里派六个子代理") && !p.body.contains("三个子代理"),
            "采集必须是六块并发派子代理，旧的「三个子代理」口径不许留：\n{}",
            p.body
        );
        assert!(
            !p.body.contains("| Wind |"),
            "Wind 已按用户 2026-09-21 的决定移出名单，不许留在采稿名单表格里：\n{}",
            p.body
        );
        assert!(
            !p.body.contains("1a-宏观-官方"),
            "官方源那一块已按用户定稿撤销，不许复活：\n{}",
            p.body
        );
        // 名单 = 用户 2026-09-20 三轮调整后的定稿（记忆里第三版，覆盖前两版）。
        // 允许 = 20 站；已删 = 用户明令不采，谁都不许加回（含被上一版名单偷偷加回的 17 个）。
        for site in [
            "巨潮",
            "中国证券报",
            "中国基金报",
            "券商中国",
            "每日经济新闻",
            "东方财富网",
            "财联社",
            "新华财经",
            "央视财经",
            "36氪",
            "投中网",
            "第一财经",
            "中国证监会",
            "上交所",
            "深交所",
            "北交所",
            "同花顺财经",
            "证券日报",
            "新浪财经",
        ] {
            assert!(
                p.body.contains(site),
                "用户定稿名单里的「{site}」不见了：\n{}",
                p.body
            );
        }
        for site in [
            // 产业栏（用户第一轮删）
            "我的钢铁网",
            "集微网",
            "机器之心",
            "界面新闻",
            "泰伯网",
            "财新网",
            "IT之家",
            "汽车之家",
            "中国房地产网",
            // 宏观栏（用户第二轮删，「只留新华财经和央视财经」）
            "华尔街见闻",
            "Bloomberg",
            "Reuters",
            "国家统计局",
            "中国人民银行",
            "财政部",
            "国家外汇管理局",
            "国家发展改革委",
            "海关总署",
            "经济参考报",
            "中国政府网",
            // 资本市场栏（用户第一、二轮删）
            "上海证券报",
            "雪球",
            "e公司",
            "证券时报",
            // 公告入口
            "同花顺公告",
        ] {
            let row = format!("| {site} |");
            assert!(
                !p.body.contains(&row),
                "「{site}」是用户明令删掉的站，不许再出现在采稿名单表格里：\n{}",
                p.body
            );
        }
        // 2026-09-21 批 5：抽取的否定回答必须确定性核验，否则「看漏了」会被当成「没有」。
        assert!(
            p.body.contains("「本页无窗口内条目」必须核一遍再交卷"),
            "契约必须要求对「本页无窗口内条目」做确定性核验：\n{}",
            p.body
        );
        // 2026-09-21 批 5 补：日期有三种写法（`2026-09-21` / `2026/09/21` / `20260921`），
        // 只数连字符写法会把中国证券报这类「日期在链接路径里」的站冤枉成 0 条。
        assert!(
            p.body.contains("取最大的那个")
                && p.body.contains("grep -c '2026/09/21'")
                && p.body.contains("grep -c '20260921'"),
            "核数必须三种写法都数、取最大：\n{}",
            p.body
        );
        assert!(
            p.body.contains("一个字都不许写正文")
                && p.body.contains("接栏、去重、统计全部交给 `bash`"),
            "主循环不许自己写正文、不许逐条 edit：\n{}",
            p.body
        );
        assert!(
            p.body.contains("宏观") && p.body.contains("产业动态") && p.body.contains("资本市场"),
            "输出必须是固定三栏"
        );
        // 2026-09-20 用户定死条目格式：三行（信源层级·来源·时间 / 概要 / 链接），
        // 对话里就是这份清单本身；失败站点不点名，拿不准的写在条目里。
        // 2026-09-21 批 5 实跑：来源字段被写成「央视网财经（来源：证券时报/券商中国）」
        // 「东方财富公告（逸豪新材 301176）」—— 来源只该是站点名。
        assert!(
            p.body.contains("来源只写站点的名字"),
            "契约必须禁止把公司名/转载方塞进来源：\n{}",
            p.body
        );
        assert!(
            p.body.contains("完整概要（主体·时间·核心事实·关键数据）")
                && p.body.contains("原文链接"),
            "条目要按定死的三行格式写，且条条有原文链接"
        );
        assert!(
            p.body.contains("逐字一致") && p.body.contains("不许压成"),
            "对话里给的是清单本身，不是摘要段"
        );
        assert!(
            !p.body.contains("【站点异常】") && !p.body.contains("【待核项】"),
            "这两节用户已删（2026-09-20）：失败站点只留文末一个数字，待核写在条目里"
        );
        assert!(
            !p.body.contains("单日不超过") && !p.body.contains("五栏"),
            "旧帽子的 30 条和五栏不许留：{}",
            p.id
        );
        // 2026-09-22 用户定稿：窗口就一条 —— **今天 00:00 → 我按下采集那一刻**。
        // 之前那版「上一期发布时刻 → 本期发布时刻（固定 06:00）」整块撤下：它让窗口依赖
        // 上一期目录，历史一清空就退化成「0 点到 6 点」这种谁都看不懂的窗。条目第一字段仍是
        // 站点属性的**信源层级**（官方／主流／其他），只决定要不要交叉验证。
        for must in [
            "今天 00:00",
            "按下采集",
            "信源层级",
            "官方",
            "主流",
            "其他",
            "不进重要性、不进排序",
            "两套体系",
            "永不许互相套用",
        ] {
            assert!(
                p.body.contains(must),
                "窗口／信源层级缺了「{must}」：\n{}",
                p.body
            );
        }
        // 反着钉：旧窗口口径（上一期 / 固定 06:00 / 兜底标记 / 卷间空档）一律不许复活。
        for dead in [
            "FALLBACK_TIME_WINDOW",
            "COVERAGE_GAP",
            "T_prev",
            "T_current",
            "06:00",
            "上一期",
            "空档",
        ] {
            assert!(
                !p.body.contains(dead),
                "旧窗口口径「{dead}」已撤，不许复活：\n{}",
                p.body
            );
        }
        // 没有定义的等级标记整块撤下：条目里不许再出现 `【A】/【B】/【C】`，
        // 也不许留着旧的排序口径。公告池的 `A／B／C／D` 是吕老师那边的事，不在此列。
        for dead in [
            "【A】",
            "【B】",
            "【C】",
            "A x／B y／C z",
            "A/B/C（每条必标）",
        ] {
            assert!(
                !p.body.contains(dead),
                "采集端旧等级标记「{dead}」已撤，不许复活：\n{}",
                p.body
            );
        }
        // 2026-09-22 用户裁决「加上」：公告池的 A／B／C／D 与采集端的【官方／主流／其他】
        // 靠得太近，两端各写一句硬话 —— 不许换算、不许互相套用、采集端不许出现 A／B／C／D。
        assert!(
            !p.body.contains("【A／B／C／D】"),
            "采集端条目里不许出现公告池的等级：\n{}",
            p.body
        );
    }

    /// 吕老师是**定调岗**，判据是《财富早知道·新闻筛选执行标准》的执行版
    /// （2026-09-22 换版）：**三道门**（财经属性门 / 三具体门 / 归栏）→ **相似复检**
    /// → **同日竞争八步** → **主稿 12—20 + 公告池**。掉一样，下游拿到的就不是能直接撰稿的框架。
    #[test]
    fn lv_lao_shi_decides_what_runs_and_why_it_does_not() {
        let p = get("lv-lao-shi").expect("主编吕老师必须存在");
        // 第一道门：财经属性门——政治／政务消息死在这一步（不是"不够重要"）。
        for must in [
            "财经属性门",
            "改变了哪个价格",
            "会去动仓位吗",
            "出访、会见、讲话",
        ] {
            assert!(
                p.body.contains(must),
                "财经属性门缺了「{must}」：\n{}",
                p.body
            );
        }
        // 第二 / 第三道门，以及归栏的四条边界规则。
        for must in [
            "具体主体",
            "具体数字",
            "具体后果",
            "三缺一即出局",
            "归不进三个栏目",
            "边界规则",
        ] {
            assert!(p.body.contains(must), "三道门缺了「{must}」：\n{}", p.body);
        }
        // 取舍与版面：相似复检、同日竞争（不是分数）、主稿规模、公告池、必停点。
        for must in [
            "相似复检",
            "同日竞争",
            "不是分数",
            "主稿 12—20",
            "公告池",
            "弃稿台账",
            "material_read",
            "不点头就停在这",
        ] {
            assert!(
                p.body.contains(must),
                "定调岗的定义缺了「{must}」：\n{}",
                p.body
            );
        }
        // 2026-09-22：**吕老师跟秒、跟节目时长没有任何关系。** 他只管「播哪几条、什么
        // 顺序、什么角度」——连「我不定秒数」这种否定句都不许写：提一句就是给模型一个
        // 念头，迟早会漏成一个 `seconds` 字段。时长与配速类的字一个都不留。
        // （新闻自身的日期是事实，不是配速——「时间锚点」允许。）
        for bad in ["秒", "配速", "时长", "分钟"] {
            assert!(
                !p.body.contains(bad),
                "吕老师契约不许出现时长／配速类内容（不该出现「{bad}」）：\n{}",
                p.body
            );
        }
        // C 批换版：采集端的标记从没有定义的 A／B／C 换成站点属性的**信源层级**，
        // 但它的作用只有一个——决定这条要不要再找一个来源，绝不许当重要性。
        for must in [
            "信源层级",
            "只说明",
            "不参与入选、不参与排序",
            "站点属性",
            "两套体系",
            "永不许互相套用",
        ] {
            assert!(
                p.body.contains(must),
                "信源层级口径缺了「{must}」：\n{}",
                p.body
            );
        }
        // 事件组（C 批）：同一条链只算一个名额，相似复检与同日竞争都以它为单位。
        for must in ["事件组", "只占一个主稿名额", "以事件组为单位"] {
            assert!(
                p.body.contains(must),
                "事件组合并缺了「{must}」：\n{}",
                p.body
            );
        }
    }

    #[test]
    fn dedicated_persona_owns_its_memories() {
        let x = get("sao-di-seng").expect("产业专家扫地僧必须存在");
        assert!(!x.builtin);
        assert_eq!(x.memory_owner(), Some("sao-di-seng"));
    }

    #[test]
    fn unknown_persona_is_none_and_never_panics() {
        assert!(get("nobody").is_none());
    }

    #[test]
    fn session_persona_maps_to_memory_owner_only_for_known_dedicated_personas() {
        assert_eq!(memory_owner_for(None, None), None, "没人物 → 全局");
        assert_eq!(
            memory_owner_for(Some("sao-di-seng"), None).as_deref(),
            Some("sao-di-seng")
        );
        assert_eq!(
            memory_owner_for(Some("li-xian"), None),
            None,
            "自带角色 → 全局"
        );
        assert_eq!(
            memory_owner_for(Some("xiao-xie"), None),
            None,
            "已下线的人物不许再持有记忆（它的记忆按 personas.md §5.2 回落全局）"
        );
        assert_eq!(
            memory_owner_for(Some("xiao-le"), None),
            None,
            "已下线的工位也不再持有记忆"
        );
        assert_eq!(
            memory_owner_for(Some("nobody"), None),
            None,
            "不认识的 id 不许凭空造归属"
        );
    }

    /// 组会话看得见两档：**本项目组 + 这一轮说话的人**（`docs/spec/projects.md` §4.2）。
    /// 手艺跟着人上桌；桌外的人、不认识的组，都不许凭空长出来。
    #[test]
    fn a_team_view_sees_the_team_and_the_one_holding_the_baton() {
        const TEAM: &str = "caifu-zaozhidao";
        // 说话人是桌上的人 → 组规 + 他的手艺（没交过棒时是第一棒采集）
        assert_eq!(
            memory_owners_for(None, Some(TEAM), Some("lv-lao-shi")),
            vec![TEAM.to_string(), "lv-lao-shi".to_string()],
            "组规 + 说话人的手艺，两条轴都要"
        );
        // 接了棒 → 换人
        assert_eq!(
            memory_owners_for(None, Some(TEAM), Some("xiao-xie")),
            vec![TEAM.to_string()],
            "桌外的人不算——不许凭空长出一个组外的人"
        );
        assert_eq!(
            memory_owners_for(None, Some(TEAM), Some("nobody")),
            vec![TEAM.to_string()]
        );
        assert_eq!(
            memory_owners_for(None, Some(TEAM), None),
            vec![TEAM.to_string()],
            "还没人接棒 → 只看组"
        );
        // 工位 / 无人物：与 `memory_owner_for` 逐条一致
        assert_eq!(
            memory_owners_for(Some("sao-di-seng"), None, None),
            vec!["sao-di-seng".to_string()]
        );
        assert!(
            memory_owners_for(Some("li-xian"), None, None).is_empty(),
            "自带 → 全局"
        );
        assert!(
            memory_owners_for(None, None, None).is_empty(),
            "自由对话 → 全局"
        );
        assert_eq!(
            memory_owners_for(None, Some("nobody"), Some("lv-lao-shi")),
            Vec::<String>::new(),
            "不认识的项目组不许凭空造归属"
        );
    }

    /// 项目组会话的记忆归**组**，不归任何个人——归错了就会以「全局」的身份
    /// 灌进每个人的工位会话（规格 §4.2「宁可窄，不要串味」）。
    #[test]
    fn a_team_session_owns_its_memories_as_a_team() {
        assert_eq!(
            memory_owner_for(None, Some("caifu-zaozhidao")).as_deref(),
            Some("caifu-zaozhidao")
        );
        assert_eq!(
            memory_owner_for(None, Some("nobody")).as_deref(),
            None,
            "不认识的项目组不许凭空造归属"
        );
        assert_eq!(
            memory_owner_for(None, Some("caifu-zaozhidao")).as_deref(),
            Some("caifu-zaozhidao"),
            "组会话的归属与「谁在说」无关：接口人是谁都算这个组的"
        );
        assert_eq!(
            memory_owner_for(Some("sao-di-seng"), Some("caifu-zaozhidao")).as_deref(),
            Some("caifu-zaozhidao"),
            "两条轴不该同时有值；万一有，以组为准（宁可窄，不要串味）"
        );
    }

    #[test]
    fn the_persona_block_keeps_the_hat_and_the_relationship_apart() {
        let x = get("sao-di-seng").unwrap();
        let b = block(x, &[get("yu-tian").unwrap()]);
        assert!(b.contains("产业专家扫地僧") && b.contains("热闹我不看，我看门道"));
        assert!(b.contains("人设红线"), "必须写明口吻可变、关系基调不可变");
        assert!(
            b.contains("编辑雨天 · 错在哪，我指给你"),
            "名册要按侧栏那行的样子写（名字 · 那句自述），用户按这个找得到人：\n{b}"
        );
        assert!(!b.contains("产业专家扫地僧 · "), "自己不进「其他工位」名单");
    }

    #[test]
    fn an_empty_roster_forbids_inventing_a_station_name() {
        let x = get("li-xian").unwrap();
        let b = block(x, &[]);
        assert!(b.contains("只有你这一个工位"), "空名册要说实话：\n{b}");
        assert!(
            b.contains("不许自己编一个工位名字"),
            "没名单却不禁编名字 = 「小王那个工位」还会回来：\n{b}"
        );
    }

    #[test]
    fn the_roster_lists_every_open_station_in_all_order() {
        let open = open_at(&prefs_path(), &BTreeSet::new());
        assert!(
            open.iter().all(|p| p.builtin),
            "没有授权码时只剩自带：{:?}",
            open.iter().map(|p| &p.id).collect::<Vec<_>>()
        );
        let me = get("li-xian").unwrap();
        let others = open
            .iter()
            .copied()
            .filter(|p| p.id != me.id)
            .collect::<Vec<_>>();
        let b = block(me, &others);
        for p in others {
            assert!(
                b.contains(&format!("- {} · {}", p.name, p.role)),
                "缺 {}：\n{b}",
                p.id
            );
        }
    }

    #[test]
    fn selected_ids_drop_unknown_unlicensed_and_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personas.json");
        let licensed: BTreeSet<String> = ["sao-di-seng".to_string()].into_iter().collect();
        std::fs::write(&path, r#"{"enabled":["sao-di-seng","ghost","li-xian"]}"#).unwrap();
        let selected = selected_ids_at(&path, &licensed);
        assert_eq!(
            selected.into_iter().collect::<Vec<_>>(),
            vec!["sao-di-seng".to_string()]
        );
    }

    #[test]
    fn frontmatter_with_a_misspelled_key_is_rejected() {
        let raw = "id: xiao-le\nname: 搭子小乐\nrole: 什么都接\nvoice: 直\nskills: []\n\
                   kind: fallback\nbuitin: true\nempty_hint: 说\nversion: 0.1.0\n";
        assert!(
            serde_yaml::from_str::<Persona>(raw).is_err(),
            "错键 `buitin` 必须报错——静默落回 builtin=false 会让自带角色的记忆变私有"
        );
    }

    #[test]
    fn a_dashes_line_inside_a_value_does_not_close_the_frontmatter() {
        let raw = "---\nid: x\nname: 小乐\nrole: >\n  真\n  ---\n  假\nvoice: 直\n\
                   skills: []\nkind: fallback\nbuiltin: true\nempty_hint: 说\nversion: 0.1.0\n\
                   ---\n\n## 我干什么\n- 有\n";
        let p = parse("personas/x.md", raw);
        assert!(
            p.role.contains("---"),
            "折进 role 的 `---` 不能被当成闭合：{}",
            p.role
        );
        assert_eq!(p.body, "## 我干什么\n- 有", "闭合行之后到文件末尾都是正文");
    }

    #[test]
    fn file_stem_must_equal_the_id() {
        let raw = "---\nid: xiao-xie\nname: 林碳小谢\nrole: 林碳的专家和记者\nvoice: 带数据\nskills: []\n\
                   kind: dedicated\nbuiltin: false\nempty_hint: 说\nversion: 0.1.0\n---\n\n## 我干什么\n- 有\n";
        let outcome = std::panic::catch_unwind(|| parse("personas/someone-else.md", raw));
        assert!(outcome.is_err(), "文件名 stem 与 id 不一致必须 panic");
    }

    #[test]
    fn every_definition_file_is_registered_in_sources() {
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let personas_dir = src_dir.join("personas");

        let mut on_disk: Vec<String> = std::fs::read_dir(&personas_dir)
            .expect("src/personas/ 目录必须存在")
            .map(|e| {
                e.expect("读取 personas/ 目录项")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|n| n.ends_with(".md"))
            .collect();
        on_disk.sort();

        let mut registered: Vec<String> = SOURCES
            .iter()
            .map(|(path, _)| {
                src_dir
                    .join(path)
                    .file_name()
                    .expect("SOURCES 里的路径必须带文件名")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        registered.sort();

        assert_eq!(
            on_disk, registered,
            "src/personas/ 下的每个 .md 都必须登记在 SOURCES 里"
        );
        for (path, _) in SOURCES {
            assert!(
                src_dir.join(path).is_file(),
                "SOURCES 登记的 {path} 在磁盘上不存在"
            );
        }
    }

    /// 「指路必须指向真实存在的工位」（规格 §6）。上一版把这条留给了人记——
    /// 结果三个人下线之后，五个定义里还在指名道姓地把用户送去空地。
    /// 这条测试把它变成机器守：**活的工位不许提到已下线的人**（全名或 `aka` 里的称呼）。
    #[test]
    fn no_live_station_points_at_a_retired_one() {
        let retired: Vec<Persona> = SOURCES
            .iter()
            .map(|&(path, raw)| parse(path, raw))
            .filter(|p| p.retired)
            .collect();
        assert!(!retired.is_empty(), "这条测试靠下线名单才有意义");
        for gone in &retired {
            assert!(
                !gone.aka.is_empty(),
                "{} 下线时要自报「别人怎么称呼它」，否则这条测试守的是全名，而指路写的全是简称",
                gone.name
            );
        }

        for (path, raw) in SOURCES {
            let live = parse(path, raw);
            if live.retired {
                continue;
            }
            for gone in &retired {
                for call in std::iter::once(&gone.name).chain(gone.aka.iter()) {
                    assert!(
                        !live.body.contains(call.as_str()),
                        "{path} 的正文还在把用户指给已下线的人：{call}。改为指向在册工位（成稿找小宋，产业判断找扫地僧）"
                    );
                }
            }
        }
    }
}
