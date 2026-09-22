//! Context compaction: summarise old messages when approaching the model's
//! context limit, keeping recent turns verbatim.

use crate::{CompletionRequest, ContentBlock, LlmProvider, Message, Role, Session};

// 摘要会作为一条 user 消息回到会话里，所以**语言必须跟对话一致**：中文会话
// 收到英文摘要，模型会开始用英文回答，用户看到的是「它怎么忽然换了个人」。
// 前缀不由模型给（`SUMMARY_PREFIX` 由引擎自己加），所以这里要求「只写正文」。
const COMPACTION_SYSTEM: &str = "Summarize the following conversation preserving: key decisions made, tool results and their outcomes, user preferences stated, and any facts the user asked to remember. Write the summary in the **same language as the conversation** (never translate it). Be concise — aim for 1/5 the original length. Output only the summary body — no heading, no prefix.";

/// 压缩摘要在会话里的**固定开头**，由引擎（不是模型）写死。
///
/// 两个用途：① 模型一眼看出「这是更早对话的摘要，不是用户的新要求」；
/// ② 界面据此把这条消息**渲染成说明卡**，而不是一条像用户自己说的话的气泡
/// （`ui/src/utils/displayMessages.ts` 里的 `CONTEXT_SUMMARY_PREFIX` 必须与此一致）。
pub const SUMMARY_PREFIX: &str = "[Context Summary]";

/// Estimate token count for a string.
///
/// We split characters into two buckets:
/// - **CJK** (Han, Hiragana, Katakana, Hangul, full-width punctuation): one
///   character is roughly one token in modern tokenisers.
/// - **Everything else** (ASCII, Latin scripts, code): roughly 4 chars/token.
///
/// The estimate is intentionally conservative on the CJK side (1 char/token
/// rather than the empirical 0.7–0.9) and uses 4 chars/token for ASCII (the
/// classic OpenAI rule of thumb). This stays within ~15% of cl100k_base /
/// claude tokenizers on mixed prose, and over-counts mildly on pure code —
/// which is the right side to err on for compaction triggering.
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk: usize = 0;
    let mut other: usize = 0;
    for c in text.chars() {
        if is_cjk(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + (other as f64 / 4.0).ceil() as usize
}

/// True for code points that tokenise at roughly one-token-per-char.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F |   // CJK symbols & punctuation
        0x3040..=0x309F |   // Hiragana
        0x30A0..=0x30FF |   // Katakana
        0x3400..=0x4DBF |   // CJK Unified Ideographs Extension A
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0xAC00..=0xD7AF |   // Hangul Syllables
        0xF900..=0xFAFF |   // CJK Compatibility Ideographs
        0xFF00..=0xFFEF |   // Halfwidth and Fullwidth Forms
        0x20000..=0x2A6DF | // CJK Extension B
        0x2A700..=0x2B73F | // CJK Extension C
        0x2B740..=0x2B81F   // CJK Extension D
    )
}

/// Estimate total tokens for a session + system prompt + tools.
pub fn estimate_session_tokens(system: &str, session: &Session, tools_json_approx: usize) -> usize {
    let mut total = estimate_tokens(system) + tools_json_approx;
    for msg in &session.messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => total += estimate_tokens(text),
                ContentBlock::Thinking { thinking, .. } => total += estimate_tokens(thinking),
                ContentBlock::ToolUse { input, .. } => {
                    total += estimate_tokens(&serde_json::to_string(input).unwrap_or_default());
                }
                ContentBlock::ToolResult { content, .. } => total += estimate_tokens(content),
                // Base64 image data is sent to the provider; count it so
                // compaction triggers correctly for image-heavy turns.
                ContentBlock::Image { source } => total += estimate_tokens(&source.data),
            }
        }
    }
    total
}

/// Should we compact? Returns true when estimated usage exceeds
/// `model_limit * (1 - headroom)`.
///
/// Private on purpose: 判据只有 [`maybe_compact`] 一个出口，公开它就会招来
/// 第 3、4 份拷贝（`hermes-cli` / `hermes-turn` 原来各有一份，已删）。
fn should_compact(
    system: &str,
    session: &Session,
    tools_json_approx: usize,
    model_limit: usize,
    headroom: f64,
) -> bool {
    if session.messages.len() <= 8 {
        return false;
    }
    let threshold = (model_limit as f64 * (1.0 - headroom)) as usize;
    estimate_session_tokens(system, session, tools_json_approx) > threshold
}

