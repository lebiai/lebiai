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
const SOURCES: [(&str, &str); 9] = [
    // 自带（谁都有，不进授权码）——顺序即侧栏「工位」里自带那一组的顺序。
    ("personas/xiao-le.md", include_str!("personas/xiao-le.md")),
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
    ("personas/xiao-xie.md", include_str!("personas/xiao-xie.md")),
    ("personas/xiao-jin.md", include_str!("personas/xiao-jin.md")),
    ("personas/yu-tian.md", include_str!("personas/yu-tian.md")),
    ("personas/xiao-yu.md", include_str!("personas/xiao-yu.md")),
];

/// `hermes-store` 有 `frontmatter::parse_doc_str`，但 `hermes-store` 依赖
/// `hermes-core`，反向引用会成环——这里保留一份精简实现，按**行边界**切分：
/// 只有整行等于 `---` 才算闭合，值内部（如折叠标量里的 `---`）不会被当成边界。
/// Ruling 1.1-b，见 `docs/records/20260914-personas.md` §2.0.1。
fn parse(path: &str, raw: &str) -> Persona {
    let mut lines = raw.lines();
    assert!(
        lines.next().map(str::trim_end) == Some("---"),
        "{path}: 人物定义必须以整行 `---` 开头"
    );
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    assert!(closed, "{path}: frontmatter 缺闭合的整行 `---`");
    let body = lines.collect::<Vec<_>>().join("\n");

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

/// 会话的人物 id → 记忆归属。会话没人物、人物 id 不认识、或人物是自带角色 → `None`。
/// 「会话 → 归属」这条转换只许有这一处，别在每个调用点各写一遍 `get(...).and_then(...)`。
pub fn memory_owner_for(session_persona: Option<&str>) -> Option<String> {
    let persona = get(session_persona?)?;
    persona.memory_owner().map(str::to_string)
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
        assert!(get("xiao-le").is_none(), "下线的工位不许再出现在名册里");
        assert!(
            !ids().contains(&"xiao-le"),
            "ids() 是授权码与侧栏的真源，下线的 id 不能留在里面"
        );
        let raw = SOURCES
            .iter()
            .find(|(path, _)| *path == "personas/xiao-le.md")
            .expect("定义文件仍在 SOURCES 里登记，否则回不来")
            .1;
        assert!(
            raw.contains("retired: true"),
            "暂时下线靠 `retired: true`；文件不在 = 恢复要翻历史，不算「暂时」"
        );
    }

    #[test]
    fn the_roster_has_the_five_licensed_roles_plus_the_clerk() {
        for (id, name) in [
            ("wang-hai-yan", "情报王海燕"),
            ("xiao-xie", "林碳小谢"),
            ("xiao-jin", "具身智能小金"),
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

    #[test]
    fn dedicated_persona_owns_its_memories() {
        let x = get("xiao-xie").expect("林碳小谢必须存在");
        assert!(!x.builtin);
        assert_eq!(x.memory_owner(), Some("xiao-xie"));
    }

    #[test]
    fn unknown_persona_is_none_and_never_panics() {
        assert!(get("nobody").is_none());
    }

    #[test]
    fn session_persona_maps_to_memory_owner_only_for_known_dedicated_personas() {
        assert_eq!(memory_owner_for(None), None, "没人物 → 全局");
        assert_eq!(
            memory_owner_for(Some("xiao-xie")).as_deref(),
            Some("xiao-xie")
        );
        assert_eq!(memory_owner_for(Some("li-xian")), None, "自带角色 → 全局");
        assert_eq!(
            memory_owner_for(Some("xiao-le")),
            None,
            "已下线的工位也不再持有记忆"
        );
        assert_eq!(
            memory_owner_for(Some("nobody")),
            None,
            "不认识的 id 不许凭空造归属"
        );
    }

    #[test]
    fn the_persona_block_keeps_the_hat_and_the_relationship_apart() {
        let x = get("xiao-xie").unwrap();
        let b = block(x, &[get("yu-tian").unwrap()]);
        assert!(b.contains("林碳小谢") && b.contains("没依据的话，我不说"));
        assert!(b.contains("人设红线"), "必须写明口吻可变、关系基调不可变");
        assert!(
            b.contains("编辑雨天 · 错在哪，我指给你"),
            "名册要按侧栏那行的样子写（名字 · 那句自述），用户按这个找得到人：\n{b}"
        );
        assert!(!b.contains("林碳小谢 · "), "自己不进「其他工位」名单");
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
        let licensed: BTreeSet<String> = ["xiao-xie".to_string()].into_iter().collect();
        std::fs::write(&path, r#"{"enabled":["xiao-xie","ghost","li-xian"]}"#).unwrap();
        let selected = selected_ids_at(&path, &licensed);
        assert_eq!(
            selected.into_iter().collect::<Vec<_>>(),
            vec!["xiao-xie".to_string()]
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
}
