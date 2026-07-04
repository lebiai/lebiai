//! Stream events pushed from server to client over the chat WebSocket.
//!
//! Mirrors `hermes-gui/src/events.rs`, with one improvement: `ToolExecStart`
//! keeps the full `input` JSON (the GUI drops it) so the Flutter tool-call
//! card can render the call's parameters, not just the one-line summary.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum ChatStreamEvent {
    #[serde(rename_all = "camelCase")]
    TextDelta { text: String },
    #[serde(rename_all = "camelCase")]
    ThinkingDelta { text: String },
    #[serde(rename_all = "camelCase")]
    ToolUseStart { id: String, name: String },
    #[serde(rename_all = "camelCase")]
    ToolExecStart {
        id: String,
        name: String,
        summary: String,
        /// Full tool-call arguments as JSON. Present here (unlike the GUI)
        /// so clients can render an expandable "parameters" view.
        input: serde_json::Value,
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
    },
    #[serde(rename_all = "camelCase")]
    UsageUpdate {
        input_tokens: u32,
        output_tokens: u32,
    },
    #[serde(rename_all = "camelCase")]
    Error { message: String },
    #[serde(rename_all = "camelCase")]
    SkillCandidateProposed {
        name: String,
        description: String,
        body: String,
        triggers: Vec<String>,
    },
    Done,
}
