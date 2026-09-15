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
use crate::commands::style;

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: &str,
    session: &mut Session,
    path: &std::path::Path,
    tools: &[hermes_core::ToolSpec],
    skills: &[LoadedSkill],
    persona: Option<&hermes_core::persona::Persona>,
    // `all_active` = 全量 active，只有 `/compile` 该用它：它再按 `profile_input`
    // 取出全局可见的一份去编译（`profile.md` 是单个全局文件，只装全局口径）。
    all_active: &[LoadedMemory],
    // `active_view` / `pinned_view` = 本会话可见的那一份 = 真正注入模型的那一份
    // （`visible_to` 的 owned 切片）。「他记得什么」的每个显示面都必须吃这一份，
    // 否则显示与注入是两份事实。
    active_view: &[LoadedMemory],
    pinned_view: &[LoadedMemory],
    base_system: Option<&str>,
    topic_cards: Option<&str>,
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
                eprintln!("(no MCP tools loaded; check ~/.lebi-ai/mcp.json)");
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
            if active_view.is_empty() {
                eprintln!("(no active memories)");
            } else {
                for m in active_view {
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
                    eprintln!("  {}: {}", s.frontmatter.name, s.frontmatter.description);
                }
            }
        }
        "context" => {
            let ctx_profile = hermes_memory::load_profile().unwrap_or(None);
            let roster = hermes_core::persona::open();
            let sources = ContextSources {
                base: base_system,
                persona,
                roster: &roster,
                topic_cards,
                compiled_profile: ctx_profile.as_deref(),
                always_active_skills,
                pinned: pinned_view,
                active: active_view,
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
                use hermes_memory::{
                    Confidence, MemoryFrontmatter, Scope as MemScope, Source as MemSource,
                };
                let mut fm = MemoryFrontmatter::new(
                    MemSource::User,
                    Confidence::High,
                    vec![],
                    hermes_core::companion::zones::PREFERENCES.to_string(),
                );
                fm.pinned = true;
                match memory_store.put(MemScope::User, fm, text) {
                    Ok(p) => eprintln!(
                        "{} remembered: {} ({})",
                        style::ok_mark(),
                        text.chars().take(60).collect::<String>(),
                        p.display()
                    ),
                    Err(e) => eprintln!("{} {e}", style::err_mark()),
                }
            }
        }
        s if s.starts_with("forget ") => {
            let id_prefix = s.strip_prefix("forget ").unwrap().trim();
            if id_prefix.is_empty() {
                eprintln!("usage: /forget <id-prefix>");
            } else {
                let matches: Vec<_> = active_view
                    .iter()
                    .filter(|m| m.frontmatter.id.starts_with(id_prefix))
                    .collect();
                match matches.len() {
                    0 => eprintln!("no memory matching \"{id_prefix}\""),
                    1 => {
                        let m = matches[0];
                        eprint!(
                            "delete \"{}\"? [y/N] ▸ ",
                            m.body.lines().next().unwrap_or("")
                        );
                        std::io::stderr().flush().ok();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_ok() && input.trim() == "y" {
                            match memory_store.delete(m.scope, &m.frontmatter.id) {
                                Ok(true) => eprintln!("{} forgotten", style::ok_mark()),
                                Ok(false) => eprintln!("{} not found on disk", style::err_mark()),
                                Err(e) => eprintln!("{} {e}", style::err_mark()),
                            }
                        } else {
                            eprintln!("(cancelled)");
                        }
                    }
                    n => {
                        eprintln!("{n} memories match \"{id_prefix}\":");
                        for m in &matches {
                            eprintln!(
                                "  {} — {}",
                                m.frontmatter.id,
                                m.body.lines().next().unwrap_or("")
                            );
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
                eprintln!("{}", style::dim("(reflecting on session so far...)"));
                if let Err(e) =
                    crate::commands::reflect::run_with_min_turns(provider, session, 0).await
                {
                    eprintln!("{}", style::red(&format!("(reflection failed: {e:#})")));
                }
            }
        }
        "compile" => {
            // 入参是全量 `all_active`，但**喂给编译器的是全局可见的那一份**：`profile.md`
            // 是单个全局文件、被每个视图无条件注入系统提示词，所以它只能装全局口径
            // （1.10e）。人物私有的专业口径不进 profile——不是「保持全量」，而是「只装
            // 全局可见」；它仍在记忆索引与主题卡里。守卫与编译喂的是同一份。
            let profile_memories = hermes_reflect::profile_input(all_active);
            if profile_memories.is_empty() {
                eprintln!("(no globally-visible memories to compile)");
            } else {
                eprint!("{}", style::dim("(compiling memory profile...)"));
                std::io::stderr().flush().ok();
                match hermes_reflect::compile_profile(provider, &profile_memories).await {
                    Ok(profile) => match hermes_memory::save_profile(&profile) {
                        Ok(p) => {
                            eprint!("\r\x1b[K");
                            eprintln!(
                                "{}",
                                style::green(&format!("✓ profile updated ({})", p.display()))
                            );
                        }
                        Err(e) => {
                            eprint!("\r\x1b[K");
                            eprintln!("{}", style::red(&format!("✗ save failed: {e}")));
                        }
                    },
                    Err(e) => {
                        eprint!("\r\x1b[K");
                        eprintln!("{}", style::red(&format!("✗ compile failed: {e}")));
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
            let zones = hermes_memory::group_by_zone(active_view);
            eprintln!(
                "Memory Palace: {} memories across {} zones",
                active_view.len(),
                zones.len()
            );
            for (zone, mems) in &zones {
                eprintln!("  {zone}: {} memories", mems.len());
            }
            if topic_cards.is_some() {
                eprintln!("  index: topic cards loaded (in system prompt)");
            } else {
                eprintln!("  index: no topic cards yet (`hermes topics --build`)");
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
        eprintln!(
            "{} invalid skill name (no path separators or '..')",
            style::err_mark()
        );
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
        name, "Describe the skill procedure here in markdown."
    )) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{} editor failed: {e}", style::err_mark());
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
        Ok(p) => eprintln!(
            "{} skill \"{name}\" saved: {}",
            style::ok_mark(),
            p.display()
        ),
        Err(e) => eprintln!("{} {e}", style::err_mark()),
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
            eprintln!("{} {e}", style::err_mark());
            return;
        }
    };

    let body = match edit_in_editor(&existing.body) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{} editor failed: {e}", style::err_mark());
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
        Ok(p) => eprintln!(
            "{} skill \"{name}\" updated: {}",
            style::ok_mark(),
            p.display()
        ),
        Err(e) => eprintln!("{} {e}", style::err_mark()),
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
        Err(e) => eprintln!("{} {e}", style::err_mark()),
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
