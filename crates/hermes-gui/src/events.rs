use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum ChatStreamEvent {
    #[serde(rename_all = "camelCase")]
    TextDelta {
        text: String,
    },
    /// Replace the streamed assistant text after citation sanitizing.
    #[serde(rename_all = "camelCase")]
    TextCorrected {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    ThinkingDelta {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolUseStart {
        id: String,
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolExecStart {
        id: String,
        name: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolUseResult {
        id: String,
        content: String,
        is_error: bool,
    },
    #[serde(rename_all = "camelCase")]
    UsageUpdate {
        input_tokens: u32,
        output_tokens: u32,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        message: String,
    },
    /// User pressed Stop — generation interrupted cleanly.
    Cancelled,
    /// Open-work tool finished — sidebar refresh + in-chat cue.
    #[serde(rename_all = "camelCase")]
    ZaibanUpdated {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        existing_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        existing_title: Option<String>,
    },
    /// User said remember this standard — waiting in 它记得的.
    RememberQueued,
    /// 较早的上下文已被整理成摘要（本轮发送时触发）。UI 用一行安静的文字说明，
    /// 不是错误、不需要用户做任何事。
    #[serde(rename_all = "camelCase")]
    ContextCompacted {
        replaced: usize,
        before_tokens: usize,
        after_tokens: usize,
    },
    Done,
}

// Micro-reflection is **not** a stream event. See `commands/micro.rs` and the
// Tauri event `hermes://micro-reflection` (session-scoped, post-Done).
