//! 今天这活跑到哪了：产物目录 + 待点头的决定 + 棒在谁手上（`docs/spec/projects.md` §4.3 / §5 / §6）。
//! 界面不再挂「这一期」看板；这里只给点头卡和侧栏小字用。
//!
//! 三件事，都在这个文件里：
//! 1. **接力**：把这一棒交给谁（落成会话事件，关掉 App 也忘不了）。
//! 2. **这一期有什么**：今天的产出目录 + 里面的决定（读文件头，不另存一份状态）。
//! 3. **点头 / 要改**：用户对一份决定的答复（只改我们写的头，正文一字不动）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use hermes_core::decision::{self, Decision, Kind, Status};
use hermes_core::{Artifact, HandoffRecord, SessionEvent, Team};

use crate::error::GuiError;
use crate::state::AppState;

/// 决定在界面上的样子（camelCase；**文件里仍是 snake_case**——那是给人看的文档）。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DecisionView {
    pub kind: Kind,
    pub kind_label: String,
    pub by: String,
    pub by_name: String,
    pub at: String,
    pub why: String,
    pub status: Status,
    pub status_label: String,
    pub items: Vec<DecisionItemView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_note: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DecisionItemView {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 这一期里的一件东西（产物）。带决定头的会多一张卡。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeItem {
    pub rel_path: String,
    pub name: String,
    pub ext: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<DecisionView>,
}

/// 这一棒是怎么传下来的（人类可读）。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HandoffView {
    /// 交出来的人名；`None` = 你亲手交出去的。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub to_name: String,
    pub to_id: String,
    pub at: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeView {
    /// 哪一期（`YYYY-MM-DD`）。
    pub day: String,
    /// 这一棒现在在谁手上（人物 id / 名字）。没交过 → 第一棒采集。
    pub holder_id: String,
    pub holder_name: String,
    /// 接力链（按时间顺序）。
    pub handoffs: Vec<HandoffView>,
    /// 今天的产物。
    pub items: Vec<EpisodeItem>,
    /// 有没有**待你点头**的决定——侧栏那一行小字与点头卡都要用它。
    pub pending_decision: bool,
}

/// 决定头最多读这么多字节：它是文件**开头**的一小段，没必要为一张卡读完整份稿子。
const HEAD_BYTES: u64 = 8 * 1024;

fn persona_name(id: &str) -> String {
    hermes_core::persona::get(id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// 这份文件是不是一份决定（只读开头）。
fn read_decision(path: &Path) -> Option<Decision> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; HEAD_BYTES as usize];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    decision::parse(&String::from_utf8_lossy(&buf))
}

fn view_of(d: &Decision) -> DecisionView {
    DecisionView {
        kind: d.kind,
        kind_label: d.kind.label().to_string(),
        by: d.by.clone(),
        by_name: persona_name(&d.by),
        at: d.at.with_timezone(&chrono::Local).to_rfc3339(),
        why: d.why.clone(),
        status: d.status,
        status_label: d.status.label().to_string(),
        items: d
            .items
            .iter()
            .map(|i| DecisionItemView {
                title: i.title.clone(),
                note: i.note.clone(),
            })
            .collect(),
        verdict: d.verdict.clone(),
        answered_at: d
            .answered_at
            .map(|t| t.with_timezone(&chrono::Local).to_rfc3339()),
        answered_note: d.answered_note.clone(),
    }
}

/// 今天这一期的产物：直接扫工作区今天的产出目录。
///
/// **不另存一份「这一期有什么」**——真源就是文件本身（`docs/spec/projects.md` §4.3：
/// 期 = 日期目录）。所以用户在 Finder 里删了一个文件，这里当场就少一件。
fn episode_items(workspace: &Path, day: &str) -> Vec<EpisodeItem> {
    let groups = crate::commands::outputs::collect_outputs(workspace);
    let Some(group) = groups.into_iter().find(|g| g.day.as_deref() == Some(day)) else {
        return Vec::new();
    };
    group
        .items
        .into_iter()
        .map(|item| {
            let path = workspace.join(&item.rel_path);
            let decision = read_decision(&path).map(|d| view_of(&d));
            EpisodeItem {
                rel_path: item.rel_path,
                name: item.name,
                ext: item.ext,
                modified: item.modified,
                decision,
            }
        })
        .collect()
}

fn artifact_of(item: &EpisodeItem) -> Artifact {
    let modified = chrono::DateTime::parse_from_rfc3339(&item.modified)
        .map(|t| t.with_timezone(&chrono::Utc))
        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
    Artifact {
        topic_status: item
            .decision
            .as_ref()
            .and_then(|d| (d.kind == Kind::TopicList).then_some(d.status)),
        modified,
    }
}

/// 按今天的产物把棒往前推，并落成接力事件。用户点名桌上的人可以跳步 / 加料。
pub(crate) fn sync_baton(
    active: &mut crate::state::ActiveSession,
    workspace: &Path,
    user_text: Option<&str>,
) -> Result<(), GuiError> {
    let Some(team_id) = active.session.meta.team.clone() else {
        return Ok(());
    };
    let Some(team) = hermes_core::team::get(&team_id) else {
        return Ok(());
    };
    let day = chrono::Local::now().date_naive().to_string();
    let artifacts: Vec<Artifact> = episode_items(workspace, &day)
        .iter()
        .map(artifact_of)
        .collect();
    let open = crate::commands::personas::open_ids();
    let at = chrono::Utc::now();
    let current = active
        .session
        .flow
        .holder()
        .filter(|h| team.member(h).is_some())
        .unwrap_or_else(|| hermes_core::start_id(team))
        .to_string();
    let since = active.session.flow.handoffs().last().map(|h| h.at);
    let mut moves = hermes_core::catch_up(team, current.clone(), since, &artifacts, &open, at);
    if let Some(named) = user_text.and_then(|text| hermes_core::named_member(team, text)) {
        let after = moves
            .last()
            .map(|(_, to)| to.as_str())
            .unwrap_or(current.as_str());
        if named != after && open.contains(&named) {
            moves.push((after.to_string(), named.to_string()));
        }
    }
    if moves.is_empty() {
        return Ok(());
    }
    for (from, to) in moves {
        let rec = HandoffRecord {
            from: Some(from),
            to,
            at,
            note: None,
        };
        let writer = active
            .ensure_writer()
            .map_err(|e| GuiError::Session(e.to_string()))?;
        writer
            .append(&SessionEvent::Handoff(rec.clone()))
            .map_err(|e| GuiError::Session(e.to_string()))?;
        active.session.flow.push(rec);
    }
    Ok(())
}

/// 项目组那**一条**会话（日复一日，所以按最近活动取）。
fn team_session_path(team_id: &str) -> Option<PathBuf> {
    let dir = hermes_core::data_path("sessions");
    let paths = hermes_store::list_sessions(&dir).ok()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for path in paths {
        let Ok((meta, _)) = hermes_store::read_session_listing(&path) else {
            continue;
        };
        if meta.team.as_deref() != Some(team_id) {
            continue;
        }
        let at = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().map(|(b, _)| at > *b).unwrap_or(true) {
            best = Some((at, path));
        }
    }
    best.map(|(_, p)| p)
}

fn handoff_views(team: &Team, flow: &hermes_core::Flow) -> Vec<HandoffView> {
    let _ = team;
    flow.handoffs()
        .iter()
        .map(|h| HandoffView {
            from_name: h.from.as_deref().map(persona_name),
            to_name: persona_name(&h.to),
            to_id: h.to.clone(),
            at: h.at.with_timezone(&chrono::Local).to_rfc3339(),
        })
        .collect()
}

/// 一个组今天走到了哪。**一处算，两处用**：侧栏那一行小字与点头卡。
pub(crate) struct TeamDay {
    pub items: Vec<EpisodeItem>,
    pub handoffs: Vec<HandoffView>,
    pub holder_id: String,
    pub holder_name: String,
}

impl TeamDay {
    /// 有没有**待你点头**的决定（唯一必停的一步）。
    pub fn pending(&self) -> bool {
        self.items.iter().any(|i| {
            i.decision
                .as_ref()
                .is_some_and(|d| d.status == Status::Pending)
        })
    }
}

pub(crate) fn team_day(workspace: &Path, team_id: &str, day: &str) -> TeamDay {
    let team = hermes_core::team::get(team_id);
    let flow = team_session_path(team_id)
        .and_then(|p| hermes_store::read_flow(&p).ok())
        .unwrap_or_default();
    let items = episode_items(workspace, day);
    let artifacts: Vec<Artifact> = items.iter().map(artifact_of).collect();
    let open = crate::commands::personas::open_ids();
    let holder = if let Some(t) = team {
        hermes_core::effective_holder(
            t,
            flow.holder(),
            flow.handoffs().last().map(|h| h.at),
            &artifacts,
            &open,
            chrono::Utc::now(),
        )
    } else {
        String::new()
    };
    TeamDay {
        items,
        handoffs: team.map(|t| handoff_views(t, &flow)).unwrap_or_default(),
        holder_name: persona_name(&holder),
        holder_id: holder,
    }
}

#[tauri::command]
pub fn list_episode(state: State<'_, AppState>, team_id: String) -> Result<EpisodeView, GuiError> {
    hermes_core::team::get(&team_id)
        .ok_or_else(|| GuiError::NotFound(format!("team {team_id}")))?;
    let day = chrono::Local::now().date_naive().to_string();
    let d = team_day(Path::new(&state.workspace_root()), &team_id, &day);
    let pending_decision = d.pending();
    Ok(EpisodeView {
        day,
        holder_id: d.holder_id,
        holder_name: d.holder_name,
        handoffs: d.handoffs,
        pending_decision,
        items: d.items,
    })
}

/// 接力前的两道门（纯函数，可测）：**在这张桌子上**，而且**他还开着**。
///
/// 缺席的人不许接——「该步没人接就停在那」（规格 §7.1），而不是假装交出去了。
fn check_handoff_to(team: &Team, to: &str, open: &[&str]) -> Result<(), GuiError> {
    if team.member(to).is_none() {
        return Err(GuiError::NotFound(format!("{to} 不在这张桌子上——交不出去")));
    }
    if !open.contains(&to) {
        return Err(GuiError::NotFound(format!(
            "{} 现在缺席（不在你的授权里）——该步没人接就停在那，别假装交出去了",
            persona_name(to)
        )));
    }
    Ok(())
}

/// 把这一棒交给谁（引擎内部用：产物推进 / 点名跳步）。用户主路径不再点「交给…」。
pub async fn hand_off_at(
    sessions: &crate::state::Sessions,
    team_id: &str,
    session_id: &str,
    to: &str,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<HandoffRecord, GuiError> {
    let team = hermes_core::team::get(team_id)
        .ok_or_else(|| GuiError::NotFound(format!("team {team_id}")))?;
    check_handoff_to(team, to, &crate::commands::personas::open_ids())?;

    let mut map = sessions.lock().await;
    let active = map
        .get_mut(session_id)
        .ok_or_else(|| GuiError::NotFound(format!("session {session_id} 不在当前活动会话里")))?;
    if active.writer.is_none() {
        return Err(GuiError::Internal(
            "这条会话是别的渠道进来的，只读——接力要在桌面端做".into(),
        ));
    }
    let rec = HandoffRecord {
        from: active.session.flow.holder().map(str::to_string),
        to: to.to_string(),
        at,
        note: None,
    };
    let writer = active
        .ensure_writer()
        .map_err(|e| GuiError::Session(e.to_string()))?;
    writer
        .append(&SessionEvent::Handoff(rec.clone()))
        .map_err(|e| GuiError::Session(e.to_string()))?;
    active.session.flow.push(rec.clone());
    Ok(rec)
}

#[tauri::command]
pub async fn hand_off(
    state: State<'_, AppState>,
    session_id: String,
    to: String,
) -> Result<(), GuiError> {
    let team_id =
        {
            let map = state.sessions.lock().await;
            let active = map
                .get(&session_id)
                .ok_or_else(|| GuiError::NotFound(format!("session {session_id}")))?;
            active.session.meta.team.clone().ok_or_else(|| {
                GuiError::Internal("这条会话不属于任何项目组——接力是组里的事".into())
            })?
        };
    hand_off_at(
        &state.sessions,
        &team_id,
        &session_id,
        &to,
        chrono::Utc::now(),
    )
    .await?;
    Ok(())
}

/// 用户对一份决定的答复：点头 / 要改。
///
/// **只改我们写的那段头**（状态、答复时间、理由），正文一字不动——那是人物写的，
/// 不属于引擎。用户手改过、头读不出来 → 报错，不猜。
pub fn apply_answer(
    raw: &str,
    approve: bool,
    note: Option<&str>,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<String, GuiError> {
    let mut d = decision::parse(raw).ok_or_else(|| {
        GuiError::Internal("这份文件的开头不是一份决定（可能被手改过）——不改它".into())
    })?;
    d.status = if approve {
        Status::Approved
    } else {
        Status::Revised
    };
    d.answered_at = Some(at);
    d.answered_note = note
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(decision::rewrite(raw, &d))
}

#[tauri::command]
pub async fn answer_decision(
    state: State<'_, AppState>,
    path: String,
    approve: bool,
    note: Option<String>,
) -> Result<(), GuiError> {
    let workspace = PathBuf::from(state.workspace_root());
    let resolved = crate::commands::outputs::resolve_workspace_file(&workspace, &path)?;
    let raw = std::fs::read_to_string(&resolved)
        .map_err(|e| GuiError::Internal(format!("读不动 {}：{e}", resolved.display())))?;
    let out = apply_answer(&raw, approve, note.as_deref(), chrono::Utc::now())?;
    std::fs::write(&resolved, out.as_bytes())
        .map_err(|e| GuiError::Internal(format!("写不动 {}：{e}", resolved.display())))?;
    if approve {
        let mut map = state.sessions.lock().await;
        for active in map.values_mut() {
            if active.session.meta.team.is_some() {
                let _ = sync_baton(active, &workspace, None);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision_file(day: &str, status: Status) -> String {
        let d = Decision {
            kind: Kind::TopicList,
            by: "lv-lao-shi".into(),
            at: "2026-09-17T06:03:00Z".parse().unwrap(),
            why: "今日产业类只剩两条硬货".into(),
            status,
            episode: day.into(),
            team: Some("caifu-zaozhidao".into()),
            items: vec![decision::Item {
                title: "央行开展 5000 亿 MLF".into(),
                note: None,
            }],
            verdict: None,
            answered_at: None,
            answered_note: None,
        };
        decision::render(&d, "# 选题单\n\n正文\n")
    }

    /// 这一期只认今天那一格：昨天的产物不许混进来。
    #[test]
    fn the_episode_is_today_and_the_decision_is_read_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        std::fs::create_dir_all(ws.join("outputs/2026-09-17")).unwrap();
        std::fs::create_dir_all(ws.join("outputs/2026-09-16")).unwrap();
        std::fs::write(
            ws.join("outputs/2026-09-17/选题单-2026-09-17.md"),
            decision_file("2026-09-17", Status::Pending),
        )
        .unwrap();
        std::fs::write(ws.join("outputs/2026-09-17/财经资讯.md"), "普通产物").unwrap();
        std::fs::write(
            ws.join("outputs/2026-09-16/选题单-2026-09-16.md"),
            decision_file("2026-09-16", Status::Pending),
        )
        .unwrap();

        let items = episode_items(ws, "2026-09-17");
        assert_eq!(items.len(), 2, "只该有今天那两份：{items:?}");
        let card = items
            .iter()
            .find_map(|i| i.decision.as_ref())
            .expect("选题单要成卡");
        assert_eq!(card.kind_label, "选题单");
        assert_eq!(card.by_name, "主编吕老师", "人物 id 要翻成人名");
        assert_eq!(card.items[0].title, "央行开展 5000 亿 MLF");
        assert_eq!(card.status_label, "待点头");
        let plain = items.iter().find(|i| i.name == "财经资讯.md").unwrap();
        assert!(plain.decision.is_none(), "普通产物不带卡");
    }

    /// 点头：状态与理由落进文件，正文一个字不动。
    #[test]
    fn answering_stamps_the_status_and_keeps_the_writer_s_words() {
        let raw = decision_file("2026-09-17", Status::Pending);
        let out = apply_answer(
            &raw,
            false,
            Some("第二条换掉，公告太多"),
            "2026-09-17T07:05:00Z".parse().unwrap(),
        )
        .unwrap();
        let d = decision::parse(&out).unwrap();
        assert_eq!(d.status, Status::Revised);
        assert_eq!(d.answered_note.as_deref(), Some("第二条换掉，公告太多"));
        assert!(out.ends_with("# 选题单\n\n正文\n"), "正文不许被动：{out}");

        assert!(
            apply_answer("随便一份文件", true, None, chrono::Utc::now()).is_err(),
            "读不出头就不改——不猜"
        );
    }

    /// 接力前两道门：不在桌上 → 交不出去；在桌上但缺席 → 停在那，不许假装交出去。
    #[test]
    fn a_handoff_is_refused_off_the_table_and_to_someone_absent() {
        let team = hermes_core::team::get("caifu-zaozhidao").unwrap();
        let all: Vec<&str> = team.members.iter().map(|m| m.id.as_str()).collect();

        assert!(check_handoff_to(team, "xiao-song", &all).is_ok());
        assert!(
            check_handoff_to(team, "da-dao-yan", &all).is_err(),
            "组外的人不在这张桌子上"
        );
        assert!(
            check_handoff_to(team, "yu-tian", &all).is_err(),
            "工位还在，但不坐这张桌"
        );
        assert!(
            check_handoff_to(team, "xiao-yu", &all).is_err(),
            "口播那一棒取消了，小雨不在桌上"
        );
        let err = check_handoff_to(team, "xiao-song", &["lv-lao-shi"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("缺席") && err.contains("记者小宋"), "{err}");
    }
}
