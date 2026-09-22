//! `hermes personas` — 这个版本内置的人物（工位）定义；
//! 以及把 `--persona <ID>` 的字符串解析成人物定义。
//!
//! 人物定义本身住在 `hermes_core::persona`（编译进去的，不是用户数据）。

use anyhow::Result;

/// `--persona` 的值 → 人物定义。
///
/// `None` = 没指定人物：行为与没有这一层时一致（自然退化）。
///
/// 未知 id 是**错误**，且必须在使用者还没进会话时就失败——报错里带上可用 id，
/// 否则用户只能对着一个空会话猜自己拼错了哪个字。
pub fn resolve_persona(
    id: Option<&str>,
) -> std::result::Result<Option<&'static hermes_core::persona::Persona>, String> {
    let Some(id) = id else { return Ok(None) };
    hermes_core::persona::get(id).map(Some).ok_or_else(|| {
        format!(
            "unknown persona `{id}`。可用：{}",
            hermes_core::persona::ids().join(", ")
        )
    })
}

/// `hermes personas` 的正文行，一行一个人物（自带角色在前，与 `persona::all()` 同序）。
///
/// id 列是 ASCII，定宽对齐；名字 / 角色是中文，跟着排就行——按字符数补空格会把中文列
/// 排成锯齿，所以不补。
pub fn list_lines() -> Vec<String> {
    hermes_core::persona::all()
        .iter()
        .map(|p| {
            let tag = if p.builtin { "自带" } else { "授权" };
            format!("{:<9} {}（{}） · {}", p.id, p.name, tag, p.role)
        })
        .collect()
}

pub fn run() -> Result<()> {
    for line in list_lines() {
        println!("{line}");
    }
    println!();
    println!("用法：hermes chat --persona <ID>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_persona_is_refused_with_the_valid_list() {
        let err = resolve_persona(Some("nobody")).unwrap_err();
        assert!(err.contains("nobody"));
        assert!(err.contains("sao-di-seng"), "报错要给出可用 id：{err}");
    }

    #[test]
    fn no_persona_means_the_todays_behaviour() {
        assert!(resolve_persona(None).unwrap().is_none());
        assert_eq!(
            resolve_persona(Some("sao-di-seng")).unwrap().unwrap().id,
            "sao-di-seng"
        );
    }

    #[test]
    fn the_list_shows_who_is_builtin() {
        let lines = list_lines();
        assert!(lines
            .iter()
            .any(|l| l.contains("li-xian") && l.contains("自带")));
        assert!(lines.iter().any(|l| l.contains("sao-di-seng")));
    }
}