/// 压缩阈值三件套。四个入口（GUI / CLI / agent / server）各自从自己的配置
/// 组装出它，判据本身只有一份。
#[derive(Debug, Clone, Copy)]
pub struct CompactionPolicy {
    pub model_limit: usize,
    pub headroom: f64,
    pub keep_recent_turns: usize,
}

/// 一次压缩的结果：落盘（[`crate::CompactionRecord`]）与用户提示都用它。
#[derive(Debug, Clone)]
pub struct Compacted {
    /// 被摘要取代的消息条数。
    pub replaced: usize,
    /// 取代它们的摘要正文。
    pub summary: String,
    /// 压缩前 / 后的估算 token（用于「省了多少」这类展示与日志）。
    pub before_tokens: usize,
    pub after_tokens: usize,
}

/// **唯一**的压缩入口：判断 + 执行，一次到位。
///
/// - `Ok(None)` = 这次不需要压（绝大多数轮次都是这个，不产生任何调用）。
/// - `Ok(Some(_))` = 已经压完，`session.messages` 已被就地替换；
///   调用方负责把结果落盘，并翻译成自己的用户提示。
/// - `Err(_)` = 摘要调用失败，`session.messages` **未被改动**。
///   调用方默认应当「警告放行」：压不了顶多慢，不该让用户这轮说不出话。
pub async fn maybe_compact(
    provider: &dyn LlmProvider,
    session: &mut Session,
    system: &str,
    tools_json: &str,
    policy: CompactionPolicy,
) -> crate::Result<Option<Compacted>> {
    let tools_approx = estimate_tokens(tools_json);
    if !should_compact(
        system,
        session,
        tools_approx,
        policy.model_limit,
        policy.headroom,
    ) {
        return Ok(None);
    }

    let before_tokens = estimate_session_tokens(system, session, tools_approx);
    let Some((replaced, summary)) =
        compact_session(provider, session, policy.keep_recent_turns).await?
    else {
        return Ok(None);
    };
    let after_tokens = estimate_session_tokens(system, session, tools_approx);

    Ok(Some(Compacted {
        replaced,
        summary,
        before_tokens,
        after_tokens,
    }))
}

/// Compact the session: summarise all messages except the most recent
/// `keep_recent_pairs * 2` messages. Replaces the old messages with a
/// single summary message at position 0.
///
/// Returns `(replaced, summary)` — the number of messages that were folded
/// away and the summary text that replaced them — or `None` when there is
/// nothing to fold. Private: 该不该压、压完怎么落盘，走 [`maybe_compact`]。
async fn compact_session(
    provider: &dyn LlmProvider,
    session: &mut Session,
    keep_recent_pairs: usize,
) -> crate::Result<Option<(usize, String)>> {
    let keep_count = keep_recent_pairs * 2;
    if session.messages.len() <= keep_count {
        return Ok(None);
    }

    let split_at = session.messages.len() - keep_count;
    let old_messages = &session.messages[..split_at];

    let transcript = format_for_summary(old_messages);
    let req = CompletionRequest {
        model: String::new(),
        system: Some(COMPACTION_SYSTEM.to_string()),
        messages: vec![Message::user_text(transcript)],
        tools: Vec::new(),
        max_tokens: 2048,
        temperature: Some(0.1),
        enable_caching: false,
    };

    let resp = provider.complete(req).await?;
    let raw_summary = resp.text();
    // 摘要为空 = 这一次折叠会把这段上下文变成一张**空白卡**（用户实测：133 条消息
    // 被折成 `[Context Summary]` 一行，模型从此看不到那段项目历史，而磁盘上还有
    // 全文——「文件没丢」不等于「它还记得」）。宁可不折：回 Err，调用方按原历史
    // 继续跑这一轮，消息一条不删。
    if summary_is_blank(&raw_summary) {
        return Err(crate::Error::Provider(
            "compaction produced an empty summary; kept the full history instead".into(),
        ));
    }
    let summary_text = compose_summary(&raw_summary);

    let recent: Vec<Message> = session.messages[split_at..].to_vec();
    session.messages.clear();
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: summary_text.clone(),
        }],
        at: None,
        // 引擎写的（不是人说的一轮）——没有说话人。
        speaker: None,
    });
    session.messages.extend(recent);

    Ok(Some((split_at, summary_text)))
}

