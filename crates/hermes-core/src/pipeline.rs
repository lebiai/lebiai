//! 项目组里棒怎么交：产出物就是棒，不是用户点人名。
//!
//! 名册顺序就是流水线。没交过棒 = 第一个人（采）。接口人那一步是选题门：
//! 选题单待点头就停，点了头才往下。模型不许自己宣布交了棒。

use chrono::{DateTime, Utc};

use crate::decision::Status;
use crate::team::Team;

/// 今天的一份产物。选题单带状态；其余是料 / 稿。
#[derive(Debug, Clone)]
pub struct Artifact {
    pub topic_status: Option<Status>,
    pub modified: DateTime<Utc>,
}

/// 开工第一棒：名册第一个人。
pub fn start_id(team: &Team) -> &str {
    team.members
        .first()
        .map(|m| m.id.as_str())
        .unwrap_or(team.interface.as_str())
}

/// 没交过棒、或交了个不在桌上的人：回落第一棒，不再回落接口人。
pub fn fallback_holder(team: &Team) -> &str {
    start_id(team)
}

pub fn next_after<'a>(team: &'a Team, id: &str) -> Option<&'a str> {
    let i = team.members.iter().position(|m| m.id == id)?;
    team.members.get(i + 1).map(|m| m.id.as_str())
}

/// 这一棒该不该交出去。`since` = 这个人接到棒的时间；没交过 = 从一天的开头算。
pub fn next_holder<'a>(
    team: &'a Team,
    holder: &str,
    artifacts: &[Artifact],
    since: Option<DateTime<Utc>>,
) -> Option<&'a str> {
    let next = next_after(team, holder)?;
    if holder == team.interface {
        let approved = artifacts
            .iter()
            .any(|a| a.topic_status == Some(Status::Approved));
        return approved.then_some(next);
    }
    let since = since.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let work = artifacts
        .iter()
        .any(|a| a.topic_status.is_none() && a.modified > since);
    work.then_some(next)
}

/// 按产物把棒往前推，直到该停。返回 (from, to)。下一个人缺席就停。
pub fn catch_up(
    team: &Team,
    mut holder: String,
    mut since: Option<DateTime<Utc>>,
    artifacts: &[Artifact],
    present: &[&str],
    at: DateTime<Utc>,
) -> Vec<(String, String)> {
    let mut moves = Vec::new();
    while let Some(next) = next_holder(team, &holder, artifacts, since) {
        if !present.contains(&next) {
            break;
        }
        moves.push((holder.clone(), next.to_string()));
        holder = next.to_string();
        since = Some(at);
    }
    moves
}

/// 现在棒该在谁手上（只算，不落盘）。
pub fn effective_holder(
    team: &Team,
    flow_holder: Option<&str>,
    since: Option<DateTime<Utc>>,
    artifacts: &[Artifact],
    present: &[&str],
    at: DateTime<Utc>,
) -> String {
    let holder = flow_holder
        .filter(|h| team.member(h).is_some())
        .unwrap_or_else(|| start_id(team))
        .to_string();
    match catch_up(team, holder.clone(), since, artifacts, present, at).last() {
        Some((_, to)) => to.clone(),
        None => holder,
    }
}

