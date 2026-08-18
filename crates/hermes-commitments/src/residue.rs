//! Leave-session residue: leftover debts, not a second reflection.

use hermes_core::{CompletionRequest, LlmProvider, Message, Session};
use serde::Deserialize;

use crate::store::Commitment;

const MAX_ITEMS: usize = 2;

const SYSTEM: &str = r#"You extract leftover work debts after a conversation stops.

A leftover debt is something the user still OWES after they leave — a deliverable they said they would finish later. It is NOT:
- steps inside one deliverable
- work already finished this session
- a preference / living rule / mood
- a topic they only talked about

Default to ZERO items. False positives are worse than missing one.

Return EXACTLY one JSON object:
{"items":[{"title":"user's own verbs","doneWhen":"optional","softDue":"optional original phrase","softDueDate":"YYYY-MM-DD or omit"}]}

Rules:
- At most 2 items. Prefer 0 or 1.
- title must be the user's wording (verb + object). Never rewrite into empty slogans.
- If they already listed N separate deliverables, you may emit those N only when each has its own done-picture — still cap at 2; pick the ones still owed.
- If unsure, return {"items":[]}.
"#;

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidueItem {
    pub title: String,
    #[serde(default)]
    pub done_when: Option<String>,
    #[serde(default)]
    pub soft_due: Option<String>,
    #[serde(default)]
    pub soft_due_date: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResidueJson {
    #[serde(default)]
    items: Vec<ResidueItem>,
}

/// Cheap gate so we do not call the model on greetings / finished Q&A.
pub fn looks_like_owe_language(text: &str) -> bool {
    let t = text.to_lowercase();
    const MARKERS: &[&str] = &[
        "记下",
        "记一下",
        "收成在办",
        "周五",
        "这周",
        "下周",
        "下次",
        "回头",
        "待会",
        "待會",
        "还要",
        "还欠",
        "记得",
        "别忘",
        "交给",
        "发给",
        "截止",
        "之前做完",
        "写完",
        "改完",
        "later",
        "remind",
        "todo",
        "friday",
        "next week",
        "don't forget",
        "follow up",
    ];
    MARKERS.iter().any(|m| t.contains(m))
}

pub fn session_has_owe_language(session: &Session) -> bool {
    session.messages.iter().any(|m| {
        m.is_human_send()
            && m.content
                .iter()
                .any(|b| b.as_text().is_some_and(looks_like_owe_language))
    })
}

/// Conservative LLM extract. Caller must skip when `session_has_owe_language` is false.
pub async fn scan_residue(
    provider: &dyn LlmProvider,
    session: &Session,
    existing: &[Commitment],
) -> hermes_core::Result<Vec<ResidueItem>> {
    let transcript = format_transcript(session);
    if transcript.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut user = String::new();
    if !existing.is_empty() {
        user.push_str("Already captured (do NOT repeat):\n");
        for c in existing.iter().take(20) {
            user.push_str(&format!("- {}\n", c.title));
        }
        user.push('\n');
    }
    user.push_str("Transcript:\n");
    user.push_str(&transcript);

    let req = CompletionRequest {
        model: String::new(),
        system: Some(SYSTEM.to_string()),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: 512,
        temperature: Some(0.1),
        enable_caching: false,
    };
    let resp = provider.complete(req).await?;
    let text = resp.text();
    let raw = strip_fence(&text);
    let parsed: ResidueJson = serde_json::from_str(raw).unwrap_or_default();
    Ok(parsed
        .items
        .into_iter()
        .filter(|i| !i.title.trim().is_empty())
        .take(MAX_ITEMS)
        .collect())
}

fn format_transcript(session: &Session) -> String {
    let mut buf = String::new();
    for m in &session.messages {
        if !m.is_human_send() && m.role != hermes_core::Role::Assistant {
            continue;
        }
        if m.is_internal_instruction_only() || m.is_tool_result_only() {
            continue;
        }
        let role = match m.role {
            hermes_core::Role::User => "User",
            hermes_core::Role::Assistant => "Assistant",
        };
        for b in &m.content {
            if let Some(t) = b.as_text() {
                let t = t.trim();
                if t.is_empty() {
                    continue;
                }
                let preview: String = t.chars().take(400).collect();
                buf.push_str(&format!("[{role}] {preview}\n"));
            }
        }
    }
    buf
}

const MERGE_SYSTEM: &str = r#"You decide which open-work titles are the SAME leftover debt (same done-picture).
Not the same: different deliverables (draft vs meeting), or a step vs the deliverable.
Default to no pairs. At most one pair.

Return EXACTLY one JSON object:
{"pairs":[{"a":"id","b":"id"}]}
"#;

#[derive(Debug, Deserialize, Default)]
struct MergeJson {
    #[serde(default)]
    pairs: Vec<MergePairRaw>,
}

#[derive(Debug, Deserialize)]
struct MergePairRaw {
    a: String,
    b: String,
}

/// LLM: at most one pair of ids that are the same debt. Conservative.
pub async fn scan_merge_pairs(
    provider: &dyn LlmProvider,
    items: &[Commitment],
) -> hermes_core::Result<Option<(String, String)>> {
    let owed: Vec<&Commitment> = items.iter().filter(|c| c.status.is_owed()).collect();
    if owed.len() < 2 {
        return Ok(None);
    }
    let mut user = String::from("Open work:\n");
    for c in &owed {
        user.push_str(&format!("- {} | {}\n", c.id, c.title));
        for a in &c.aliases {
            user.push_str(&format!("  alias: {a}\n"));
        }
    }
    let req = CompletionRequest {
        model: String::new(),
        system: Some(MERGE_SYSTEM.to_string()),
        messages: vec![Message::user_text(user)],
        tools: Vec::new(),
        max_tokens: 256,
        temperature: Some(0.0),
        enable_caching: false,
    };
    let resp = provider.complete(req).await?;
    let text = resp.text();
    let raw = strip_fence(&text);
    let parsed: MergeJson = serde_json::from_str(raw).unwrap_or_default();
    let ids: std::collections::HashSet<&str> = owed.iter().map(|c| c.id.as_str()).collect();
    Ok(parsed.pairs.into_iter().find_map(|p| {
        if p.a != p.b && ids.contains(p.a.as_str()) && ids.contains(p.b.as_str()) {
            Some((p.a, p.b))
        } else {
            None
        }
    }))
}

fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owe_markers() {
        assert!(looks_like_owe_language("帮我记下周五交改稿"));
        assert!(!looks_like_owe_language("你好"));
        assert!(!looks_like_owe_language("这段怎么改更好"));
    }
}
