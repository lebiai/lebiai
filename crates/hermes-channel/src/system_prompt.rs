//! Session-level system prompt + per-turn time-header injection.
//!
//! Shared by CLI chat and IM channels (hermes-channel); kept cache-stable
//! so Anthropic prompt caching stays warm across turns.
//!
//! The cache-stable session system prompt MUST NOT contain time-varying
//! data — Anthropic prompt caching needs the prefix to be byte-identical
//! across turns. Time gets injected per-turn into the user message instead,
//! so the system prefix stays warm.

use hermes_core::companion;

/// Build the session system prompt: companion identity + workspace + tool strategy.
///
/// **Cache-stable**: this prompt MUST NOT contain time-varying data
/// (timestamps, session counters, etc.) — Anthropic prompt caching needs
/// the prefix to be byte-identical across turns. Per-turn dynamic context
/// (current time, palace index, matched skills) is injected separately.
pub fn compose_system_prompt(
    user_system: Option<String>,
    workspace_root: &std::path::Path,
) -> Option<String> {
    let clause = format!(
        "{protocol}\n\
         ## Workspace\n\
         Your working directory is `{ws}`. All file reads, writes, and \
         commands MUST stay inside this directory. If a task seems to \
         require touching anything outside, stop and ask the user before \
         proceeding.\n\n\
         ## Tool Strategy (how you Do engineering work)\n\
         Explore before acting: grep/glob to locate, read to understand, \
         then edit/write to change. Verify with bash (tests, build).\n\
         - Use grep to find symbols, glob to find files by pattern.\n\
         - Use read to inspect specific lines before editing.\n\
         - Use edit for surgical changes; write only for new files.\n\
         - Use bash for builds, tests, and shell commands.\n\
         - Use git for read-only repo inspection (status, diff, log, blame).\n\
         - Use think to reason through complex multi-step plans.\n\
         - For multi-step tasks (~3+ steps): call todo_write first to lay out the plan, \
         keep exactly one task in_progress, and mark tasks completed as you finish.\n\
         Minimize tool calls: batch related reads, avoid re-reading unchanged files.\n\n\
         ## Code Analysis Workflow\n\
         1. grep/glob → locate relevant files and symbols\n\
         2. read → understand context around the target code\n\
         3. Trace callers/callees if the change has side effects\n\
         4. edit → make the minimal change\n\
         5. bash → run tests or build to verify\n\n\
         ## Memory Palace\n\
         You have a Memory Palace with zone-organized memories. The palace \
         index (zone map) is in your system prompt when available.\n\
         - Use palace_zones to list zones\n\
         - Use palace_read_zone to load a zone's content\n\
         - Use palace_recall / memory_search to find past work, standards, preferences\n\
         - Prefer zones: `work` (episodes), `preferences`, `standards` / `core`, `general`\n\
         - Use memory_save (with zone + tags) to persist new learnings\n\
         - Use memory_delete to remove outdated memories\n\
         When the user starts work similar to a past episode, **search first**, then use Continuity.\n\
         Don't guess about preferences or conventions — load the relevant zone first.\n\n\
         ## Output Style\n\
         Be concise. Show file paths with line numbers when referencing code. \
         End task responses with: Changed / Verified / Not verified / Risks when useful.\n\
         After a complete deliverable (any work domain), optional Care: at most 1–3 concrete \
         improvements fit to the user — skip on final-only / 定稿.\n\n\
         ## Web\n\
         When answering time-sensitive questions, use web_search first.\n\
         - Prefer snippets from search results — only web_fetch if you need the full page.\n\
         - Only fetch URLs that appeared in search results or that the user gave you. \
         Never guess or construct URLs.\n\
         - If web_fetch returns 403/404 or very little text, switch to a different source. \
         Do not retry the same site.\n\
         - For weather, news, or real-time data: search first, use the snippet, \
         move on.",
        protocol = companion::companion_protocol(),
        ws = workspace_root.display()
    );
    Some(match user_system {
        Some(extra) if !extra.is_empty() => format!("{clause}\n\n{extra}"),
        _ => clause,
    })
}

/// One-line "## Context" header with the current wall-clock time, formatted
/// for prepending to the per-turn user message. Kept out of the cacheable
/// system prompt so Anthropic prompt caching can hit on every turn.
pub fn current_time_header() -> String {
    let now = chrono::Local::now();
    format!(
        "[Context: current time {}]",
        now.format("%Y-%m-%d %H:%M (%A)")
    )
}

/// Prepend `current_time_header()` to the last user message in `history`.
/// Operates on a local clone so the persisted session log stays clean.
pub fn inject_time_header(mut history: Vec<hermes_core::Message>) -> Vec<hermes_core::Message> {
    let header = current_time_header();
    for msg in history.iter_mut().rev() {
        if matches!(msg.role, hermes_core::Role::User) {
            let mut new_content = Vec::with_capacity(msg.content.len() + 1);
            new_content.push(hermes_core::ContentBlock::Text { text: header });
            new_content.append(&mut msg.content);
            msg.content = new_content;
            break;
        }
    }
    history
}
