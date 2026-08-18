use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum ChatStreamEvent {
    #[serde(rename_all = "camelCase")]
    TextDelta {
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
    ConfirmRequired {
        id: String,
        tool_name: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
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
    #[serde(rename_all = "camelCase")]
    SkillCandidateProposed {
        name: String,
        description: String,
        body: String,
        triggers: Vec<String>,
    },
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
    Done,
}

// Micro-reflection is **not** a stream event. See `commands/micro.rs` and the
// Tauri event `hermes://micro-reflection` (session-scoped, post-Done).
