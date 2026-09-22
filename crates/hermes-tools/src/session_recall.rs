//! 翻旧账：模型能翻回**本会话**更早的对话。
//!
//! 上下文压缩把旧轮次换成摘要，摘要是有损的——用户问「上周三为什么把半导体压在末尾」，
//! 模型手里已经没有原文了，但**文件里有**。这个工具就是那条回去的路。
//!
//! 与 [`crate::persona_scope::PersonaToolHost`] 同形：per-turn 的薄壳，只多一条工具，
//! 其余原样转发。**路径由引擎给，模型给不了**——工具只认构造时绑定的那一条会话，
//! 所以它翻不出别人的会话。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::{Error, Result, ToolCallOutcome, ToolHost, ToolSpec};
use serde::Deserialize;

pub const TOOL_NAME: &str = "conversation_recall";

pub struct SessionRecallHost {
    inner: Arc<dyn ToolHost>,
    path: PathBuf,
}

impl SessionRecallHost {
    pub fn new(inner: Arc<dyn ToolHost>, path: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            path: path.into(),
        }
    }

    pub fn spec() -> ToolSpec {
        ToolSpec {
            name: TOOL_NAME.into(),
            description:
                "Look back at earlier turns of THIS conversation (including parts already \
                 compacted out of your context). Use it when the user refers to something said \
                 on an earlier day and you don't have it: `query` is what to look for (a word, a \
                 name, a number). Returns the matching lines with the day they were said. \
                 Scanning a long conversation can take a moment — say you are looking back."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "What to look for in this conversation's earlier turns"},
                    "limit": {"type": "integer", "description": "Max hits (default 6)"}
                },
                "required": ["query"]
            }),
            requires_confirmation: false,
        }
    }
}

#[derive(Deserialize)]
struct RecallArgs {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl ToolHost for SessionRecallHost {
    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        let mut tools = self.inner.list_tools().await?;
        tools.push(Self::spec());
        Ok(tools)
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolCallOutcome> {
        if name != TOOL_NAME {
            return self.inner.call(name, args).await;
        }
        let a: RecallArgs = serde_json::from_value(args)
            .map_err(|e| Error::ToolHost(format!("{TOOL_NAME}: bad args: {e}")))?;
        // 上限从 20 提到 200：折叠后的历史写明了「原文用 session_recall 翻」——
        // 一次最多给 20 行的话，那句话就是空头支票（P1-14）。
        let limit = a.limit.unwrap_or(6).clamp(1, 200);

        let hits = hermes_store::recall_in_session(&self.path, &a.query, limit)
            .map_err(|e| Error::ToolHost(format!("{TOOL_NAME}: {e}")))?;

        if hits.is_empty() {
            // 没说过就是没说过：明说翻过、没找到，**不许编**。
            return Ok(ToolCallOutcome {
                content: format!(
                    "Looked back through this conversation: nothing matches '{}'.",
                    a.query
                ),
                is_error: false,
            });
        }

        let mut buf = format!("Earlier in this conversation (matching '{}'):\n", a.query);
        for h in &hits {
            let when = h.day.as_deref().map(hermes_store::label_for);
            match when {
                Some(day) => buf.push_str(&format!("\n[{day} · {}] {}\n", h.role, h.text)),
                None => buf.push_str(&format!("\n[{}] {}\n", h.role, h.text)),
            }
        }
        Ok(ToolCallOutcome {
            content: buf,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_core::SessionEvent;

    struct Nop;

    #[async_trait]
    impl ToolHost for Nop {
        async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
            Ok(vec![])
        }
        async fn call(&self, name: &str, _args: serde_json::Value) -> Result<ToolCallOutcome> {
            Ok(ToolCallOutcome {
                content: format!("inner:{name}"),
                is_error: false,
            })
        }
    }

    fn write_session(dir: &std::path::Path, lines: Vec<SessionEvent>) -> PathBuf {
        let path = dir.join("s.jsonl");
        let body: Vec<String> = lines
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect();
        std::fs::write(&path, body.join("\n") + "\n").unwrap();
        path
    }

    fn said(text: &str, at: &str) -> hermes_core::Message {
        let mut m = hermes_core::Message::user_text(text);
        m.at = Some(at.parse().unwrap());
        m
    }

    #[tokio::test]
    async fn the_tool_is_offered_and_the_rest_falls_through_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            vec![SessionEvent::Meta(hermes_core::SessionMeta::new("m", "p"))],
        );
        let host = SessionRecallHost::new(Arc::new(Nop), path);
        assert_eq!(host.list_tools().await.unwrap().len(), 1);
        assert_eq!(host.list_tools().await.unwrap()[0].name, TOOL_NAME);
        assert_eq!(
            host.call("read", serde_json::json!({}))
                .await
                .unwrap()
                .content,
            "inner:read",
            "别的工具原样转发"
        );
    }

    #[tokio::test]
    async fn it_finds_what_was_said_on_which_day_and_never_invents_a_hit() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            vec![
                SessionEvent::Meta(hermes_core::SessionMeta::new("m", "p")),
                SessionEvent::Message(said("半导体先压末尾", "2026-09-16T02:00:00Z")),
                SessionEvent::Message(said("今天说别的", "2026-09-17T02:00:00Z")),
            ],
        );
        let host = SessionRecallHost::new(Arc::new(Nop), path);

        let hit = host
            .call(TOOL_NAME, serde_json::json!({"query": "半导体"}))
            .await
            .unwrap();
        assert!(!hit.is_error);
        assert!(hit.content.contains("压末尾"), "{}", hit.content);
        assert!(
            hit.content.contains("9 月 16 日"),
            "得说清是哪天：{}",
            hit.content
        );

        let miss = host
            .call(TOOL_NAME, serde_json::json!({"query": "压根没提过的事"}))
            .await
            .unwrap();
        assert!(!miss.is_error, "没找到不是错误");
        assert!(miss.content.contains("nothing matches"), "{}", miss.content);
    }
}
