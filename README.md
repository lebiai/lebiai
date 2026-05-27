# Hermes

A self-evolving AI agent built in Rust.

Hermes learns from every conversation — distilling reusable skills, accumulating memories, and resolving conflicts — so it gets better the more you use it.

## Features

- **Self-evolution** — Reflection pipeline extracts skills and memories from conversations, with human approval
- **Micro-reflection** — Lightweight per-turn background analysis (~500 tokens) that never blocks input
- **MCP tools** — Connect any [Model Context Protocol](https://modelcontextprotocol.io/) server (stdio or Streamable HTTP)
- **Multi-provider** — Anthropic Claude, DeepSeek, any OpenAI-compatible API
- **Plain-file storage** — Memories and skills are Markdown + YAML frontmatter, human-readable and git-friendly
- **Single binary** — No database, no message queue; ships as a single static binary (Docker image also available)
- **CLI and desktop GUI** — Same engine, same files; pick the surface that fits the task

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

## Docker 一键部署

不想本地装 Rust 工具链、或者想把 WeChat bot 当作后台服务长跑？用 Docker：

```bash
# 1. 准备配置（同上 Quick Start 的 config.toml）
mkdir -p ~/.small-rust-hermes
# ... 写好 ~/.small-rust-hermes/config.toml ...

# 2. 构建 + 扫码登录微信（一次性）
docker compose build
docker compose run --rm hermes-wechat wechat login

# 3. 启动微信 bot 长跑
docker compose up -d
docker compose logs -f
```

镜像是 debian-slim 基础（~100 MB），同时也能当通用 CLI 用：

```bash
docker run --rm -it \
  -v ~/.small-rust-hermes:/data/.small-rust-hermes \
  -e HOME=/data \
  hermes:latest ask "解释这段错误"
```

完整说明见 [docs/docker.md](docs/docker.md)。

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

## Desktop GUI

The GUI is a Tauri 2 desktop app that talks to the exact same engine, config file, skills, memories, and sessions as the CLI. Anything you create in the GUI shows up under `~/.small-rust-hermes/` on disk, and vice versa.

What the GUI gives you that the CLI doesn't:

- **Tool confirmation modal** — when the model wants to run a tool that requires prompting, you get Allow / Always-allow (session) / Deny with an optional reason
- **Memory sidebar with zones** — left-side zone navigator (All / Pinned / one row per zone), search, and an inline create form with scope + zone + pinned
- **Skill CRUD** — create / edit / delete skills inline, no need to drop into `$EDITOR`
- **Editable settings** — model, max-tokens, base URL, API key (write-only), reflect min-turns, auto-accept, context limit, and permission allow/deny rules; writes round-trip via `toml_edit` so comments and unknown keys in `config.toml` survive
- **Reflection conflict UI** — for each conflict candidate: Keep new / Keep old / Merge (inline textarea) / Scope split / Skip; mirrors the CLI's `apply_conflict_action`

### Run in dev

Two processes — vite on `5173`, then the Tauri shell that loads it:

```bash
# Terminal 1 — frontend dev server (hot reload)
cd crates/hermes-gui/ui
npm install      # first time only
npm run dev

# Terminal 2 — Tauri window
cargo run -p hermes-gui
```

### Build a release bundle

```bash
cd crates/hermes-gui/ui && npm run build && cd -
cargo build -p hermes-gui --release
# binary: target/release/hermes-gui
```

### Package a macOS DMG

```bash
# one-time: cargo install tauri-cli --version "^2.0" --locked
scripts/build-dmg.sh
# output: target/release/bundle/dmg/Hermes_<version>_<arch>.dmg
```

Set `TAURI_TARGET=universal-apple-darwin` to build a universal (Intel + Apple Silicon) DMG. The resulting DMG is unsigned — for distribution add codesigning + notarization separately.

Config is shared with the CLI (`~/.small-rust-hermes/config.toml`, mode 600). The GUI writes the file atomically and preserves its mode. Changes take effect on next launch — there is no hot reload yet.

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

This project is licensed under the **PolyForm Noncommercial License 1.0.0** —
see the [LICENSE](./LICENSE) file for the full text.

**TL;DR:**

- ✅ **Free for noncommercial use** — personal study, research, hobby projects,
  teaching, and use inside non-profits / educational / governmental /
  charitable organizations.
- ❌ **Commercial use is not permitted without a paid license** — this
  includes building paid products on top of it, deploying it inside a
  for-profit company's workflow, or using its output as part of any
  revenue-generating activity.
- ❌ **Reselling, sublicensing, or repackaging for a fee is prohibited** —
  even with attribution.

**许可摘要（中文）：**

- ✅ **非商业用途免费**：个人学习、研究、业余项目、教学，以及在非营利
  组织 / 学校 / 政府机构 / 慈善机构内部使用，均无需付费。
- ❌ **未经书面授权禁止任何商业用途**：包括但不限于构建付费产品、
  在营利性公司内部工作流中部署、使用其输出物开展营利活动。
- ❌ **禁止售卖、转售、再许可、收费转发或重新打包**——即使附带
  原作者署名也不行。

### Commercial Licensing / 商业授权

如需将本项目用于任何商业场景，请联系作者获取商业授权：

- **作者 / Author:** 老码小张
- **联系邮箱 / Contact:** [1595819400@qq.com](mailto:1595819400@qq.com)

商业授权可根据使用范围、规模和场景一事一议，欢迎来信沟通。