/// 这条文本是不是「更早的对话被压缩成的摘要」。
///
/// 引擎自己写开头（不是模型），所以可以当作稳定契约来认：界面靠它把这条
/// 用户消息渲染成说明卡而不是气泡，按天分层时它也不算「人说的话」。
pub fn is_summary_text(text: &str) -> bool {
    text.trim_start().starts_with(SUMMARY_PREFIX)
}

/// 给模型写的摘要正文套上引擎自己的开头。落盘（[`crate::CompactionRecord::summary`]）
/// 与回放用的都是这个结果，所以界面永远能靠开头认出它。
fn compose_summary(raw: &str) -> String {
    // 提示词已经说了「不要前缀」，模型偶尔还是会给——这里剥一次，避免出现两个。
    let body = raw.trim();
    let body = body
        .strip_prefix(SUMMARY_PREFIX)
        .map(str::trim_start)
        .unwrap_or(body);
    format!("{SUMMARY_PREFIX}\n{body}")
}

/// 正文里至少要有这么多**非空白**字符，才算一次有效摘要。
///
/// 取 2 是刻意保守：合法摘要不会短到一两个字，而模型偶发的空返回（拒答、只回一个
/// 内容块头、只回 `[Context Summary]`）会被这里挡住。判据只此一处。
const MIN_SUMMARY_CHARS: usize = 2;

/// 摘要正文是否等于「什么都没有」（`[Context Summary]` 前缀不算正文）。
fn summary_is_blank(raw: &str) -> bool {
    let body = raw.trim();
    let body = body.strip_prefix(SUMMARY_PREFIX).unwrap_or(body);
    body.chars().filter(|c| !c.is_whitespace()).count() < MIN_SUMMARY_CHARS
}

/// 发给模型的历史预算（**只影响请求体，不碰落盘**）。
///
/// 一轮干活最多 `max_tool_rounds`（默认 25）次 LLM 往返，而每次往返都把整段历史
/// 再发一遍。只截断每条的字数、仍把几百条都塞进去，条数一涨请求体就线性变慢。
/// 所以更早的消息**整段省略**（一条指路），只保留最近窗口；窗口内的长块再截断。
/// 原文留在会话文件里，模型要用 `session_recall` 翻。
///
/// 窗口内折叠覆盖三种会变大的块：
/// - `ToolResult` 的正文
/// - `ToolUse.input` 里的长字符串
/// - `Text` 的长正文
///
/// **上限不许低于工具自己的承诺**（见 [`MAX_TOOL_RESULT_CHARS`]）。
#[derive(Debug, Clone, Copy)]
pub struct RequestFold {
    /// 请求体只保留最近这么多条。更早的合成一条省略说明。
    pub keep_recent_messages: usize,
    /// 更早的内容截断到这个字符数。
    ///
    /// 现行路径把更早的条**整段省略**，这个字段不再参与请求体；保留是为了
    /// 调用方/测试仍能构造同一份结构，避免再分叉一份 Fold。
    pub older_chars: usize,
    /// 最近窗口内的上限 = [`MAX_TOOL_RESULT_CHARS`]。
    ///
    /// **必须 ≥ 各工具自己承诺的单次返回上限**，且这个前提由
    /// `hermes-tools::TOOL_RESULT_CEILINGS` + 那条跨 crate 的断言保证。
    pub recent_chars: usize,
}

/// 一次工具调用**允许**交给模型的字符数上限。
///
/// 这不是「折叠的上限」，而是**所有会吐正文的工具必须遵守的单次返回上限**；
/// `RequestFold::recent_chars` 就等于它。两侧是**同一根绳子**：
///
/// - **工具侧：** 一次调用最多吐这么多。超了要**自己带继续用的把手**
///   （`read` 给 `offset=`），不许悄悄砍、也不许无声地吐更多；
/// - **折叠侧：** 最近窗口上限 = 它 ⇒ **工具刚给模型的东西，下一轮也不会被砍**。
///
/// 为什么是 64,000：本产品最长的一份**正当单次读取**是「一天的资讯成品」——
/// 2026-09-21 实测 64,005 字节 ≈ **37,428 字**，而吕老师要**整读一次**才有判断依据。
/// 上限低于它，「整读一次」当场变成「读两次」：引擎把 37,428 字砍成 20,000 字发过去，
/// 模型只能再读一次拿剩下那 17,466 字 —— 同样的字节走两趟，还要多一次模型往返。
/// **引擎的省字，反而让总账变贵。** 留 1.7× 余量给更忙的一天，超了由工具分页。
pub const MAX_TOOL_RESULT_CHARS: usize = 64_000;

