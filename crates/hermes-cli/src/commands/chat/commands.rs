//! Slash-command dispatch for the chat REPL.
//!
//! `handle_command` is invoked for every line starting with `/`. It returns
//! `false` only for `/exit` / `/quit` to signal the REPL to break out;
//! everything else (including unknown commands) returns `true`.

use std::io::Write;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, Session};
use hermes_llm::ContextLimits;
use hermes_memory::{LoadedMemory, MemoryStore};
use hermes_skills::{FsSkillStore, LoadedSkill};

use crate::commands::context::ContextSources;

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: &str,
    session: &mut Session,
    path: &std::path::Path,
    tools: &[hermes_core::ToolSpec],
    skills: &[LoadedSkill],
    active_memories: &[LoadedMemory],
    base_system: Option<&str>,
    palace_index: Option<&str>,
    always_active_skills: &[&LoadedSkill],
    memory_store: &dyn MemoryStore,
    skill_store: &FsSkillStore,
    provider: &dyn LlmProvider,
    limits: ContextLimits,
) -> bool {
    match cmd.trim() {
        "exit" | "quit" => return false,
        "clear" => {
            let dropped = session.messages.len();
            session.messages.clear();
            eprintln!("(cleared {dropped} in-memory messages — JSONL transcript untouched)");
        }
        "tokens" => {
            eprintln!(
                "tokens: input={} output={} (cumulative)",
                session.total_input_tokens, session.total_output_tokens
            );
        }
        "tools" => {
            if tools.is_empty() {
                eprintln!("(no MCP tools loaded; check ~/.small-rust-hermes/mcp.json)");
            } else {
                for t in tools {
                    eprintln!(
                        "  {} — {}",
                        t.name,
                        t.description.lines().next().unwrap_or("")
                    );
                }
            }
        }
        "memory" | "memories" => {
            if active_memories.is_empty() {
                eprintln!("(no active memories)");
            } else {
                for m in active_memories {
                    let pin = if m.frontmatter.pinned { "★ " } else { "  " };
                    let line = m.body.lines().next().unwrap_or("").trim();
                    eprintln!("{pin}{} [{:?}]: {}", m.frontmatter.id, m.scope, line);
                }
            }
        }
        "skills" => {
            if skills.is_empty() {
                eprintln!("(no skills loaded)");
            } else {
                for s in skills {
                    eprintln!(
                        "  {}: {}",
                        s.frontmatter.name, s.frontmatter.description
                    );
                }
            }
        }
        "context" => {
            let pinned: Vec<_> = active_memories
                .iter()
                .filter(|m| m.frontmatter.pinned)
                .cloned()
                .collect();
            let ctx_profile = hermes_memory::load_profile().unwrap_or(None);
            let sources = ContextSources {
                base: base_system,
                palace_index,
                compiled_profile: ctx_profile.as_deref(),
                always_active_skills,
                pinned: &pinned,
                active: active_memories,
                all_skills: skills,
                effectiveness: None,
                memory_effectiveness: None,
                limits,
            };
            let s = sources.build_session_system();
            if s.is_empty() {
                eprintln!("(empty system prompt — no base, no memory, no skills)");
            } else {
                eprintln!("--- session-level system prompt ---");
                eprintln!("{s}");
                eprintln!("--- end (skills triggered per turn are appended dynamically) ---");
            }
        }
        "session" => {
            eprintln!("path:     {}", path.display());
            eprintln!("messages: {}", session.messages.len());
        }
        s if s.starts_with("remember ") => {
            let text = s.strip_prefix("remember ").unwrap().trim();
            if text.is_empty() {
                eprintln!("usage: /remember <text>");
            } else {
                use hermes_memory::{Confidence, MemoryFrontmatter, Scope as MemScope, Source as MemSource};
                let mut fm = MemoryFrontmatter::new(MemSource::User, Confidence::High, vec![], "core".to_string());
                fm.pinned = true;
                match memory_store.put(MemScope::User, fm, text) {
                    Ok(p) => eprintln!("\x1b[32m✓\x1b[0m remembered: {} ({})", text.chars().take(60).collect::<String>(), p.display()),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
                }
            }
        }
        s if s.starts_with("forget ") => {
            let id_prefix = s.strip_prefix("forget ").unwrap().trim();
            if id_prefix.is_empty() {
                eprintln!("usage: /forget <id-prefix>");
            } else {
                let matches: Vec<_> = active_memories
                    .iter()
                    .filter(|m| m.frontmatter.id.starts_with(id_prefix))
                    .collect();
                match matches.len() {
                    0 => eprintln!("no memory matching \"{id_prefix}\""),
                    1 => {
                        let m = matches[0];
                        eprint!("delete \"{}\"? [y/N] ▸ ", m.body.lines().next().unwrap_or(""));
                        std::io::stderr().flush().ok();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_ok() && input.trim() == "y" {
                            match memory_store.delete(m.scope, &m.frontmatter.id) {
                                Ok(true) => eprintln!("\x1b[32m✓\x1b[0m forgotten"),
                                Ok(false) => eprintln!("\x1b[31m✗\x1b[0m not found on disk"),
                                Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
                            }
                        } else {
                            eprintln!("(cancelled)");
                        }
                    }
                    n => {
                        eprintln!("{n} memories match \"{id_prefix}\":");
                        for m in &matches {
                            eprintln!("  {} — {}", m.frontmatter.id, m.body.lines().next().unwrap_or(""));
                        }
                        eprintln!("use a longer prefix to disambiguate");
                    }
                }
            }
        }
        "reflect" => {
            if session.messages.is_empty() {
                eprintln!("(nothing to reflect on yet — send a message first)");
            } else {
                eprintln!("\x1b[90m(reflecting on session so far...)\x1b[0m");
                if let Err(e) = crate::commands::reflect::run_with_min_turns(provider, session, 0).await {
                    eprintln!("\x1b[31m(reflection failed: {e:#})\x1b[0m");
                }
            }
        }
        "compile" => {
            if active_memories.is_empty() {
                eprintln!("(no memories to compile)");
            } else {
                eprint!("\x1b[90m(compiling memory profile...)\x1b[0m");
                std::io::stderr().flush().ok();
                match hermes_reflect::compile_profile(provider, active_memories).await {
                    Ok(profile) => match hermes_memory::save_profile(&profile) {
                        Ok(p) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[32m✓ profile updated ({})\x1b[0m", p.display());
                        }
                        Err(e) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[31m✗ save failed: {e}\x1b[0m");
                        }
                    },
                    Err(e) => {
                        eprint!("\r\x1b[K");
                        eprintln!("\x1b[31m✗ compile failed: {e}\x1b[0m");
                    }
                }
            }
        }
        s if s.starts_with("skill add") => {
            let rest = s.strip_prefix("skill add").unwrap().trim();
            handle_skill_add(rest, skill_store);
        }
        s if s.starts_with("skill edit ") => {
            let name = s.strip_prefix("skill edit ").unwrap().trim();
            handle_skill_edit(name, skill_store);
        }
        s if s.starts_with("skill show ") => {
            let name = s.strip_prefix("skill show ").unwrap().trim();
            handle_skill_show(name, skill_store);
        }
        "skill" => {
            if skills.is_empty() {
                eprintln!("(no skills loaded)");
            } else {
                for s in skills {
                    eprintln!("  {}: {}", s.frontmatter.name, s.frontmatter.description);
                }
            }
        }
        "palace" => {
            let zones = hermes_memory::group_by_zone(active_memories);
            eprintln!("Memory Palace: {} memories across {} zones", active_memories.len(), zones.len());
            for (zone, mems) in &zones {
                eprintln!("  {zone}: {} memories", mems.len());
            }
            if palace_index.is_some() {
                eprintln!("  index: loaded (in system prompt)");
            } else {
                eprintln!("  index: not loaded");
            }
        }
        "palace compile" => {
            if active_memories.is_empty() {
                eprintln!("(no memories to compile)");
            } else {
                eprint!("\x1b[90m(compiling palace index via LLM...)\x1b[0m");
                std::io::stderr().flush().ok();
                match hermes_reflect::compile_palace_index(provider, active_memories).await {
                    Ok(index) => match hermes_memory::save_palace_index(&index) {
                        Ok(p) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[32m✓ palace index compiled ({})\x1b[0m", p.display());
                            eprintln!("(restart chat to use the new index)");
                        }
                        Err(e) => {
                            eprint!("\r\x1b[K");
                            eprintln!("\x1b[31m✗ save failed: {e}\x1b[0m");
                        }
                    },
                    Err(e) => {
                        eprint!("\r\x1b[K");
                        eprintln!("\x1b[31m✗ compile failed: {e}\x1b[0m");
                    }
                }
            }
        }
        "help" => {
            eprintln!("commands:");
            eprintln!("  /exit, /quit   — leave the chat");
            eprintln!("  /clear         — drop in-memory history (transcript on disk kept)");
            eprintln!("  /tokens        — cumulative input/output token counts");
            eprintln!("  /stats         — detailed session stats: turns, tools, cost estimate");
            eprintln!("  /tools         — list tools loaded from MCP servers");
            eprintln!("  /memory        — list active memories with ids");
            eprintln!("  /skills        — list available skills");
            eprintln!("  /skill add     — interactively add a new skill");
            eprintln!("  /skill edit <name> — edit an existing skill in $EDITOR");
            eprintln!("  /skill show <name> — display full skill body");
            eprintln!("  /context       — show the assembled session-level system prompt");
            eprintln!("  /session       — show transcript path and turn count");
            eprintln!("  /remember <text> — save a memory (pinned, high confidence)");
            eprintln!("  /forget <id>   — delete a memory by id prefix (with confirmation)");
            eprintln!("  /reflect       — trigger on-demand reflection");
            eprintln!("  /compile       — recompile memory profile");
            eprintln!("  /palace        — show Memory Palace zone counts");
            eprintln!("  /palace compile — LLM-compile the palace index");
            eprintln!("  /help          — this list");
        }
        other => eprintln!("unknown command: /{other}  (try /help)"),
    }
    true
}

fn handle_skill_add(rest: &str, skill_store: &FsSkillStore) {
    use hermes_skills::{Scope as SkScope, SkillFrontmatter, SkillStore as _};

    let (name, description) = if rest.is_empty() {
        eprint!("skill name: ");
        std::io::stderr().flush().ok();
        let mut name = String::new();
        if std::io::stdin().read_line(&mut name).is_err() || name.trim().is_empty() {
            eprintln!("(cancelled)");
            return;
        }
        let name = name.trim().to_string();

        eprint!("description: ");
        std::io::stderr().flush().ok();
        let mut desc = String::new();
        if std::io::stdin().read_line(&mut desc).is_err() {
            eprintln!("(cancelled)");
            return;
        }
        (name, desc.trim().to_string())
    } else {
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 || parts[1].trim().is_empty() {
            eprintln!("usage: /skill add <name> <description>");
            return;
        }
        (parts[0].trim().to_string(), parts[1].trim().to_string())
    };

    if name.contains('/') || name.contains("..") || name.contains('\\') {
        eprintln!("\x1b[31m✗\x1b[0m invalid skill name (no path separators or '..')");
        return;
    }

    eprint!("triggers (comma-separated): ");
    std::io::stderr().flush().ok();
    let mut triggers_input = String::new();
    if std::io::stdin().read_line(&mut triggers_input).is_err() {
        eprintln!("(cancelled)");
        return;
    }
    let triggers: Vec<String> = triggers_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let body = match edit_in_editor(&format!(
        "# {}\n\n{}\n",
        name,
        "Describe the skill procedure here in markdown."
    )) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m editor failed: {e}");
            return;
        }
    };
    if body.trim().is_empty() {
        eprintln!("(cancelled — empty body)");
        return;
    }

    let fm = SkillFrontmatter {
        name: name.clone(),
        description,
        triggers,
        version: Some("0.1.0".into()),
        license: None,
        always_active: false,
        extra: serde_yaml::Mapping::new(),
    };
    match skill_store.put(SkScope::User, fm, &body) {
        Ok(p) => eprintln!("\x1b[32m✓\x1b[0m skill \"{name}\" saved: {}", p.display()),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

fn handle_skill_edit(name: &str, skill_store: &FsSkillStore) {
    use hermes_skills::SkillStore as _;

    if name.is_empty() {
        eprintln!("usage: /skill edit <name>");
        return;
    }
    let existing = match skill_store.get(name) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("skill \"{name}\" not found");
            return;
        }
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m {e}");
            return;
        }
    };

    let body = match edit_in_editor(&existing.body) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m editor failed: {e}");
            return;
        }
    };
    if body.trim().is_empty() {
        eprintln!("(cancelled — empty body)");
        return;
    }

    let scope = existing.scope;
    let fm = existing.frontmatter;
    match skill_store.put(scope, fm, &body) {
        Ok(p) => eprintln!("\x1b[32m✓\x1b[0m skill \"{name}\" updated: {}", p.display()),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

fn handle_skill_show(name: &str, skill_store: &FsSkillStore) {
    use hermes_skills::SkillStore as _;

    if name.is_empty() {
        eprintln!("usage: /skill show <name>");
        return;
    }
    match skill_store.get(name) {
        Ok(Some(s)) => {
            eprintln!("name:        {}", s.frontmatter.name);
            eprintln!("description: {}", s.frontmatter.description);
            eprintln!("triggers:    {}", s.frontmatter.triggers.join(", "));
            eprintln!("scope:       {:?}", s.scope);
            eprintln!();
            eprintln!("{}", s.body.trim());
        }
        Ok(None) => eprintln!("skill \"{name}\" not found"),
        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {e}"),
    }
}

/// Open $EDITOR (or vi) with initial content, return the edited content.
fn edit_in_editor(initial_content: &str) -> Result<String> {
    use std::io::Read as _;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let mut tmp = tempfile::Builder::new()
        .prefix("hermes-skill-")
        .suffix(".md")
        .tempfile()
        .context("creating temp file for editor")?;
    write!(tmp, "{initial_content}")?;
    let tmp_path = tmp.path().to_owned();

    let status = std::process::Command::new(&editor)
        .arg(&tmp_path)
        .status()
        .context(format!("running editor '{editor}'"))?;

    if !status.success() {
        anyhow::bail!("editor exited with status {status}");
    }

    let mut content = String::new();
    let mut f = std::fs::File::open(&tmp_path).context("re-opening temp file")?;
    f.read_to_string(&mut content)?;
    Ok(content)
}
