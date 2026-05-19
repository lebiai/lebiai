//! `hermes` CLI entry point.

mod commands;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "hermes",
    version,
    about = "Self-evolving agent (small-rust-hermes)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// One-shot prompt: send a single user message, print assistant reply.
    Ask {
        prompt: String,
        #[arg(long)]
        system: Option<String>,
    },
    /// Autonomous agent: receive a goal, iterate until complete.
    Run {
        goal: String,
        #[arg(long)]
        system: Option<String>,
        #[arg(long)]
        max_iterations: Option<usize>,
    },
    /// Interactive multi-turn REPL with JSONL session persistence.
    Chat {
        #[arg(long)]
        system: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Resume a previous session by JSONL path.
        #[arg(long, value_name = "PATH", conflicts_with = "resume_last")]
        resume: Option<std::path::PathBuf>,
        /// Resume the most recent session under ~/.small-rust-hermes/sessions/.
        #[arg(long, conflicts_with = "resume")]
        resume_last: bool,
    },

    /// Inspect configured MCP servers.
    #[command(subcommand)]
    Mcp(McpCmd),

    /// Inspect / manage skills.
    #[command(subcommand)]
    Skills(SkillsCmd),

    /// Inspect / manage memories.
    #[command(subcommand)]
    Memory(MemoryCmd),

    /// Browse session transcripts.
    #[command(subcommand)]
    Session(SessionCmd),

    /// Show reflection-acceptance statistics (meta-reflection signal).
    ReflectStats {
        /// Limit to the most recent N entries.
        #[arg(long, default_value_t = 50)]
        last: usize,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// List recent sessions, newest first.
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print a one-line summary of every turn in a session.
    Show {
        path: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum McpCmd {
    /// List MCP servers from mcp.json.
    List,
    /// Connect to one (or all) and list advertised tools.
    Test {
        server: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SkillsCmd {
    /// List skills.
    List {
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,
    },
    /// Print full body of one skill.
    Show { name: String },
    /// Remove a skill directory.
    Delete {
        name: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
}

#[derive(Subcommand, Debug)]
enum MemoryCmd {
    /// List active memories (default), `--all` or `--pinned` for variants.
    List {
        #[arg(long, conflicts_with = "pinned")]
        all: bool,
        #[arg(long, conflicts_with = "all")]
        pinned: bool,
    },
    /// Print full body of one memory.
    Show { id: String },
    /// Remove a memory file.
    Delete {
        id: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
    /// Mark `pinned: true` (always loaded into system prompt).
    Pin { id: String },
    /// Mark `pinned: false`.
    Unpin { id: String },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ScopeArg {
    User,
    Project,
}

impl From<ScopeArg> for hermes_skills::Scope {
    fn from(a: ScopeArg) -> Self {
        match a {
            ScopeArg::User => Self::User,
            ScopeArg::Project => Self::Project,
        }
    }
}

impl From<ScopeArg> for hermes_memory::Scope {
    fn from(a: ScopeArg) -> Self {
        match a {
            ScopeArg::User => Self::User,
            ScopeArg::Project => Self::Project,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Command::Ask { prompt, system } => commands::ask::run(prompt, system).await,
        Command::Run {
            goal,
            system,
            max_iterations,
        } => commands::agent::run(goal, system, max_iterations).await,
        Command::Chat {
            system,
            model,
            resume,
            resume_last,
        } => {
            let resume_path = resolve_resume(resume, resume_last)?;
            commands::chat::run(system, model, resume_path).await
        }
        Command::Mcp(sub) => match sub {
            McpCmd::List => commands::mcp::list().await,
            McpCmd::Test { server } => commands::mcp::test(server).await,
        },
        Command::Skills(sub) => match sub {
            SkillsCmd::List { scope } => commands::skills::list(scope.map(Into::into)),
            SkillsCmd::Show { name } => commands::skills::show(&name),
            SkillsCmd::Delete { name, scope } => commands::skills::delete(&name, scope.into()),
        },
        Command::Memory(sub) => match sub {
            MemoryCmd::List { all, pinned } => {
                let filter = if pinned {
                    commands::memory::Filter::Pinned
                } else if all {
                    commands::memory::Filter::All
                } else {
                    commands::memory::Filter::Active
                };
                commands::memory::list(filter)
            }
            MemoryCmd::Show { id } => commands::memory::show(&id),
            MemoryCmd::Delete { id, scope } => commands::memory::delete(&id, scope.into()),
            MemoryCmd::Pin { id } => commands::memory::set_pinned(&id, true),
            MemoryCmd::Unpin { id } => commands::memory::set_pinned(&id, false),
        },
        Command::Session(sub) => match sub {
            SessionCmd::List { limit } => commands::session::list(limit),
            SessionCmd::Show { path } => commands::session::show(&path),
        },
        Command::ReflectStats { last } => commands::reflect_stats::run(last),
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_env("HERMES_LOG").unwrap_or_else(|_| EnvFilter::new("warn,hermes_cli=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// Resolve `--resume <path>` / `--resume-last`. Returns `Ok(None)` if
/// neither flag is set.
fn resolve_resume(explicit: Option<PathBuf>, last: bool) -> Result<Option<PathBuf>> {
    if let Some(p) = explicit {
        if !p.exists() {
            return Err(anyhow!("session file not found: {}", p.display()));
        }
        return Ok(Some(p));
    }
    if last {
        let dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("could not resolve $HOME"))?
            .join(".small-rust-hermes")
            .join("sessions");
        let mut sessions = hermes_store::list_sessions(&dir)
            .map_err(|e| anyhow!("listing sessions: {e}"))?;
        if let Some(p) = sessions.drain(..).next() {
            return Ok(Some(p));
        }
        return Err(anyhow!("no sessions found in {}", dir.display()));
    }
    Ok(None)
}
