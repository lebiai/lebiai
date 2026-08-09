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
    #[serde(rename_all = "camelCase")]
    SkillCandidateProposed {
        name: String,
        description: String,
        body: String,
        triggers: Vec<String>,
    },
    /// Background micro-reflection (mirrors GUI; never blocks the chat stream).
    #[serde(rename_all = "camelCase")]
    MicroReflection {
        summary: String,
        memory_count: usize,
        skill_count: usize,
        auto_accepted: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        reflection: Option<MicroReflectionPayload>,
    },
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroReflectionPayload {
    pub summary: String,
    pub skill_candidates: Vec<MicroSkillCand>,
    pub memory_candidates: Vec<MicroMemoryCand>,
    pub conflicts: Vec<MicroConflict>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroSkillCand {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
    pub rationale: String,
    pub confidence: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroMemoryCand {
    pub fact: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub confidence: String,
    pub rationale: String,
    pub supersedes: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroConflict {
    pub with: String,
    pub kind: String,
    pub explain: String,
    pub options: Vec<String>,
}

impl MicroReflectionPayload {
    pub fn from_output(output: &hermes_reflect::ReflectionOutput) -> Self {
        Self {
            summary: output.summary.clone(),
            skill_candidates: output
                .skill_candidates
                .iter()
                .map(|c| MicroSkillCand {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    triggers: c.triggers.clone(),
                    body: c.body.clone(),
                    rationale: c.rationale.clone(),
                    confidence: format!("{:?}", c.confidence),
                })
                .collect(),
            memory_candidates: output
                .memory_candidates
                .iter()
                .map(|c| MicroMemoryCand {
                    fact: c.fact.clone(),
                    tags: c.tags.clone(),
                    scope: format!("{:?}", c.scope),
                    confidence: format!("{:?}", c.confidence),
                    rationale: c.rationale.clone(),
                    supersedes: c.supersedes.clone(),
                })
                .collect(),
            conflicts: output
                .conflicts
                .iter()
                .map(|c| MicroConflict {
                    with: c.with.clone(),
                    kind: c.kind.clone(),
                    explain: c.explain.clone(),
                    options: c.options.clone(),
                })
                .collect(),
        }
    }
}
