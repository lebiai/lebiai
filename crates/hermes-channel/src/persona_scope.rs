//! 人物（工位）的**面**：看得见谁、看得见哪些技能。
//!
//! 两件事放在一起，是因为它们是同一个问题的两半——「这个工位能碰什么」：
//! ① 名册：越界时能指向哪些**真实存在**的工位；
//! ② 技能面：系统提示里广告哪些技能（规格 `docs/spec/personas.md` §2.3 / §6.1②）。
//!
//! 规则只写在这里一份。CLI/IM（[`crate::context`]）与 GUI（[`crate::companion_context`]）
//! 各自那份提示词都必须从这里取——两份判据迟早会漂，而漂的那天没人会发现。

use hermes_core::persona::Persona;
use hermes_skills::LoadedSkill;

/// 名册：本机开着的工位，去掉「我」自己。
///
/// 自己也在 [`hermes_core::persona::open`] 里（侧栏会列出来），但指路指的是**别人**，
/// 把「找我自己」写进名单只会让模型说「这活归我」。
pub fn others<'a>(me: &Persona, roster: &'a [&'a Persona]) -> Vec<&'a Persona> {
    roster.iter().copied().filter(|p| p.id != me.id).collect()
}

/// 这个工位能在提示词里看见哪些技能。
///
/// - 人物**没写** `skills` → 不收窄：逐字节等于这一层没做之前的行为。
/// - 人物**写了**（可以是空名单）→ 只留名单里的，外加**内置元技能**。
///
/// 元技能指 `memory-palace` / `skill-creator` / `find-skills`：它们是搭子的底层纪律
/// （记忆怎么写、技能怎么造、去哪找），不属于任何专业领域，剥掉会让人物会话失去
/// 记忆纪律（规格 §6.1②「收窄的例外」）。判据是 `always_active` **或**在
/// [`hermes_skills::BUNDLED_SKILLS`] 名单里——两个条件都要，`skill-creator` 与
/// `find-skills` 并没有标 `always_active`。
///
/// 收窄的是**广告**，不是能力：没被广告的技能照样躺在磁盘上，用户装它、删它都不受影响。
pub fn visible_skills<'a>(
    persona: Option<&Persona>,
    all: &'a [LoadedSkill],
) -> Vec<&'a LoadedSkill> {
    let Some(allow) = persona.and_then(|p| p.skills.as_ref()) else {
        return all.iter().collect();
    };
    all.iter()
        .filter(|s| {
            let name = s.frontmatter.name.as_str();
            s.frontmatter.always_active
                || hermes_skills::BUNDLED_SKILLS.contains(&name)
                || allow.iter().any(|a| a == name)
        })
        .collect()
}

/// 技能索引段开头那句。索引里混着人名（`xiao-wang` 这类是用户自己起的名），
/// 不点明「这是技能不是工位」，模型会拿它当工位指给用户（2026-09-15 实测）。
pub const SKILLS_ARE_NOT_STATIONS: &str =
    "The entries below are **skills (capabilities)**, not people/workstations — \
     never offer one of these names as a place to take a task. Workstations are listed in the \
     persona block above.";

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_skills::SkillFrontmatter;

    fn skill(name: &str, always_active: bool) -> LoadedSkill {
        LoadedSkill {
            frontmatter: SkillFrontmatter {
                name: name.into(),
                description: format!("{name} desc"),
                triggers: vec![],
                version: None,
                license: None,
                always_active,
                extra: Default::default(),
            },
            body: String::new(),
            source: std::path::PathBuf::from(format!("{name}/SKILL.md")),
            scope: hermes_skills::Scope::User,
        }
    }

    fn persona_with(skills: Option<Vec<&str>>) -> Persona {
        let raw = format!(
            "---\nid: t\nname: 测试\nrole: 一句话\nvoice: 直\nkind: dedicated\n\
             builtin: false\nempty_hint: 说\nversion: 0.1.0\n{}\n---\n\n## 我干什么\n- 有\n",
            match skills {
                None => String::new(),
                Some(v) => format!(
                    "skills: [{}]",
                    v.iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        );
        hermes_core::persona::parse_for_test("personas/t.md", &raw)
    }

    fn names<'a>(v: &[&'a LoadedSkill]) -> Vec<&'a str> {
        v.iter().map(|s| s.frontmatter.name.as_str()).collect()
    }

    #[test]
    fn an_undeclared_persona_does_not_narrow_anything() {
        let all = vec![skill("memory-palace", true), skill("xiao-wang", false)];
        let p = persona_with(None);
        assert_eq!(
            names(&visible_skills(Some(&p), &all)),
            ["memory-palace", "xiao-wang"]
        );
    }

    #[test]
    fn a_declared_persona_only_advertises_its_own_plus_meta() {
        let all = vec![
            skill("memory-palace", true),
            skill("skill-creator", false),
            skill("xiao-wang", false),
            skill("wechat-article", false),
        ];
        let p = persona_with(Some(vec!["wechat-article"]));
        assert_eq!(
            names(&visible_skills(Some(&p), &all)),
            ["memory-palace", "skill-creator", "wechat-article"],
            "元技能永远留着，别人的技能一个都不留"
        );
    }

    #[test]
    fn an_empty_roster_keeps_only_the_meta_skills() {
        let all = vec![skill("memory-palace", true), skill("xiao-wang", false)];
        let p = persona_with(Some(vec![]));
        assert_eq!(names(&visible_skills(Some(&p), &all)), ["memory-palace"]);
    }

    #[test]
    fn no_persona_means_no_narrowing() {
        let all = vec![skill("xiao-wang", false)];
        assert_eq!(names(&visible_skills(None, &all)), ["xiao-wang"]);
    }
}