/// 用户这句话点名了桌上唯一一个人 → 把棒交给他（跳步 / 加料）。点了两个就不猜。
pub fn named_member<'a>(team: &'a Team, text: &str) -> Option<&'a str> {
    if text.trim().is_empty() {
        return None;
    }
    let mut hits: Vec<&str> = Vec::new();
    for m in &team.members {
        let Some(p) = crate::persona::get(&m.id) else {
            continue;
        };
        let hit = std::iter::once(p.name.as_str())
            .chain(p.aka.iter().map(String::as_str))
            .any(|n| !n.is_empty() && text.contains(n));
        if hit && !hits.contains(&m.id.as_str()) {
            hits.push(m.id.as_str());
        }
    }
    (hits.len() == 1).then(|| hits[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team;

    fn t() -> &'static Team {
        team::get("caifu-zaozhidao").unwrap()
    }

    fn all() -> Vec<&'static str> {
        t().members.iter().map(|m| m.id.as_str()).collect()
    }

    fn at() -> DateTime<Utc> {
        "2026-09-20T01:00:00Z".parse().unwrap()
    }

    fn work(when: &str) -> Artifact {
        Artifact {
            topic_status: None,
            modified: when.parse().unwrap(),
        }
    }

    fn topic(status: Status) -> Artifact {
        Artifact {
            topic_status: Some(status),
            modified: at(),
        }
    }

    #[test]
    fn the_first_baton_is_gathering_not_the_gate() {
        assert_eq!(start_id(t()), "wang-hai-yan");
        assert_eq!(fallback_holder(t()), "wang-hai-yan");
        assert_eq!(next_after(t(), "wang-hai-yan"), Some("lv-lao-shi"));
        assert_eq!(next_after(t(), "lv-lao-shi"), Some("xiao-song"));
        assert_eq!(
            next_after(t(), "xiao-song"),
            None,
            "小宋交完稿，这一天就走完了"
        );
    }

    #[test]
    fn intel_moves_the_baton_to_the_editor() {
        let present = all();
        let arts = vec![work("2026-09-20T00:10:00Z")];
        let moves = catch_up(t(), "wang-hai-yan".into(), None, &arts, &present, at());
        assert_eq!(
            moves,
            vec![("wang-hai-yan".into(), "lv-lao-shi".into())],
            "料齐了就该选题，不许等用户点交给"
        );
    }

    #[test]
    fn a_pending_topic_list_stops_at_the_gate() {
        let present = all();
        let arts = vec![work("2026-09-20T00:10:00Z"), topic(Status::Pending)];
        let holder = effective_holder(t(), None, None, &arts, &present, at());
        assert_eq!(holder, "lv-lao-shi", "选题没点头，停在吕老师");
        assert!(next_holder(t(), "lv-lao-shi", &arts, Some(at())).is_none());
    }

    #[test]
    fn a_nod_moves_the_baton_to_the_writer() {
        let present = all();
        let arts = vec![work("2026-09-20T00:10:00Z"), topic(Status::Approved)];
        let holder = effective_holder(t(), Some("lv-lao-shi"), Some(at()), &arts, &present, at());
        assert_eq!(holder, "xiao-song");
    }

    #[test]
    fn a_draft_after_the_nod_ends_the_line() {
        let present = all();
        let nod: DateTime<Utc> = "2026-09-20T00:20:00Z".parse().unwrap();
        let arts = vec![
            work("2026-09-20T00:10:00Z"),
            topic(Status::Approved),
            work("2026-09-20T00:30:00Z"),
        ];
        let holder = effective_holder(t(), Some("xiao-song"), Some(nod), &arts, &present, at());
        assert_eq!(holder, "xiao-song", "成稿落盘就是终点，后面没有下一棒");
    }

    #[test]
    fn intel_from_before_the_writer_got_the_baton_does_not_finish_the_line() {
        let nod: DateTime<Utc> = "2026-09-20T00:20:00Z".parse().unwrap();
        let arts = vec![work("2026-09-20T00:10:00Z"), topic(Status::Approved)];
        assert!(
            next_holder(t(), "xiao-song", &arts, Some(nod)).is_none(),
            "海燕的旧料不许被当成小宋的成稿"
        );
    }

    #[test]
    fn an_absent_next_person_stops_the_line() {
        let present = vec!["wang-hai-yan", "xiao-song"];
        let arts = vec![work("2026-09-20T00:10:00Z")];
        let moves = catch_up(t(), "wang-hai-yan".into(), None, &arts, &present, at());
        assert!(moves.is_empty(), "吕老师缺席，不许假装交出去了");
    }

    #[test]
    fn naming_one_person_at_the_table_hands_them_the_baton() {
        assert_eq!(named_member(t(), "海燕再补两条"), Some("wang-hai-yan"));
        assert_eq!(named_member(t(), "让小宋先出一版"), Some("xiao-song"));
        assert_eq!(named_member(t(), "小宋和吕老师都看看"), None);
        assert_eq!(named_member(t(), "今天开盘怎么看"), None);
    }
}
