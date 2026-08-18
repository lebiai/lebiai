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

/// Which surface this session prompt is for. Identity and tool story must match
/// what the surface can actually do (P0 §0 / §3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptKind {
    /// Desktop / CLI dialogue — work companion, generic Do.
    Dialogue,
    /// IM — companion, but no durable writes and no shell.
    Im,
    /// Engine batch (`hermes run`) — not the companion product.
    Batch,
}

/// Build the session system prompt.
///
/// **Cache-stable**: this prompt MUST NOT contain time-varying data
/// (timestamps, session counters, etc.).
pub fn compose_system_prompt(
    user_system: Option<String>,
    workspace_root: &std::path::Path,
    kind: PromptKind,
) -> Option<String> {
    let ws = workspace_root.display();
    let clause = match kind {
        PromptKind::Batch => format!(
            "You are a **batch worker** for the lebi-AI local engine. \
             You are NOT the work companion product (搭子). Do not claim to remember \
             the user or to speak as lebi-AI the companion.\n\n\
             ## Workspace\n\
             Working directory: `{ws}`. Stay inside it.\n\n\
             Complete the stated goal. If the goal is ambiguous or conflicts with \
             an explicit constraint, stop and state the tension instead of inventing \
             a plan. Prefer the smallest correct action. Report real paths.\n\
             Code is only relevant if the goal is about code.\n"
        ),
        PromptKind::Im => format!(
            "{protocol}\n\
             ## Workspace\n\
             Notes may mention `{ws}`. On this channel you cannot write files, \
             run a shell, or save lasting memories/skills. Do not claim you did.\n\n\
             ## What you can do here\n\
             - think, web_search, web_fetch (only URLs from search or the user)\n\
             - memory_search / palace_zones / palace_read_zone / palace_recall — \
               use these before pretending you remember\n\
             - skill_list / skill_read / skill_read_file — load a skill body before acting on it\n\n\
             If the user needs a file written, a command run, or something remembered \
             for next time, say that belongs on the desktop app — do not invent a save.\n\n\
             ## Output\n\
             Be concise. After a complete deliverable, optional Care: at most 1–3 \
             concrete improvements; skip on 定稿 / final-only.\n",
            protocol = companion::companion_protocol(),
        ),
        PromptKind::Dialogue => format!(
            "{protocol}\n\
             ## Workspace\n\
             Your working directory is `{ws}`. File reads, writes, and commands \
             stay inside it unless the user names an explicit path. New deliverables \
             the user did not path: write under `outputs/`.\n\n\
             ## How you Do work (any domain)\n\
             1. Understand the ask. If notes or the palace may apply, search first \
                (`memory_search` / palace tools). Do not pretend you remember.\n\
             2. Act with the smallest correct tools: read existing material, write \
                or edit a deliverable, search the web when facts must be current.\n\
             3. Verify before declaring done. Report real paths and results.\n\
             4. Code, git, and bash are available when the *task* is engineering — \
                they are not your default stance. Do not start with grep/glob unless \
                the user is asking about files or code.\n\
             5. Multi-step (~3+): todo_write, one task in_progress, mark completed.\n\
             6. Named Desktop/Documents/Downloads: one write to that path. No probing.\n\
             7. Open a file, folder, video, or page with the `open` tool — never bash `open` / osascript / guessing Word, Pages, or WPS.\n\
             8. Speak results, not lab notes (no sandbox / Finder / path-test narration).\n\n\
             ## Memory\n\
             Prefer zones `preferences` / `standards` / `work` (work-episode). \
             Use memory_save only for durable preferences, standards, or episodes \
             the user wants kept. Do not spam writes.\n\n\
             ## Output\n\
             Be concise. After a complete deliverable, optional Care: at most 1–3 \
             concrete improvements fit to them — skip on 定稿 / final-only.\n\
             Time-sensitive facts: web_search first; fetch only URLs from results \
             or the user.\n",
            protocol = companion::companion_protocol(),
        ),
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dialogue_is_not_a_coding_playbook() {
        let p = compose_system_prompt(None, Path::new("/tmp/ws"), PromptKind::Dialogue).unwrap();
        assert!(p.contains("work companion") || p.contains("搭子"));
        assert!(!p.contains("Code Analysis Workflow"));
        assert!(!p.contains("how you Do engineering work"));
        assert!(p.contains("No probing"));
        assert!(p.contains("lab notes") || p.contains("Never narrate"));
        assert!(p.contains("`open` tool"));
    }

    #[test]
    fn im_does_not_promise_durable_writes() {
        let p = compose_system_prompt(None, Path::new("/tmp/ws"), PromptKind::Im).unwrap();
        assert!(p.contains("cannot write files"));
        assert!(!p.contains("Use memory_save"));
        assert!(!p.contains("`open` tool"));
    }

    #[test]
    fn batch_is_not_the_companion() {
        let p = compose_system_prompt(None, Path::new("/tmp/ws"), PromptKind::Batch).unwrap();
        assert!(p.contains("batch worker"));
        assert!(!p.contains("You are lebi-AI"));
    }
}
