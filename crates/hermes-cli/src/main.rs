//! `hermes` CLI entry point (product: lebi-AI / 乐彼AI).

mod commands;

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "hermes",
    version,
    about = "lebi-AI — your local work companion"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Interactive first-run setup: choose a provider, enter your API key,
    /// and write ~/.lebi-ai/config.toml.
    Init,

    /// Check configuration & environment health (makes no changes).
    Doctor,

    /// One-shot prompt (engine mode, not the companion). No tools unless `--tools`.
    Ask {
        prompt: String,
        #[arg(long)]
        system: Option<String>,
        /// Enable built-in tools. Dangerous calls still need `--auto-allow`.
        #[arg(long)]
        tools: bool,
        /// Auto-approve tool confirms (including high-risk). Off by default.
        #[arg(long)]
        auto_allow: bool,
    },
    /// Engine batch: iterate a goal until done (not the companion product).
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
        /// Resume the most recent session under ~/.lebi-ai/sessions/.
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

    /// Distill the memory store: cluster near-duplicate memories across
    /// sessions and merge each cluster into one survivor. The first
    /// mechanism that makes the knowledge base *converge*, not just grow.
    Distill {
        /// Actually write changes. Without this, `hermes distill` is a
        /// read-only dry-run that only prints the proposed clusters.
        #[arg(long, default_value_t = false)]
        apply: bool,
        /// For each accepted cluster, call the LLM once to fuse the members
        /// into a single denser statement. Implies `--apply`. Cost is
        /// bounded to clusters the user accepts.
        #[arg(long, default_value_t = false)]
        llm_merge: bool,
        /// TF-IDF cosine-similarity threshold for "near-duplicate".
        /// Genuine rewordings cluster around 0.55–0.65; unrelated facts
        /// score below ~0.1.
        #[arg(long, default_value_t = hermes_memory::distill::DEFAULT_THRESHOLD)]
        threshold: f64,
    },

    /// WeChat (iLink Bot) bridge: scan QR in the terminal, chat with the model
    /// from WeChat.
    #[command(subcommand)]
    Wechat(WechatCmd),

    /// Feishu (Lark) bridge: connect via WS long-connection, chat with the model
    /// from Feishu.
    #[command(subcommand)]
    Feishu(FeishuCmd),

    /// Telegram bridge: connect via Bot API long-poll, chat with the model
    /// from Telegram.
    #[command(subcommand)]
    Telegram(TelegramCmd),

    /// Start the HTTP/WebSocket server (Flutter / mobile client backend).
    Serve {
        /// Port to listen on.
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Address to bind. Default `127.0.0.1` (localhost only). Use
        /// `0.0.0.0` to expose on the LAN/internet — the auth token is
        /// always required either way.
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        /// Auth token (overrides --token-file / `HERMES_SERVER_TOKEN` /
        /// the saved file). The server prints it on startup if generated.
        #[arg(long)]
        token: Option<String>,
        /// Read the auth token from this file (one line, trimmed).
        #[arg(long, value_name = "FILE")]
        token_file: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum WechatCmd {
    /// Render a terminal QR; scan it in WeChat to authorize this machine.
    /// Persists `bot_token` to `~/.lebi-ai/wechat.toml` (mode 600).
    Login,
    /// Long-poll for incoming WeChat messages and reply with the lebi-AI model.
    Run,
}

#[derive(Subcommand, Debug)]
enum FeishuCmd {
    /// Validate app_id/app_secret and persist them to
    /// `~/.lebi-ai/feishu.toml` (mode 600).
    Auth,
    /// Connect to Feishu via WS long-connection and reply to messages.
    Run,
}

#[derive(Subcommand, Debug)]
enum TelegramCmd {
    /// Validate the bot token (from @BotFather) and persist it to
    /// `~/.lebi-ai/telegram.toml` (mode 600).
    Auth,
    /// Long-poll for incoming Telegram messages and reply with the lebi-AI model.
    Run,
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// List recent sessions, newest first.
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print a one-line summary of every turn in a session.
    Show { path: std::path::PathBuf },
}

#[derive(Subcommand, Debug)]
enum McpCmd {
    /// List MCP servers from mcp.json.
    List,
    /// Connect to one (or all) and list advertised tools.
    Test { server: Option<String> },
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
    /// Install a skill from `owner/repo@slug` (full directory) or a raw
    /// https:// URL to a SKILL.md (single file, no siblings).
    Install {
        source: String,
        #[arg(long)]
        overwrite: bool,
        /// Optional branch / tag / commit SHA (defaults to `main`).
        /// Ignored for raw-URL installs.
        #[arg(long, value_name = "REF")]
        git_ref: Option<String>,
    },
    /// Remove a skill directory. Bundled meta-skills (memory-palace,
    /// skill-creator, find-skills) are refused — they reinstall at
    /// launch and the delete would be a no-op.
    Delete { name: String },
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
    if hermes_core::maybe_migrate_data_root() {
        tracing::info!("migrated legacy data directory to ~/.lebi-ai");
    }
    let cli = Cli::parse();
    match cli.command {
        Command::Init => commands::init::run().await,
        Command::Doctor => commands::doctor::run().await,
        Command::Ask {
            prompt,
            system,
            tools,
            auto_allow,
        } => commands::ask::run(prompt, system, tools, auto_allow).await,
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
            SkillsCmd::Install {
                source,
                overwrite,
                git_ref,
            } => commands::skills::install(&source, overwrite, git_ref.as_deref()).await,
            SkillsCmd::Delete { name } => commands::skills::delete(&name),
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
        Command::Distill {
            apply,
            llm_merge,
            threshold,
        } => {
            commands::distill::run(&commands::distill::DistillOpts {
                apply,
                llm_merge,
                threshold,
            })
            .await
        }
        Command::Wechat(sub) => match sub {
            WechatCmd::Login => commands::wechat::login().await,
            WechatCmd::Run => commands::wechat::run().await,
        },
        Command::Feishu(sub) => match sub {
            FeishuCmd::Auth => commands::feishu::auth().await,
            FeishuCmd::Run => commands::feishu::run().await,
        },
        Command::Telegram(sub) => match sub {
            TelegramCmd::Auth => commands::telegram::auth().await,
            TelegramCmd::Run => commands::telegram::run().await,
        },
        Command::Serve {
            port,
            host,
            token,
            token_file,
        } => {
            let token = Arc::new(hermes_server::auth::resolve_token(
                &hermes_server::auth::TokenOpts {
                    value: token,
                    file: token_file,
                },
            )?);
            commands::serve::run(host, port, token).await
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("HERMES_LOG")
        .unwrap_or_else(|_| EnvFilter::new("warn,hermes_cli=info"));
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
        let dir = hermes_core::data_path("sessions");
        let mut sessions =
            hermes_store::list_sessions(&dir).map_err(|e| anyhow!("listing sessions: {e}"))?;
        if let Some(p) = sessions.drain(..).next() {
            return Ok(Some(p));
        }
        return Err(anyhow!("no sessions found in {}", dir.display()));
    }
    Ok(None)
}
