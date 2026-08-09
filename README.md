# lebi-AI（乐彼AI）

**Feels more like your hand every time.** Local. Sharper every yes.

> 越用越像你的手感。  
> 接得住你的想法，推得动你的事，必要时敢顶你——第二次更准。

Not a chatbot toy. Not a sycophant. Not a lawyer suite. Not a coding-only IDE.  
A **local work buddy**: understands how you think and work, moves real work forward, pushes back when it matters—and gets tighter with your standards after you approve what to keep.

Product card: [`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md) · Blueprint: [`docs/work-companion-solution.md`](./docs/work-companion-solution.md).

> **权威文档（仓库根仅此 4 份，冲突时按序号小的覆盖大的）**
> - [PRODUCT_PRINCIPLES.md](./PRODUCT_PRINCIPLES.md) — P0 产品原则
> - [DEVELOPMENT_RULES.md](./DEVELOPMENT_RULES.md) — P1 开发规则与变更流程
> - [AGENTS.md](./AGENTS.md) — P2 AI / 协作者入口
> - [README.md](./README.md) — P3 本简介
>
> **其他所有说明文档** → [`docs/`](./docs/)（索引：[`docs/README.md`](./docs/README.md)；
> 变更台账：[`docs/records/`](./docs/records/)）

## Features

- **Work & companion** — Do real work, advise, remember with evidence, optional care after delivery (see product protocol in `hermes-core`)
- **Better with use** — Reflection extracts work episodes, standards, preferences, and skills — with human approval
- **Micro-reflection** — Lightweight per-turn background analysis that never blocks input
- **MCP tools** — Connect any [Model Context Protocol](https://modelcontextprotocol.io/) server (stdio or Streamable HTTP)
- **Multi-provider** — Anthropic Claude, DeepSeek, any OpenAI-compatible API
- **Plain-file storage** — Memories and skills are Markdown + YAML frontmatter, human-readable and git-friendly
- **Single binary** — No database, no message queue; ships as a single static binary (Docker image also available)
- **CLI and desktop GUI** — Same engine, same files; GUI primary surface is **Dialogue** (对话), not idle chat

## Quick Start

```bash
# Clone and build
git clone https://github.com/coder-brzhang/small-rust-hermes.git
cd small-rust-hermes
cargo build --release

# Configure (requires an API key)
mkdir -p ~/.lebi-ai
cat > ~/.lebi-ai/config.toml << 'EOF'
default_provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
EOF
chmod 600 ~/.lebi-ai/config.toml

# Run
cargo run -p hermes-cli -- chat
```

## Docker 一键部署

不想本地装 Rust 工具链、或者想把 WeChat bot 当作后台服务长跑？用 Docker：

```bash
# 1. 准备配置（同上 Quick Start 的 config.toml）
mkdir -p ~/.lebi-ai
# ... 写好 ~/.lebi-ai/config.toml ...

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
  -v ~/.lebi-ai:/data/.lebi-ai \
  -e HOME=/data \
  hermes:latest ask "解释这段错误"
```

完整说明见 [docs/docker.md](docs/docker.md)。

## 消息渠道

乐彼AI（lebi-AI）支持通过微信、飞书和 Telegram 与 AI 对话——在手机上发消息，乐彼AI 在后台自动回复。

### 微信 (WeChat)

```bash
# 1. 扫码登录（终端显示二维码，用微信扫码授权）
hermes wechat login

# 2. 启动长轮询，接收微信消息并回复
hermes wechat run
```

凭证保存在 `~/.lebi-ai/wechat.toml`（mode 600）。

### 飞书 (Feishu / Lark)

飞书通过 **WebSocket 长连接** 接收消息，无需公网回调地址，适合本地开发和内网部署。

#### 前置准备

1. 登录 [飞书开放平台](https://open.feishu.cn)，创建一个 **自建应用**
2. 在应用凭证页面获取 **App ID** 和 **App Secret**
3. 开启 **机器人** 能力：应用功能 → 机器人 → 开启
4. 添加事件订阅：事件与回调 → 事件配置 → 添加事件 → 搜索 `im.message.receive_v1` 并订阅
5. 接收消息方式选择 **长连接**：事件与回调 → 接收消息方式 → 选择「使用长连接接收消息」
6. 发布应用版本并让目标用户/群组可见

#### 配置与运行

```bash
# 方式一：交互式配置（推荐）
hermes feishu auth
# 按提示输入 App ID 和 App Secret，自动验证并保存

# 方式二：手动填写配置文件
cat > ~/.lebi-ai/feishu.toml << 'EOF'
app_id = "cli_xxxxxxxxxxxx"
app_secret = "xxxxxxxxxxxxxxxxxxxxxxxx"
domain = "https://open.feishu.cn"
EOF
chmod 600 ~/.lebi-ai/feishu.toml

# 启动飞书长连接
hermes feishu run
```

运行后 乐彼AI 会通过 WebSocket 连接到飞书服务器，收到文本消息时自动调用 AI 回复。每个飞书用户拥有独立的会话历史，保存在 `~/.lebi-ai/sessions/feishu/{user_id}/` 下。

工具调用时会在飞书中实时推送 🔧 摘要消息，让你知道 AI 正在做什么。

### Telegram

Telegram 通过 Bot API 长轮询接收消息，无需公网回调地址，适合本地开发和内网部署。

#### 前置准备

1. 在 Telegram 中找 **@BotFather**，发送 `/newbot` 按提示创建 bot，得到 **Bot Token**
2. （可选）在 BotFather 用 `/setprivacy` 设为 Disabled，让 bot 能收到群里 @ 之外的指令

#### 配置与运行

```bash
# 方式一：交互式配置（推荐，自动验证 token 并保存）
hermes telegram auth
# 按提示输入 Bot Token

# 方式二：手动填写配置文件
cat > ~/.lebi-ai/telegram.toml << 'EOF'
bot_token = "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
EOF
chmod 600 ~/.lebi-ai/telegram.toml

# 启动长轮询
hermes telegram run
```

运行后 乐彼AI 会长轮询 Telegram Bot API，收到文本消息时自动调用 AI 回复。每个 chat 拥有
独立的会话历史，保存在 `~/.lebi-ai/sessions/telegram/{chat_id}/` 下。长轮询游标
持久化在 `~/.lebi-ai/telegram-offset.txt`，重启后不会重复回复已处理的消息。

工具调用时会在 Telegram 中实时推送 🔧 摘要消息，让你知道 AI 正在做什么。目前只支持文本消息，
贴纸 / 图片 / 语音等会收到「目前只支持文本消息。」的提示。

## Usage

```bash
# One-shot question — lightweight single turn, no session identity, no
# memory/skills injection, no confirmation prompt (all tool calls auto-approved).
# Prefer `hermes chat` for anything involving memory, skills, or dangerous tools.
hermes ask "explain this error: cannot borrow as mutable"

# Interactive chat (session-end full reflection when min turns met; /reflect always)
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
hermes session show ~/.lebi-ai/sessions/abc123.jsonl

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

Configure MCP servers in `~/.lebi-ai/mcp.json`:

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

The GUI is a Tauri 2 desktop app that talks to the exact same engine, config file, skills, memories, and sessions as the CLI. Anything you create in the GUI shows up under `~/.lebi-ai/` on disk, and vice versa.

What the GUI gives you that the CLI doesn't:

- **Tool confirmation modal** — when the model wants to run a tool that requires prompting, you get Allow / Always-allow (session) / Deny with an optional reason
- **Session-end reflection** — leaving the current chat (new session, switch history, delete active, or quit the window) runs full reflection when `reflect.min_turns` is met; candidates are reviewed in a modal before anything is written (same approve gate as the Reflect panel)
- **Memory sidebar with zones** — left-side zone navigator (All / Pinned / one row per zone), search, and an inline create form with scope + zone + pinned
- **Skill CRUD** — create / edit / delete skills inline, no need to drop into `$EDITOR`
- **Editable settings** — model, max-tokens, base URL, API key (write-only), reflect min-turns, auto-accept, context limit, and permission allow/deny rules; writes round-trip via `toml_edit` so comments and unknown keys in `config.toml` survive
- **Reflection conflict UI** — for each conflict candidate: Keep new / Keep old / Merge (inline textarea) / Scope split / Skip; mirrors the CLI's `apply_conflict_action`

### Run the desktop GUI (default path — avoids white screen)

The window loads **`crates/hermes-gui/ui/dist`**, not Vite on port 5173.
If you only `cargo run -p hermes-gui` while `devUrl` pointed at 5173 and Vite
was not running, you get a **blank webview**. Default config does **not** use 5173.

```bash
# Recommended (build frontend + start)
scripts/run-gui.sh

# Equivalent
cd crates/hermes-gui/ui && npm install && npm run build && cd -
cargo run -p hermes-gui
```

After editing `ui/src/**`, run `npm run build` again (or re-run `scripts/run-gui.sh`).

Optional hot-reload (collaborators only, not the product path): see [docs/gui-run.md](docs/gui-run.md).

### Build a release bundle

```bash
# beforeBuildCommand in tauri.conf.json also runs `npm run build` under ui/
cd crates/hermes-gui/ui && npm install && npm run build && cd -
cargo build -p hermes-gui --release
# binary: target/release/lebi-AI
```

### Package a macOS DMG

```bash
# one-time: cargo install tauri-cli --version "^2.0" --locked
scripts/build-dmg.sh
# output: target/release/bundle/dmg/lebi-AI_<version>_<arch>.dmg
```

Set `TAURI_TARGET=universal-apple-darwin` to build a universal (Intel + Apple Silicon) DMG. The resulting DMG is unsigned — for distribution add codesigning + notarization separately.

### Package a Windows EXE

```powershell
# On a Windows machine (Tauri cannot cross-compile installers from macOS):
# one-time: cargo install tauri-cli --version "^2.0" --locked
.\scripts\build-exe.ps1
# output: target\release\bundle\nsis\lebi-AI_<version>_<arch>-setup.exe
```

Or build both installers on CI (tag `v*` or manual run): `.github/workflows/release.yml`
(macOS → DMG, Windows → NSIS EXE). Both are unsigned — SmartScreen/Gatekeeper will
prompt once on first launch.

### After downloading an installer

Install, first-launch allowlist (unsigned app), API-key setup, where your data lives,
uninstall and FAQ: see [docs/install.md](docs/install.md)（用户拿到安装包后怎么操作）。

Config is shared with the CLI (`~/.lebi-ai/config.toml`, mode 600). The GUI writes the file atomically and preserves its mode. Changes take effect on next launch — there is no hot reload yet.

## Architecture

```
hermes-core        Core abstractions: Session, LlmProvider, ToolHost, context compaction
hermes-channel     Shared chat-channel driver (CLI/GUI): Channel trait, ServeCtx, per-user sessions
hermes-llm         Anthropic + OpenAI-compatible provider implementations
hermes-turn        Turn execution engine: tool loop, parallel execution, permissions
hermes-tools       Built-in tools: read/write/edit/bash/grep/glob/git/think/todo/palace
hermes-mcp         MCP client (rmcp): stdio and Streamable HTTP transports
hermes-store       JSONL session persistence, frontmatter parsing
hermes-skills      Skill loading, storage, relevance matching (token overlap + triggers, optional embedding hybrid)
hermes-memory      Memory palace: zone-based storage, supersedes chain, effectiveness tracking
hermes-reflect     Full + micro reflection pipeline, profile compilation
hermes-cli         CLI entry point and subcommands
hermes-weixin      WeChat (iLink Bot) bridge: QR login, shared long-poll serve loop, send message
hermes-feishu      Feishu (Lark) bridge: WS long-connection, protobuf frames, send message
hermes-server      HTTP/WS backend for the mobile client (bearer-token auth; routes 1:1 with GUI commands)
hermes-gui         Desktop GUI (Tauri)
hermes-telegram   Telegram bridge (shares the channel driver with wechat/feishu)
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
~/.lebi-ai/
├── config.toml          # API keys, provider config (mode 600)
├── mcp.json             # MCP server definitions
├── wechat.toml          # WeChat bot token (mode 600)
├── feishu.toml          # Feishu app_id/app_secret (mode 600)
├── telegram.toml       # Telegram bot token (mode 600)
├── server.token        # hermes-server bearer token (mode 600, auto-generated)
├── skills/              # Learned skills (Markdown + YAML frontmatter)
├── memories/            # Accumulated memories (Markdown + YAML frontmatter)
├── sessions/            # JSONL transcripts
│   └── feishu/          # Per-Feishu-user session JSONLs
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
