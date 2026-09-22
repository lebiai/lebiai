//! Team (项目组) definitions — compiled in, product-owned, never user data.
//!
//! 项目组 = **一个会话、几个固定的人、一件周而复始的活**（`docs/spec/projects.md`）。
//! 与人物定义同一个做法：定义编译进二进制、不进数据根、文件名 stem = `id`。
//! 名册的**唯一真源**就是下面这个数组——侧栏、授权、提示词组块都从它派生。

use std::path::Path;
use std::sync::OnceLock;

use crate::frontmatter;
use crate::persona::{self, Persona};

/// 一个成员在这张桌子上的那一摊活（与 `personas.md` 的分工表同义）。
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamMember {
    pub id: String,
    pub duty: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: String,
    pub name: String,
    /// 侧栏那一行的小字：这是个什么活。
    pub role: String,
    /// 接口人：选题门（报题给你点头）。必须是成员之一。
    /// 没交过棒时第一棒是名册第一个人，不是他。
    pub interface: String,
    pub members: Vec<TeamMember>,
    pub version: String,
    #[serde(skip)]
    pub body: String,
}

impl Team {
    pub fn member(&self, id: &str) -> Option<&TeamMember> {
        self.members.iter().find(|m| m.id == id)
    }

    /// 接口人的人物定义。`parse` 已经断言他是在册人物，所以这里取得到。
    pub fn interface_persona(&self) -> &'static Persona {
        persona::get(&self.interface).unwrap_or_else(|| {
            panic!(
                "{}: 接口人 {} 不在名册里（parse 该拦下的）",
                self.id, self.interface
            )
        })
    }
}

const SOURCES: [(&str, &str); 1] = [(
    "teams/caifu-zaozhidao.md",
    include_str!("teams/caifu-zaozhidao.md"),
)];

fn parse(path: &str, raw: &str) -> Team {
    let (yaml, body) = frontmatter::split(path, raw);
    let mut t: Team =
        serde_yaml::from_str(&yaml).unwrap_or_else(|e| panic!("{path}: frontmatter 解析失败：{e}"));

    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| panic!("{path}: 取不到文件名 stem"));
    assert_eq!(
        t.id,
        stem,
        "{path}: 文件名 stem（{stem}）必须等于 id（{id}）",
        id = t.id
    );
    assert!(
        !t.members.is_empty(),
        "{path}: 一个成员都没有的项目组不算项目组"
    );
    let mut seen = Vec::new();
    for m in &t.members {
        assert!(
            !seen.contains(&m.id.as_str()),
            "{path}: 成员 {id} 写了两次——名册要按侧栏那行数人",
            id = m.id
        );
        seen.push(m.id.as_str());
        assert!(
            persona::get(&m.id).is_some(),
            "{path}: 成员 {id} 不是本版本在册的人物（下线或拼错都不许留在桌上）",
            id = m.id
        );
    }
    assert!(
        t.member(&t.interface).is_some(),
        "{path}: 接口人 {} 不在成员里——没人被点名时就没人接了",
        t.interface
    );
    t.body = body.trim().to_string();
    assert!(!t.body.is_empty(), "{path}: 正文是空的");
    t
}

/// 别的 crate 的测试要造项目组样本时走这里——**复用同一个 `parse`**。
#[doc(hidden)]
pub fn parse_for_test(path: &str, raw: &str) -> Team {
    parse(path, raw)
}

pub fn all() -> &'static [Team] {
    static ALL: OnceLock<Vec<Team>> = OnceLock::new();
    ALL.get_or_init(|| SOURCES.iter().map(|(path, raw)| parse(path, raw)).collect())
}

pub fn get(id: &str) -> Option<&'static Team> {
    all().iter().find(|t| t.id == id)
}

pub fn ids() -> Vec<&'static str> {
    all().iter().map(|t| t.id.as_str()).collect()
}

