//! `decision` —— 把一份决定记下来（选题单 / 审稿意见）。
//!
//! 决定的**结构**（谁定的 / 什么时候 / 为什么 / 清单 / 状态）由这里写；**正文**是
//! 人物自己写好的那一份文件，一个字都不动（`docs/spec/projects.md` §6）。
//!
//! 「谁在说」「哪一期」「哪个组」是**引擎盖上去的**（[`Stamp`] + [`StampedHost`]）：
//! 模型说了不算——它连自己的人物 id 都不该知道，更不该有机会写错。
//! 工作区里跑（CLI / 测试）没有会话上下文时，工具会**明说**记不了，而不是瞎填一个作者。

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::decision::{self, Decision, Kind, Status};
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};
use serde::Deserialize;

use crate::safety;

/// 引擎盖在决定上的事实。模型看不见、也改不了。
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    /// 这一轮谁在说（人物 id）。
    pub by: String,
    /// 属于哪个项目组（工位会话 / 自由对话为 `None`）。
    pub team: Option<String>,
    /// 哪一期（`YYYY-MM-DD`）。
    pub episode: String,
}

pub fn handles(name: &str) -> bool {
    name == "decision"
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "decision".into(),
        description: "Record a **decision** — a topic list (what runs tonight, in what order, \
            and why) or a review (pass / send back, with the reason).\n\
            \n\
            Write the document first, record it second: this tool changes **no word of the body** — \
            it only puts the four facts at the top of the file (who decided, when, why, what). \
            The file must already exist; write it with `write` first \
            (workspace-relative, by default `outputs/<today>/…`).\n\
            \n\
            `why` is the point: not 「today's content」 but **why it was decided that way** — \
            in a project group that line is what makes the decision reviewable later.\n\
            \n\
            `items` is for a topic list only: one entry per run, **in air order** — \
            the one-line `title` and, if useful, a short `note` (why it runs).\n\
            `verdict` is for a review only: `放行` or `打回` (on a send-back, put the reason in `why`)."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["topic-list", "review"],
                    "description": "topic-list = 选题单；review = 审稿意见。"
                },
                "path": {
                    "type": "string",
                    "description": "工作区相对路径：你已经写好的那份文件（如 outputs/2026-09-17/选题单-2026-09-17.md）。"
                },
                "why": {
                    "type": "string",
                    "description": "为什么这么定（一句话，必写）。"
                },
                "items": {
                    "type": "array",
                    "description": "选题单的清单，按播出顺序。",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "note": {"type": "string"}
                        },
                        "required": ["title"]
                    }
                },
                "verdict": {
                    "type": "string",
                    "description": "审稿意见才有：`放行` 或 `打回`。"
                }
            },
            "required": ["kind", "path", "why"]
        }),
        requires_confirmation: false,
    }
}

#[derive(Deserialize)]
struct Args {
    kind: Kind,
    path: String,
    why: String,
    #[serde(default)]
    items: Vec<decision::Item>,
    #[serde(default)]
    verdict: Option<String>,
    /// 引擎盖的（[`StampedHost`] 注入）；模型给不了。
    #[serde(default, rename = "_engine")]
    engine: Option<StampArgs>,
}

#[derive(Deserialize)]
struct StampArgs {
    by: String,
    #[serde(default)]
    team: Option<String>,
    episode: String,
}

