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
    MicroReflection {
        summary: String,
        memory_count: u32,
        skill_count: u32,
    },
    #[serde(rename_all = "camelCase")]
    Error { message: String },
    Done,
}