/// 组块：这张桌子叫什么、一期怎么走、**今天桌上坐着谁**（含缺席）、你自己的那一行。
///
/// `present` = 这台机器上**开着**的成员 id（授权 + 勾选，见 `personas.md` §4）；
/// `speaker` = 这一轮由谁接（接棒的；没交过棒 → **第一棒采集**，见 `pipeline::fallback_holder`）。
///
/// 调用方负责把它放在**人物块之后、记忆之前**——顺序即断言
/// （`the_team_block_sits_after_the_persona_block_and_before_memory`）。
pub fn block(team: &Team, present: &[&str], speaker: &str) -> String {
    let mut out = format!(
        "## 你在哪张桌子上\n\
         你在 **{name}** 这张桌子上——{role}。\n\n\
         {body}\n\n\
         **今天在这张桌子上的人**（名字 · 他那一摊）：\n",
        name = team.name,
        role = team.role,
        body = team.body.trim(),
    );
    let mut absent = Vec::new();
    for m in &team.members {
        let Some(p) = persona::get(&m.id) else {
            continue;
        };
        if present.contains(&m.id.as_str()) {
            let mark = if m.id == speaker {
                "  ← 你，这一轮由你接"
            } else {
                ""
            };
            out.push_str(&format!("- {} · {}{}\n", p.name, m.duty, mark));
        } else {
            absent.push((p, m));
        }
    }
    if !absent.is_empty() {
        out.push_str("\n**今天不在的人**（这台机器上没有他们的授权）：\n");
        for (p, m) in &absent {
            out.push_str(&format!("- {} · {}（缺席）\n", p.name, m.duty));
        }
    }
    out.push_str(
        "\n## 这张桌子的规矩\n\
         - 你这一轮只做你那一摊。别人的活，说清该谁做——\
         **不许替他做、不许替他下结论**，这桌上每个人都守自己的边界。\n\
         - 干活要一棒接一棒：每一步的产出物就是交给下一棒的棒子，别跳步。\
         **产出物落盘了，下一棒会接**——你交不出去，也别假装交出去了。\n\
         - 出了**决定**（选题怎么排），要落成一份带头的选题单\
         （`decision` 工具），别只在对话里说一句——用户得能在那上面点头或让你改。\
         选题没点头，下一棒不接。这张桌子上没有审稿这一棒，不要等编辑接。\n",
    );
    if !absent.is_empty() {
        out.push_str(
            "- 有缺席时，缺的那一步就停在那儿：说清缺谁、从哪得到（授权码），**不许绕过去**。\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_definition_parses_and_ids_are_unique() {
        let all = all();
        assert!(!all.is_empty(), "至少要有《财富早知道》");
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "项目组 id 必须唯一");
        for t in all {
            assert!(
                !t.name.is_empty() && !t.role.is_empty(),
                "{} 缺 name/role",
                t.id
            );
            assert!(!t.body.trim().is_empty(), "{} 正文是空的", t.id);
        }
    }

    #[test]
    fn every_definition_file_is_registered_in_sources() {
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let teams_dir = src_dir.join("teams");

        let mut on_disk: Vec<String> = std::fs::read_dir(&teams_dir)
            .expect("src/teams/ 目录必须存在")
            .map(|e| {
                e.expect("读取 teams/ 目录项")
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
            "src/teams/ 下的每个 .md 都必须登记在 SOURCES 里"
        );
        for (path, _) in SOURCES {
            assert!(
                src_dir.join(path).is_file(),
                "SOURCES 登记的 {path} 在磁盘上不存在"
            );
        }
    }

    #[test]
    fn file_stem_must_equal_the_id() {
        assert_eq!(get("caifu-zaozhidao").unwrap().id, "caifu-zaozhidao");
        assert!(get("nobody").is_none());
    }

    /// 桌上的人必须**在册**：把一个人下线（`retired: true`）之后忘了改这里，
    /// 组里就会出现一个谁都找不到的岗位——这条测试把它挡在编译期数据这一层。
    #[test]
    fn every_member_is_a_live_persona() {
        for t in all() {
            for m in &t.members {
                assert!(
                    persona::get(&m.id).is_some(),
                    "{}: 成员 {} 不在名册里",
                    t.id,
                    m.id
                );
                assert!(!m.duty.trim().is_empty(), "{}: {} 没写那一摊活", t.id, m.id);
            }
        }
    }

    #[test]
    fn the_block_lists_the_table_and_marks_the_absent() {
        let t = get("caifu-zaozhidao").unwrap();
        let all_ids: Vec<&str> = t.members.iter().map(|m| m.id.as_str()).collect();
        let present: Vec<&str> = all_ids
            .iter()
            .copied()
            .filter(|id| *id != "xiao-song")
            .collect();

        let b = block(t, &present, "lv-lao-shi");
        assert!(b.contains("财富早知道") && b.contains("一天一期，盘前"));
        assert!(
            b.contains("情报王海燕 · 获取信息：名单内全量采、窗口内采全、信源层级、三栏交卷"),
            "{b}"
        );
        assert!(
            b.contains("← 你，这一轮由你接"),
            "本轮接棒的人要标出来：\n{b}"
        );
        assert!(
            b.contains("今天不在的人") && b.contains("记者小宋") && b.contains("（缺席）"),
            "缺席的人要写在纸面上，不许假装他在：\n{b}"
        );
        assert!(b.contains("不许绕过去"), "缺人要停在那儿，不许跳过：\n{b}");
        assert!(
            b.contains("不许替他做"),
            "边界必须写死，否则组里硬拦就松了：\n{b}"
        );
        assert!(
            b.contains("产出物落盘了") && b.contains("别假装交出去了"),
            "棒跟产物走——模型不许自己宣称交了棒：\n{b}"
        );
        assert!(
            b.contains("decision") && b.contains("点头") && b.contains("选题单"),
            "决定要落成能点头的选题单，不许只在对话里说一句：\n{b}"
        );
        assert!(
            !b.contains("编辑雨天")
                && !b.contains("产业专家扫地僧")
                && !b.contains("资料员小文")
                && !b.contains("主播小雨"),
            "拿下桌的人不许再出现在组块名册里：\n{b}"
        );
        assert!(
            b.contains("没有审稿这一棒") && !b.contains("放行") && !b.contains("打回"),
            "审稿不在这三步里，组块不许再把放行/打回当成栏目的决定：\n{b}"
        );
    }

    #[test]
    fn the_block_omits_absent_section_when_everyone_is_there() {
        let t = get("caifu-zaozhidao").unwrap();
        let present: Vec<&str> = t.members.iter().map(|m| m.id.as_str()).collect();
        let b = block(t, &present, &t.interface);
        assert!(!b.contains("今天不在的人"), "{b}");
        assert!(!b.contains("缺席"), "{b}");
    }

    /// 这张桌子对上栏目的三步：采、选、写。多出来的人是工位，不坐这张桌。
    #[test]
    fn caifu_zaozhidao_is_the_three_step_table() {
        let t = get("caifu-zaozhidao").unwrap();
        let ids: Vec<&str> = t.members.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["wang-hai-yan", "lv-lao-shi", "xiao-song"],
            "桌上只留王海燕 / 吕老师 / 小宋——口播那一棒已取消，小雨不坐这张桌"
        );
        assert_eq!(t.interface, "lv-lao-shi");
        for gone in ["sao-di-seng", "yu-tian", "xiao-yu", "xiao-wen"] {
            assert!(t.member(gone).is_none(), "{gone} 仍是工位，但不在这张桌上");
            assert!(
                persona::get(gone).is_some(),
                "{gone} 被拿下桌不等于下线工位"
            );
        }
    }

    #[test]
    fn the_interface_is_a_member_and_is_the_person_others_fall_back_to() {
        for t in all() {
            assert!(
                t.member(&t.interface).is_some(),
                "{}: 接口人不在成员里",
                t.id
            );
            assert_eq!(t.interface_persona().id, t.interface);
        }
        assert_eq!(
            get("caifu-zaozhidao").unwrap().interface_persona().name,
            "主编吕老师"
        );
    }
}