fn refuse(msg: impl Into<String>) -> ToolCallOutcome {
    ToolCallOutcome {
        content: msg.into(),
        is_error: true,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| Error::ToolHost(format!("decision: 参数不对：{e}")))?;

    let Some(engine) = a.engine else {
        return Ok(refuse(
            "decision: 记决定得先知道「谁定的」——请在某个工位或项目组里记这份决定；\
             自由对话里没有人物，这份决定会缺掉四样里的一样。",
        ));
    };
    if a.why.trim().is_empty() {
        return Ok(refuse(
            "decision: `why` 不能空——「为什么这么定」是这份决定四样里最不能省的一样。",
        ));
    }
    if a.kind == Kind::Review && a.verdict.as_deref().unwrap_or("").trim().is_empty() {
        return Ok(refuse(
            "decision: 审稿意见必须给 `verdict`（`放行` 或 `打回`）——\
             打回不给理由，稿子没法改。",
        ));
    }

    let (path, _export) = safety::resolve_for_write(workspace, &a.path)?;
    let Ok(raw) = tokio::fs::read_to_string(&path).await else {
        return Ok(refuse(format!(
            "decision: 找不到 {} —— 先用 `write` 把这份东西写成文件，再来记。\
             （正文归你写，这里只加头。）",
            a.path
        )));
    };
    // 已经是决定文件时**只换头**：正文必须原样留着。
    let d = Decision {
        kind: a.kind,
        by: engine.by,
        at: chrono::Utc::now(),
        why: a.why.trim().to_string(),
        status: match a.kind {
            Kind::TopicList => Status::Pending,
            Kind::Review => Status::Settled,
        },
        episode: if engine.episode.is_empty() {
            decision::episode_of(&a.path, chrono::Local::now().date_naive())
        } else {
            engine.episode
        },
        team: engine.team,
        items: a.items,
        verdict: a.verdict.filter(|v| !v.trim().is_empty()),
        // 重记一份决定 = 重新定了一次，上一次的答复不再作数。
        answered_at: None,
        answered_note: None,
    };
    let out = decision::rewrite(&raw, &d);
    tokio::fs::write(&path, out.as_bytes())
        .await
        .map_err(|e| Error::ToolHost(format!("decision 写 {}: {e}", path.display())))?;

    let tail = match d.kind {
        Kind::TopicList => format!(
            "{} · {} 条 · {}",
            d.kind.label(),
            d.items.len(),
            d.status.label()
        ),
        Kind::Review => format!(
            "{} · {}",
            d.kind.label(),
            d.verdict.as_deref().unwrap_or(d.status.label())
        ),
    };
    Ok(ToolCallOutcome {
        content: match d.kind {
            Kind::TopicList => format!(
                "Recorded decision [{}]: {tail} —— 用户会在选题单上看到它，等他点头才往下走。",
                a.path
            ),
            Kind::Review => format!("Recorded decision [{}]: {tail}", a.path),
        },
        is_error: false,
    })
}

/// 把 [`Stamp`] 盖到 `decision` 的调用上，其余工具原样转给里面的 host。
///
/// 这样做是为了让「谁定的」只有一个来源：**引擎知道，模型不需要知道**。
pub struct StampedHost {
    inner: Arc<dyn ToolHost>,
    stamp: Stamp,
}

impl StampedHost {
    pub fn new(inner: Arc<dyn ToolHost>, stamp: Stamp) -> Self {
        Self { inner, stamp }
    }
}

