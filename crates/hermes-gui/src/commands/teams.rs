//! 项目组：侧栏那一节，以及组会话要用的名册。
//!
//! 名册的真源是 `hermes_core::team`（编译期定义）；「谁在册」的真源是
//! `personas::open_ids()`（授权 ∩ 勾选 + 自带）。**这里不许再判一遍**——
//! 侧栏显示谁、组里坐着谁、指路名册里有谁，必须是同一份答案。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use hermes_core::team;

use crate::error::GuiError;
use crate::state::AppState;

/// 桌上一行。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberItem {
    pub id: String,
    pub name: String,
    /// 他在这张桌子上的那一摊活。
    pub duty: String,
    /// 这台机器上开着（授权 ∩ 勾选 + 自带）。`false` = **缺席**：灰掉、不可点、
    /// 不可 @，并写清「需要「X」」——不许假装他在，也不许悄悄藏掉（规格 §7.1）。
    pub present: bool,
}

/// 侧栏里的一行项目组。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamItem {
    pub id: String,
    pub name: String,
    pub role: String,
    pub members: Vec<TeamMemberItem>,
    /// 缺了几个人（0 = 到齐）。
    pub missing: usize,
    /// **接口人在册开着**才跑得起来：没人接的桌子开不出一条会话。
    pub can_run: bool,
    /// 组会话里这一轮由谁开口（接棒的；没交过棒 → 第一棒采集）。界面据此标说话人。
    pub speaker_id: String,
    /// 侧栏那一行小字。缺人时优先说缺谁（那是跑不起来的原因），
    /// 人到齐了才说这活跑到哪了。
    pub hint: String,
}

/// 今天这活跑到哪了，交给 [`crate::commands::episode`] 算——**侧栏与点头卡
/// 必须读同一份**，否则会出现「侧栏说还没开工、进去却有一张待点头的卡」。
pub(crate) fn day_of(state: &AppState, team_id: &str) -> crate::commands::episode::TeamDay {
    let day = chrono::Local::now().date_naive().to_string();
    crate::commands::episode::team_day(Path::new(&state.workspace_root()), team_id, &day)
}

/// 让一行小字同时说清「缺谁」和「从哪得到」（规格 §7.1）。
fn missing_hint(names: &[String]) -> String {
    format!(
        "缺 {} 人 · 需要{}",
        names.len(),
        names
            .iter()
            .map(|n| format!("「{n}」"))
            .collect::<Vec<_>>()
            .join("")
    )
}