impl Default for RequestFold {
    fn default() -> Self {
        Self {
            keep_recent_messages: 40,
            older_chars: 1_200,
            // = MAX_TOOL_RESULT_CHARS。改这里必须同时改工具侧的登记表
            // （`hermes-tools::TOOL_RESULT_CEILINGS`），那条跨 crate 的断言会拦住。
            recent_chars: MAX_TOOL_RESULT_CHARS,
        }
    }
}

/// 为**这一次请求**重建消息列表：更早的省略、最近窗口内长块截断。
///
/// - 入参一条都不改（调用方落盘用的仍是原文）。
/// - 被截断的内容带一句指路：原文在会话文件里，用 `session_recall` 取回。
pub fn fold_for_request(messages: &[Message], fold: RequestFold) -> Vec<Message> {
    let keep_from = window_start(messages, fold.keep_recent_messages);
    let mut out = Vec::with_capacity(messages.len().saturating_sub(keep_from) + 1);
    if keep_from > 0 {
        out.push(Message::user_text(format!(
            "[更早的对话共 {keep_from} 条，这里省略了。会话文件里仍是全文。需要前文时用 session_recall 翻旧账。]"
        )));
    }
    for msg in messages.iter().skip(keep_from) {
        let cap = fold.recent_chars;
        let mut cloned = msg.clone();
        for block in &mut cloned.content {
            match block {
                ContentBlock::ToolResult { content, .. } => {
                    if let Some(folded) = fold_text(content, cap, "原始输出") {
                        *content = folded;
                    }
                }
                ContentBlock::ToolUse { input, .. } => fold_tool_input(input, cap),
                ContentBlock::Text { text } => {
                    if let Some(folded) = fold_text(text, cap, "原文") {
                        *text = folded;
                    }
                }
                ContentBlock::Thinking { .. } | ContentBlock::Image { .. } => {}
            }
        }
        out.push(cloned);
    }
    out
}

/// 请求窗口的起点：**不许切在一次工具往返的中间**。
///
/// 窗口按条数算，但「工具结果」那条 user 消息离开它上面那条 assistant 就不成立：
/// 线格式里它会变成一串 `role:"tool"`，而带 `tool_calls` 的消息已在窗口外 ——
/// OpenAI 兼容端点直接 400（DeepSeek：「Messages with role 'tool' must be a
/// response to a preceding message with 'tool_calls'」，2026-09-20 实测）。
///
/// 条数奇偶会被**只在内存里**的引擎催促消息打乱（写文件后的 `[lebi-AI Care]`、
/// 工具预算收尾那条；落盘时被 `for_persist` 过滤掉，所以磁盘上看不出来），
/// 因此不能靠「一般不会切在中间」。起点落在工具结果上就往前挪，把开启它的
/// 那条 assistant 一起带进窗口——配对完整，信息不丢。
fn window_start(messages: &[Message], keep_recent: usize) -> usize {
    let mut start = messages.len().saturating_sub(keep_recent);
    while start > 0 && contains_tool_result(&messages[start]) {
        start -= 1;
    }
    start
}

fn contains_tool_result(m: &Message) -> bool {
    m.content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
}

/// 折叠 `ToolUse.input` 里的长字符串，**只换叶子、不动结构** ——
/// JSON 仍然合法，键还在、模型看得出自己当时调的是什么，只是正文被压短了。
fn fold_tool_input(input: &mut serde_json::Value, cap: usize) {
    match input {
        serde_json::Value::String(s) => {
            if let Some(folded) = fold_text(s, cap, "入参") {
                *s = folded;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                fold_tool_input(item, cap);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                fold_tool_input(v, cap);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// 超过 `cap` 才动手；返回 `None` = 原样保留。
fn fold_text(content: &str, cap: usize, what: &str) -> Option<String> {
    let total = content.chars().count();
    if total <= cap {
        return None;
    }
    let head: String = content.chars().take(cap).collect();
    Some(format!(
        "{head}\n\n[……{what}共 {total} 字，这里只把前 {cap} 字发给你；\
         会话文件里仍是全文。需要原文时用 session_recall 翻旧账，或用 read / grep 重取；\
         如果是网页内容，重新 web_fetch 一次也行。]"
    ))
}

fn format_for_summary(messages: &[Message]) -> String {
    let mut buf = String::new();
    for msg in messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    buf.push_str(&format!("[{role}] {text}\n"));
                }
                ContentBlock::ToolUse { name, .. } => {
                    buf.push_str(&format!("[{role} tool_use] {name}\n"));
                }
                ContentBlock::ToolResult { content, .. } => {
                    let preview = preview_with_head_tail(content, 400, 200);
                    buf.push_str(&format!("[{role} tool_result] {preview}\n"));
                }
                ContentBlock::Thinking { .. } => {}
                ContentBlock::Image { source } => {
                    buf.push_str(&format!("[{role} image: {}]\n", source.media_type));
                }
            }
        }
    }
    buf
}