#[async_trait]
impl ToolHost for StampedHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        self.inner.list_tools().await
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
        if !handles(name) {
            return self.inner.call(name, args).await;
        }
        let mut merged = match args {
            serde_json::Value::Object(map) => map,
            other => return self.inner.call(name, other).await,
        };
        // 引擎的值**覆盖**任何同名键：模型自己塞的 _engine 不作数。
        merged.insert(
            "_engine".into(),
            serde_json::json!({
                "by": self.stamp.by,
                "team": self.stamp.team,
                "episode": self.stamp.episode,
            }),
        );
        self.inner
            .call(name, serde_json::Value::Object(merged))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn stamp() -> serde_json::Value {
        serde_json::json!({
            "_engine": {"by": "lv-lao-shi", "team": "caifu-zaozhidao", "episode": "2026-09-17"}
        })
    }

    fn with_stamp(mut body: serde_json::Value) -> serde_json::Value {
        let map = body.as_object_mut().unwrap();
        for (k, v) in stamp().as_object().unwrap() {
            map.insert(k.clone(), v.clone());
        }
        body
    }

    #[tokio::test]
    async fn records_the_four_facts_and_leaves_the_body_alone() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("outputs/2026-09-17")).unwrap();
        std::fs::write(
            dir.path().join("outputs/2026-09-17/选题单-2026-09-17.md"),
            "# 选题单 · 9 月 17 日\n\n1. 央行…\n",
        )
        .unwrap();

        let out = run(
            dir.path(),
            with_stamp(serde_json::json!({
                "kind": "topic-list",
                "path": "outputs/2026-09-17/选题单-2026-09-17.md",
                "why": "今日产业类只剩两条硬货",
                "items": [{"title": "央行开展 5000 亿 MLF", "note": "宏观头一条"}]
            })),
        )
        .await
        .unwrap();
        assert!(!out.is_error, "{}", out.content);

        let text =
            std::fs::read_to_string(dir.path().join("outputs/2026-09-17/选题单-2026-09-17.md"))
                .unwrap();
        assert!(
            text.contains("# 选题单 · 9 月 17 日"),
            "正文一字不动：{text}"
        );
        let d = decision::parse(&text).unwrap();
        assert_eq!(d.by, "lv-lao-shi", "谁定的由引擎盖，模型说了不算");
        assert_eq!(d.status, Status::Pending);
        assert_eq!(d.episode, "2026-09-17");
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.team.as_deref(), Some("caifu-zaozhidao"));
    }

    /// 同一条决定重记一次 = 换头，不是叠第二层头（否则「谁定的」会有两份）。
    #[tokio::test]
    async fn recording_twice_replaces_the_header_instead_of_stacking_it() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("outputs/2026-09-17/审稿意见.md");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "意见：数字都对，但口径对不上公告。\n").unwrap();

        for _ in 0..2 {
            run(
                dir.path(),
                with_stamp(serde_json::json!({
                    "kind": "review",
                    "path": "outputs/2026-09-17/审稿意见.md",
                    "why": "数字与公告对不上",
                    "verdict": "打回"
                })),
            )
            .await
            .unwrap();
        }
        let text = std::fs::read_to_string(&p).unwrap();
        assert_eq!(text.matches("---").count(), 2, "只该有一对分隔线：{text}");
        assert!(text.contains("意见：数字都对"));
        assert_eq!(
            decision::parse(&text).unwrap().verdict.as_deref(),
            Some("打回")
        );
        assert_eq!(text.matches("by: lv-lao-shi").count(), 1);
    }

    #[tokio::test]
    async fn refuses_instead_of_half_writing() {
        let dir = tempdir().unwrap();

        // 没有引擎盖的作者 → 明说记不了，而不是瞎填一个。
        let out = run(
            dir.path(),
            serde_json::json!({"kind": "topic-list", "path": "x.md", "why": "y"}),
        )
        .await
        .unwrap();
        assert!(out.is_error && out.content.contains("谁定的"));

        // 文件不存在 → 让模型先去写正文，而不是造一份只有头的空决定。
        let out = run(
            dir.path(),
            with_stamp(serde_json::json!({
                "kind": "topic-list", "path": "outputs/x.md", "why": "y"
            })),
        )
        .await
        .unwrap();
        assert!(out.is_error && out.content.contains("先用 `write`"));
        assert!(!dir.path().join("outputs/x.md").exists(), "不许留半截文件");

        // 审稿意见没有放行/打回 → 拒。
        std::fs::write(dir.path().join("r.md"), "正文").unwrap();
        let out = run(
            dir.path(),
            with_stamp(serde_json::json!({
                "kind": "review", "path": "r.md", "why": "看了"
            })),
        )
        .await
        .unwrap();
        assert!(out.is_error && out.content.contains("verdict"));

        // 空的 why → 拒（四样缺一不可）。
        let out = run(
            dir.path(),
            with_stamp(serde_json::json!({
                "kind": "review", "path": "r.md", "why": "  ", "verdict": "放行"
            })),
        )
        .await
        .unwrap();
        assert!(out.is_error && out.content.contains("why"));
    }
}