/// 纯函数：名册 + 「谁在册」+ 「今天这期动过没」→ 侧栏条目。测试从这里进来。
pub(crate) fn items(
    open: &[&str],
    day_of: &dyn Fn(&str) -> crate::commands::episode::TeamDay,
) -> Vec<TeamItem> {
    team::all()
        .iter()
        .map(|t| {
            let d = day_of(&t.id);
            let members: Vec<TeamMemberItem> = t
                .members
                .iter()
                .map(|m| TeamMemberItem {
                    id: m.id.clone(),
                    name: hermes_core::persona::get(&m.id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| m.id.clone()),
                    duty: m.duty.clone(),
                    present: open.contains(&m.id.as_str()),
                })
                .collect();
            let absent: Vec<String> = members
                .iter()
                .filter(|m| !m.present)
                .map(|m| m.name.clone())
                .collect();
            let can_run = open.contains(&t.interface.as_str());
            // 「谁在说」与「这一棒在谁手上」是同一个答案（`persona::speaker_for` 的判据）：
            // 接棒的是桌上的人 → 他开口；没交过 / 交了个外人 → 第一棒。
            let speaker_id = if can_run {
                hermes_core::persona::speaker_for(None, Some(&t.id), Some(&d.holder_id))
                    .map(|p| p.id.clone())
                    .unwrap_or_else(|| hermes_core::start_id(t).to_string())
            } else {
                t.interface.clone()
            };
            let hint = if !can_run {
                format!("缺接口人 · 需要「{}」", t.interface_persona().name)
            } else if !absent.is_empty() {
                missing_hint(&absent)
            } else {
                // 一行小字要回答「这活跑到哪了」——按**真源**说，不编：
                // 待你点头（唯一必停的一步）> 交在谁手上 > 已经动过 > 还没开工。
                if d.pending() {
                    "待你点头".to_string()
                } else if let Some(last) = d.handoffs.last() {
                    format!("在{}手上", last.to_name)
                } else if !d.items.is_empty() {
                    "进行中".to_string()
                } else {
                    "还没开工".to_string()
                }
            };
            TeamItem {
                id: t.id.clone(),
                name: t.name.clone(),
                role: t.role.clone(),
                members,
                missing: absent.len(),
                can_run,
                speaker_id,
                hint,
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_teams(state: State<'_, AppState>) -> Result<Vec<TeamItem>, GuiError> {
    let open = crate::commands::personas::open_ids();
    Ok(items(&open, &|team_id| day_of(&state, team_id)))
}

/// 项目组必须存在，且**接口人开着**——没人接的桌子开不出一条会话。
/// 口径只此一处：`new_session` 与将来的入口都问它。
pub fn require_team_ready(id: &str) -> Result<&'static team::Team, GuiError> {
    let t = team::get(id).ok_or_else(|| GuiError::NotFound(format!("team {id}")))?;
    if !crate::commands::personas::open_ids().contains(&t.interface.as_str()) {
        return Err(GuiError::NotFound(format!(
            "team {id} 的接口人 {} 不在册",
            t.interface
        )));
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_open() -> Vec<&'static str> {
        team::get("caifu-zaozhidao")
            .unwrap()
            .members
            .iter()
            .map(|m| m.id.as_str())
            .collect()
    }

    fn empty_day(_: &str) -> crate::commands::episode::TeamDay {
        crate::commands::episode::TeamDay {
            items: Vec::new(),
            handoffs: Vec::new(),
            holder_id: "wang-hai-yan".into(),
            holder_name: "情报王海燕".into(),
        }
    }

    /// 今天出过一件东西（用来验「进行中」那一档小字）。
    fn busy_day() -> crate::commands::episode::TeamDay {
        let mut d = empty_day("");
        d.items.push(crate::commands::episode::EpisodeItem {
            rel_path: "outputs/2026-09-17/选题单.md".into(),
            name: "选题单".into(),
            ext: "md".into(),
            modified: "2026-09-17T06:03:00+08:00".into(),
            decision: None,
        });
        d
    }

    #[test]
    fn a_full_table_says_where_the_episode_stands() {
        let open = all_open();
        let rows = items(&open, &empty_day);
        assert_eq!(rows.len(), team::all().len());
        let t = &rows[0];
        assert_eq!(t.name, "财富早知道");
        assert_eq!(t.missing, 0);
        assert!(t.can_run);
        assert_eq!(t.speaker_id, "wang-hai-yan", "组里没交过棒：第一棒采");
        assert_eq!(t.hint, "还没开工");
        assert!(t.members.iter().all(|m| m.present));

        let busy = items(&open, &|_| busy_day());
        assert_eq!(busy[0].hint, "进行中");
    }

    /// 缺席要**说清是谁**，不是一句「缺 2 人」——用户得知道该买哪个角色（规格 §7.1）。
    #[test]
    fn a_missing_member_is_named_never_swallowed() {
        let open: Vec<&str> = all_open()
            .into_iter()
            .filter(|id| *id != "wang-hai-yan" && *id != "xiao-song")
            .collect();
        let rows = items(&open, &empty_day);
        let t = &rows[0];
        assert_eq!(t.missing, 2);
        assert!(t.can_run, "接口人还在，桌子就还开得起来");
        assert_eq!(t.hint, "缺 2 人 · 需要「情报王海燕」「记者小宋」");
        let absent: Vec<&str> = t
            .members
            .iter()
            .filter(|m| !m.present)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(
            absent,
            vec!["情报王海燕", "记者小宋"],
            "缺席的人还在名册里，只是标着缺席"
        );
    }

    /// 接口人不在 → 没人接。这条比「缺人」更硬：桌子直接跑不起来。
    #[test]
    fn a_table_without_its_interface_cannot_run() {
        let open: Vec<&str> = all_open()
            .into_iter()
            .filter(|id| *id != "lv-lao-shi")
            .collect();
        let rows = items(&open, &empty_day);
        assert!(!rows[0].can_run);
        assert_eq!(rows[0].hint, "缺接口人 · 需要「主编吕老师」");
    }

    /// 接了棒，侧栏与名册上的说话人就得换人——不许永远钉在接口人身上。
    #[test]
    fn the_baton_moves_who_is_speaking() {
        let open = all_open();
        assert_eq!(items(&open, &empty_day)[0].speaker_id, "wang-hai-yan");
        let handed = |_: &str| {
            let mut d = empty_day("");
            d.holder_id = "xiao-song".into();
            d.holder_name = "记者小宋".into();
            d
        };
        assert_eq!(items(&open, &handed)[0].speaker_id, "xiao-song");
        // 交到桌上没有的人手上 → 装作没交过（判据只有一处，不许瞎认）
        let off_table = |_: &str| {
            let mut d = empty_day("");
            d.holder_id = "nobody".into();
            d
        };
        assert_eq!(items(&open, &off_table)[0].speaker_id, "wang-hai-yan");
    }

    #[test]
    fn an_unknown_team_is_refused_not_invented() {
        assert!(require_team_ready("nobody").is_err());
    }
}
