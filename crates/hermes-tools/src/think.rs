//! `think` tool — structured reasoning with no side effects.

use hermes_core::{Result, ToolCallOutcome, ToolSpec};

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "think".into(),
        description: "Use this tool to think through a problem step-by-step before \
            acting. Write out your reasoning, weigh alternatives, or plan a sequence \
            of actions. This tool has no side effects — it simply returns your thought \
            back to you as confirmation. Use it when the next step isn't obvious."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "Your reasoning, analysis, or plan"
                }
            },
            "required": ["thought"]
        }),
    }
}

pub async fn run(args: serde_json::Value) -> Result<ToolCallOutcome> {
    let thought = args
        .get("thought")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Ok(ToolCallOutcome {
        content: format!("Thought recorded ({} chars). Proceed with your plan.", thought.len()),
        is_error: false,
    })
}
