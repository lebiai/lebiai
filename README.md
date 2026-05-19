# Hermes

A self-evolving AI agent built in Rust.

Hermes learns from every conversation — distilling reusable skills, accumulating memories, and resolving conflicts — so it gets better the more you use it.

## Features

- **Self-evolution** — Reflection pipeline extracts skills and memories from conversations, with human approval
- **Micro-reflection** — Lightweight per-turn background analysis (~500 tokens) that never blocks input
- **MCP tools** — Connect any [Model Context Protocol](https://modelcontextprotocol.io/) server (stdio or Streamable HTTP)
- **Multi-provider** — Anthropic Claude, DeepSeek, any OpenAI-compatible API
- **Plain-file storage** — Memories and skills are Markdown + YAML frontmatter, human-readable and git-friendly
- **Single binary** — No database, no message queue, no Docker

## Quick Start

```bash
# Clone and build
git clone https://github.com/coder-brzhang/small-rust-hermes.git
cd small-rust-hermes
cargo build --release

# Configure (requires an API key)
mkdir -p ~/.small-rust-hermes
cat > ~/.small-rust-hermes/config.toml << 'EOF'
default_provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
EOF
chmod 600 ~/.small-rust-hermes/config.toml

# Run
cargo run -p hermes-cli -- chat
```

## Usage

```bash
# One-shot question
hermes ask "explain this error: cannot borrow as mutable"

# Interactive chat (with reflection at session end)
hermes chat

# Autonomous agent: iterate until goal is complete
hermes run "add unit tests for the auth module"

# Resume last session
hermes chat --resume-last

# Specify model
hermes chat --model claude-sonnet-4-20250514
```

### Managing Knowledge

```bash
# Skills
hermes skills list
hermes skills show create-pr-with-tests
hermes skills delete old-skill

# Memories
hermes memory list
hermes memory list --pinned
hermes memory show mem_a1b2c3d4
hermes memory pin mem_a1b2c3d4
hermes memory delete mem_a1b2c3d4

# Sessions
hermes session list
hermes session show ~/.small-rust-hermes/sessions/abc123.jsonl

# Reflection stats
hermes reflect-stats --last 20
```

### MCP Tools

```bash
# List configured servers
hermes mcp list

# Test connectivity and list tools
hermes mcp test
hermes mcp test filesystem
```

Configure MCP servers in `~/.small-rust-hermes/mcp.json`:

```json
{
  "servers": {
    "filesystem": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"]
    },
    "web": {
      "type": "http",
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

## Architecture

```
hermes-core        Core abstractions: Session, LlmProvider, ToolHost, context compaction
hermes-llm         Anthropic + OpenAI-compatible provider implementations
hermes-turn        Turn execution engine: tool loop, parallel execution, permissions
hermes-tools       Built-in tools: read/write/edit/bash/grep/glob/git/think/todo/palace
hermes-mcp         MCP client (rmcp): stdio and Streamable HTTP transports
hermes-store       JSONL session persistence, frontmatter parsing
hermes-skills      Skill loading, storage, relevance matching (BM25 + triggers)
hermes-memory      Memory palace: zone-based storage, supersedes chain, effectiveness tracking
hermes-reflect     Full + micro reflection pipeline, profile compilation
hermes-cli         CLI entry point and subcommands
hermes-gui         Desktop GUI (Tauri)
```

### How Reflection Works

```
Session ends
    │
    ▼
Full Reflection ──→ LLM analyzes entire transcript
    │
    ├─→ Skill candidates    (reusable multi-step procedures)
    ├─→ Memory candidates   (durable facts / preferences)
    └─→ Conflict candidates (contradictions with existing knowledge)
    │
    ▼
User confirms / rejects each candidate
    │
    ▼
Accepted items persisted as Markdown files
```

During a session, **micro-reflection** runs asynchronously after qualifying turns — catching "remember this" moments without waiting for session end.

## File Layout

```
~/.small-rust-hermes/
├── config.toml          # API keys, provider config (mode 600)
├── mcp.json             # MCP server definitions
├── skills/              # Learned skills (Markdown + YAML frontmatter)
├── memories/            # Accumulated memories (Markdown + YAML frontmatter)
├── sessions/            # JSONL transcripts
└── reflect-log.jsonl    # Reflection acceptance/rejection audit log
```

## Development

```bash
# Typecheck (fast feedback)
cargo check --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Build release
cargo build --workspace --release
```

Minimum Rust version: 1.78 (edition 2021).

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `HERMES_LOG` | Tracing filter (default: `warn,hermes_cli=info`) |

## License

MIT OR Apache-2.0