/// Preview a long string by keeping its first `head_chars` and last
/// `tail_chars`, with a marker in between. Short strings are returned as-is.
fn preview_with_head_tail(s: &str, head_chars: usize, tail_chars: usize) -> String {
    let total: Vec<char> = s.chars().collect();
    if total.len() <= head_chars + tail_chars {
        return s.to_string();
    }
    let head: String = total.iter().take(head_chars).collect();
    let tail: String = total.iter().skip(total.len() - tail_chars).collect();
    let elided = total.len() - head_chars - tail_chars;
    format!("{head}\n…[{elided} chars elided]…\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capabilities, CompletionResponse, StopReason, Usage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tool_use(id: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.into(),
                name: "write".into(),
                input: serde_json::json!({ "path": "outputs/2026-09-20/情报.md" }),
            }],
            at: None,
            speaker: None,
        }
    }

    fn tool_result(id: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.into(),
                content: "ok".into(),
                is_error: false,
            }],
            at: None,
            speaker: None,
        }
    }

    /// 用户开头、此后 assistant/user 交替——真实会话的形状。
    fn alternating_history(pairs: usize) -> Vec<Message> {
        let mut out = vec![Message::user_text("干活")];
        for i in 0..pairs {
            out.push(tool_use(&format!("u{i}")));
            out.push(tool_result(&format!("u{i}")));
        }
        out
    }

    /// 折叠后的第一条**真消息**（省略说明不算）。
    fn first_kept(folded: &[Message]) -> &Message {
        let is_stub = |m: &Message| {
            m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { text } if text.contains("这里省略了")))
        };
        folded.iter().find(|m| !is_stub(m)).expect("窗口不许是空的")
    }

    /// 摘要调用按需返回固定文本；`under_threshold` 用例里它会 panic ——
    /// 不需要压缩时**一次模型调用都不该发生**。
    struct StubProvider {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LlmProvider for StubProvider {
        async fn complete(&self, _req: CompletionRequest) -> crate::Result<CompletionResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: "[Context Summary] 早先聊过的内容".into(),
                }],
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
                truncated_tool_ids: Vec::new(),
            })
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: false,
            }
        }

        fn name(&self) -> &str {
            "stub"
        }
    }

    fn long_session(turns: usize, filler: &str) -> Session {
        let mut messages = Vec::new();
        for i in 0..turns {
            messages.push(Message::user_text(format!("问题 {i} {filler}")));
            messages.push(Message::assistant_text(format!("回答 {i} {filler}")));
        }
        Session {
            meta: crate::SessionMeta::new("test-model", "stub"),
            messages,
            total_input_tokens: 0,
            total_output_tokens: 0,
            flow: Default::default(),
        }
    }

    fn policy(model_limit: usize, keep_recent_turns: usize) -> CompactionPolicy {
        CompactionPolicy {
            model_limit,
            headroom: 0.18,
            keep_recent_turns,
        }
    }

    #[tokio::test]
    async fn short_session_compacts_nothing_and_calls_nothing() {
        let provider = StubProvider {
            calls: AtomicUsize::new(0),
        };
        let mut session = long_session(2, "短");
        let before = session.messages.len();

        let out = maybe_compact(&provider, &mut session, "system", "[]", policy(128_000, 4))
            .await
            .unwrap();

        assert!(out.is_none(), "短会话不该触发压缩");
        assert_eq!(session.messages.len(), before, "消息一条都不该动");
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            0,
            "没到阈值就不该调用模型"
        );
    }

    #[tokio::test]
    async fn over_threshold_folds_prefix_and_reports_numbers() {
        let provider = StubProvider {
            calls: AtomicUsize::new(0),
        };
        let filler = "很长的正文".repeat(200);
        let mut session = long_session(30, &filler);
        let before_len = session.messages.len();

        let out = maybe_compact(&provider, &mut session, "system", "[]", policy(8_000, 2))
            .await
            .unwrap()
            .expect("越过阈值必须压缩");

        assert_eq!(out.replaced, before_len - 4, "保留最近 2 轮 = 4 条");
        assert!(out.summary.starts_with("[Context Summary]"));
        assert_eq!(session.messages.len(), 5, "摘要 + 最近 4 条");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(
            out.after_tokens < out.before_tokens,
            "压缩后必须更小：{} → {}",
            out.before_tokens,
            out.after_tokens
        );
        // 界面靠这个开头把摘要渲染成说明卡 —— 必须是引擎加的那一份，且只有一份。
        let first = session.messages[0].content[0].clone();
        let ContentBlock::Text { text } = first else {
            panic!("摘要必须是文本消息");
        };
        assert!(text.starts_with(SUMMARY_PREFIX));
        assert_eq!(
            text.matches(SUMMARY_PREFIX).count(),
            1,
            "模型自己带了前缀时也不能出现两个：{text}"
        );
    }

    #[test]
    fn compose_summary_strips_a_model_supplied_prefix() {
        assert_eq!(compose_summary(" 正文").trim(), "[Context Summary]\n正文");
        assert_eq!(
            compose_summary("[Context Summary]\n正文"),
            "[Context Summary]\n正文"
        );
    }

    #[test]
    fn token_estimate_ascii() {
        // "hello world" = 11 chars / 4 = 3 tokens.
        assert_eq!(estimate_tokens("hello world"), 3);
    }

    #[test]
    fn token_estimate_cjk() {
        // 5 Han chars → 5 tokens.
        assert_eq!(estimate_tokens("你好世界呀"), 5);
    }

    #[test]
    fn token_estimate_mixed() {
        // "你好 world" = 2 CJK + 6 ASCII ("好 world" includes space)
        // CJK: 2 tokens, ASCII: ceil(6/4)=2, total 4.
        // Actually "你好 world" = chars: '你','好',' ','w','o','r','l','d'
        // 2 CJK + 6 non-CJK = 2 + ceil(6/4)=2 = 4.
        assert_eq!(estimate_tokens("你好 world"), 4);
    }

    #[test]
    fn token_estimate_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn preview_keeps_short_strings_intact() {
        let s = "short string";
        assert_eq!(preview_with_head_tail(s, 400, 200), s);
    }

    #[test]
    fn preview_elides_long_strings() {
        let s = "a".repeat(1000);
        let p = preview_with_head_tail(&s, 400, 200);
        assert!(p.contains("[400 chars elided]"));
        assert!(p.starts_with(&"a".repeat(400)));
        assert!(p.ends_with(&"a".repeat(200)));
    }

    /// 空白摘要（模型偶发空返回）必须**拒绝折叠**：宁可这一轮不压，也不能把
    /// 133 条消息换成一张空白卡（2026-09-18 真实事故）。
    struct BlankSummaryProvider;

    #[async_trait::async_trait]
    impl LlmProvider for BlankSummaryProvider {
        async fn complete(&self, _req: CompletionRequest) -> crate::Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: "[Context Summary]\n".into(),
                }],
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
                truncated_tool_ids: Vec::new(),
            })
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: false,
            }
        }

        fn name(&self) -> &str {
            "blank-summary"
        }
    }

    #[tokio::test]
    async fn a_blank_summary_is_refused_and_the_prefix_survives() {
        let mut session = long_session(12, &"很长的正文".repeat(400));
        let before: Vec<String> = session
            .messages
            .iter()
            .map(|m| format!("{:?}", m.content))
            .collect();

        let out = maybe_compact(
            &BlankSummaryProvider,
            &mut session,
            "system",
            "[]",
            policy(4_000, 4),
        )
        .await;

        assert!(out.is_err(), "空白摘要必须报错，不能折叠");
        let after: Vec<String> = session
            .messages
            .iter()
            .map(|m| format!("{:?}", m.content))
            .collect();
        assert_eq!(before, after, "拒绝折叠时消息一条都不许动");
    }

    #[test]
    fn blank_summary_detection_covers_prefix_only_and_whitespace() {
        assert!(summary_is_blank(""));
        assert!(summary_is_blank("   \n"));
        assert!(summary_is_blank("[Context Summary]"));
        assert!(summary_is_blank("[Context Summary]\n\n  \t"));
        assert!(!summary_is_blank("[Context Summary]\n定下了三条口径"));
        assert!(!summary_is_blank("定下了三条口径"));
    }

    #[test]
    fn older_messages_are_omitted_from_the_request() {
        let long = "字".repeat(3_000);
        let messages = vec![
            Message::user_text("最早的问题"),
            Message::user_text("中间那句"),
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: long.clone(),
                    is_error: false,
                }],
                at: None,
                speaker: None,
            },
        ];
        // 窗口最近 2 条 = 「中间那句」+ 工具结果：起点不落在工具结果上，
        // 省略说明照出现（起点落在工具结果上的那一侧由下面两条用例守）。
        let fold = RequestFold {
            keep_recent_messages: 2,
            older_chars: 10,
            recent_chars: 100,
        };

        let folded = fold_for_request(&messages, fold);

        let original_still_long = matches!(
            &messages[2].content[0],
            ContentBlock::ToolResult { content, .. } if content.chars().count() == 3_000
        );
        assert!(original_still_long, "原消息不许被改写");

        assert_eq!(folded.len(), 3, "省略说明 + 最近两条");
        match &folded[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("更早的对话共 1 条"), "{text}");
                assert!(text.contains("session_recall"), "{text}");
            }
            other => panic!("expected omit stub, got {other:?}"),
        }
        let recent = match &folded[2].content[0] {
            ContentBlock::ToolResult { content, .. } => content.clone(),
            other => panic!("expected tool result, got {other:?}"),
        };
        assert!(recent.starts_with(&"字".repeat(100)));
        assert!(recent.contains("原始输出共 3000 字"), "{recent}");
    }

    /// 2026-09-20 实测事故：窗口按条数切，切在「工具结果」那条上 → 请求体开头是
    /// 一串 `role:"tool"`，带 `tool_calls` 的那条已被切走 → DeepSeek 400
    /// （「Messages with role 'tool' must be a response to a preceding message
    /// with 'tool_calls'」）。
    ///
    /// 起点的奇偶会被**只在内存里**的引擎催促消息打乱（写文件后的 `[lebi-AI Care]`；
    /// 落盘时被 `for_persist` 过滤，所以磁盘上看不出来）——所以这里不押单一长度，
    /// 把「写文件 → 催促 → 继续干活」按不同长度扫一遍。
    #[test]
    fn a_care_nudge_does_not_let_the_window_split_a_tool_round_trip() {
        let base = alternating_history(16);
        for extra_pairs in 0..6 {
            let mut messages = base.clone();
            messages.push(tool_use("w0"));
            messages.push(tool_result("w0"));
            messages.push(Message::user_text(
                crate::companion::care_after_tools_nudge().to_string(),
            ));
            for i in 0..extra_pairs {
                messages.push(tool_use(&format!("p{i}")));
                messages.push(tool_result(&format!("p{i}")));
            }

            let folded = fold_for_request(&messages, RequestFold::default());
            let first = first_kept(&folded);
            assert!(
                !contains_tool_result(first),
                "窗口起点不许落在工具结果上（len={}）：{:?}",
                messages.len(),
                first.content
            );
        }
    }

    /// 配对不许丢：起点落在工具结果上时，把开启它的那条 assistant 一起带进窗口。
    #[test]
    fn the_window_pulls_the_opener_in_with_its_tool_results() {
        let messages = vec![
            tool_use("a0"),
            tool_result("a0"),
            tool_use("a1"),
            tool_result("a1"),
        ];
        let folded = fold_for_request(
            &messages,
            RequestFold {
                keep_recent_messages: 1,
                older_chars: 1_200,
                recent_chars: 20_000,
            },
        );

        assert_eq!(folded.len(), 3, "省略说明 + assistant + 它的工具结果");
        assert!(
            matches!(folded[1].role, Role::Assistant),
            "第二条必须是开启工具的那条 assistant：{:?}",
            folded[1]
        );
        assert!(contains_tool_result(&folded[2]));
        match &folded[0].content[0] {
            ContentBlock::Text { text } => assert!(text.contains("更早的对话共 2 条"), "{text}"),
            other => panic!("expected omit stub, got {other:?}"),
        }
    }

    #[test]
    fn short_tool_outputs_are_left_alone() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: "很短的结果".into(),
                is_error: false,
            }],
            at: None,
            speaker: None,
        }];
        let folded = fold_for_request(&messages, RequestFold::default());
        let same = matches!(
            &folded[0].content[0],
            ContentBlock::ToolResult { content, .. } if content == "很短的结果"
        );
        assert!(same);
    }

    /// P1-1：工具**入参**也是 token 炸弹 —— 模型自己写下去的正文会跟着历史被重发。
    #[test]
    fn long_tool_inputs_are_folded_too() {
        let long = "文".repeat(2_000);
        let messages = vec![
            Message::user_text("最早的问题"),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "u1".into(),
                    name: "write".into(),
                    input: serde_json::json!({"path": "outputs/稿子.md", "content": long}),
                }],
                at: None,
                speaker: None,
            },
            Message::user_text("最近一句"),
        ];
        // 上限取接近真实的量级：**短字符串（路径、参数名）不该被碰**，
        // 只有真正撑爆请求体的长正文才压。
        let fold = RequestFold {
            keep_recent_messages: 999,
            older_chars: 1_000,
            recent_chars: 1_000,
        };
        let folded = fold_for_request(&messages, fold);

        // 原文不动。
        let kept = matches!(
            &messages[1].content[0],
            ContentBlock::ToolUse { input, .. }
                if input["content"].as_str().unwrap().chars().count() == 2_000
        );
        assert!(kept, "原消息不许被改写");

        let (path, content) = match &folded[1].content[0] {
            ContentBlock::ToolUse { input, .. } => (
                input["path"].as_str().unwrap().to_string(),
                input["content"].as_str().unwrap().to_string(),
            ),
            other => panic!("expected tool use, got {other:?}"),
        };
        // 结构没动，只有长字符串被压短。
        assert_eq!(path, "outputs/稿子.md");
        assert!(content.starts_with(&"文".repeat(1_000)));
        assert!(content.contains("入参共 2000 字"), "{content}");
    }

    /// P1-1：用户粘进来的整篇材料也走同一套。
    #[test]
    fn long_text_blocks_are_folded_too() {
        let long = "料".repeat(2_000);
        let messages = vec![
            Message::user_text(long.clone()),
            Message::user_text("最近一句"),
        ];
        let fold = RequestFold {
            keep_recent_messages: 999,
            older_chars: 10,
            recent_chars: 10,
        };
        let folded = fold_for_request(&messages, fold);
        let kept = matches!(
            &messages[0].content[0],
            ContentBlock::Text { text } if text.chars().count() == 2_000
        );
        assert!(kept, "原消息不许被改写");
        match &folded[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.starts_with(&"料".repeat(10)));
                assert!(text.contains("原文共 2000 字"), "{text}");
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    /// **真实尺寸**的回归：一天资讯成品那种体量的一条 `read` 结果，必须**原样**过折叠。
    ///
    /// 2026-09-21 实测：吕老师整读 728 行 / 37,428 字的成品，被折叠砍到前 20,000 字，
    /// 于是它**又读了一次**后半段（17,466 字）—— 同样的字节走两趟 + 多一次模型往返。
    /// 这个用例就是那天那条结果：进了窗口，一个字都不许少。
    #[test]
    fn a_days_digest_sized_tool_result_survives_the_fold_whole() {
        const DIGEST_CHARS: usize = 37_428; // 2026-09-21 成品实测
        let body: String = std::iter::repeat_n('资', DIGEST_CHARS).collect();
        let messages = vec![
            Message::user_text("出这一期。"),
            tool_use("r1"),
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "r1".into(),
                    content: body.clone(),
                    is_error: false,
                }],
                at: None,
                speaker: None,
            },
        ];
        let folded = fold_for_request(&messages, RequestFold::default());
        let kept = folded
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| match b {
                ContentBlock::ToolResult { content, .. } => Some(content),
                _ => None,
            })
            .expect("工具结果必须在窗口里");
        assert_eq!(
            kept.chars().count(),
            DIGEST_CHARS,
            "整读一次的结果被折叠砍了 —— 模型只能再读一遍（2026-09-21 实测过这个坑）"
        );
        assert!(
            !kept.contains("只把前"),
            "不许出现「只把前 N 字发给你」：工具刚给的东西要完整到"
        );
    }

    /// 最近窗口的上限**就是**引擎给「一次工具调用」定的那个数 —— 一个数，一份真相。
    /// 「它盖不盖得住每个工具自己的承诺」由登记表那条绳子管（`hermes-tools::tests`）。
    #[test]
    fn the_recent_cap_is_the_shared_tool_result_ceiling() {
        assert_eq!(
            RequestFold::default().recent_chars,
            MAX_TOOL_RESULT_CHARS,
            "折叠上限必须等于 MAX_TOOL_RESULT_CHARS，否则工具刚给的东西会被砍"
        );
    }
}
