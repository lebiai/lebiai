4000 行 Rust 写的自进化 Small Hermes Agent桌面版上线了，这一次，这只小怪兽走出了终端

不是给现有的 Agent 套个壳，是让一个已经能在终端里自我进化的 Agent，长出第二种交互形态。这是水到渠成的，直接站在 Cli 的基础上，用 GUI 的方式，向世界宣布，“Hello World!!”

### 先说说这个版本干了啥

是的 ，简单的来说就是 **Hermes 桌面版做完了。**

这是一个 Tauri 2 写的原生窗口程序，能在 macOS / Linux / Windows 上跑（macOS 已经能打 DMG）。它和原来的 `hermes` 命令行用的是**同一个引擎、同一份配置、同一组文件**，也就是说你在 CLI 里创建的记忆，在 GUI 里立刻能看到；你在 GUI 里通过的技能，下次开 CLI 也立刻能用。还有，你在 Cli 下的对话 session，同样也会直接同步到 GUI 中，就是这么的一致性。



```
~/.small-rust-hermes/
├── config.toml          # 同一份配置
├── mcp.json             # 同一份 MCP 配置
├── skills/              # 同一组技能（Markdown）
├── memories/            # 同一组记忆（Markdown）
└── sessions/            # 同一组会话（JSONL）

        ▲                       ▲
        │                       │
   hermes-cli              hermes-gui
   （命令行）               （桌面端）
        │                       │
        └────────┬──────────────┘
                 │
            同一个引擎
       hermes-core / -turn / -llm /
       -mcp / -store / -skills /
       -memory / -reflect / -tools
```

这是这次发布最重要的设计选择：GUI 不是一个新系统，是同一个 Agent 的第二张脸。因为有很多朋友就是喜欢在 GUI 下面操作，然后随着这个 GUI 的发布，我想在思考一段时间，在看看，Agent还能走出什么方向，除了进化，进化之外的东西呢？


### 给第一次听说 Small Hermes 的朋友：这是个什么东西

不是所有读者都看过之前几篇文章，所以我觉得，我先得把价值说清楚。

**Small Hermes 是一个用纯 Rust 写的、能自我进化的 AI Agent。**

它的核心循环跟所有 Agent 一样无聊：

```
用户输入 → LLM 推理 → 工具调用 → 结果反馈 → LLM 继续推理 → ... → 输出
```

但它在两个维度上和市面上 99% 的 "Agent 框架" 不一样：

**1. 它真的会变聪明。** 每次对话结束，系统会让 LLM 复盘整段对话：有没有值得记住的事实？有没有值得固化的工作流？有没有跟现有记忆冲突的认知？产出的不是模糊的"经验"，是我们让 agent 自主做的结构化的候选——`MemoryCandidate` / `SkillCandidate` / `ConflictCandidate`。**每个候选都必须经过你点头才会进库。** 这就是"自我进化"，没有玄学。

**2. 它的"大脑"是 Markdown 文件。** 没有向量数据库，没有 Redis，没有 Docker。一条记忆长这样：

```yaml
---
id: mem_a3f7b2c1
created: 2026-05-01T10:30:00Z
zone: core
tags: [rust, style]
pinned: true
supersedes: [mem_001_older]
---
用户偏好使用 anyhow 处理应用层错误，底层库使用 thiserror。
不要用 unwrap()。
```

你可以用任何编辑器打开它，可以 `git diff`，可以放进版本控制。Agent 不会偷偷改自己的"大脑"——所有改动都在文件系统上可见、可审计、可回滚。

**为什么这件事重要？** 因为市面上太多 Agent 把自己包装成黑盒：你不知道它记住了什么，不知道它学到了什么，不知道它哪天会因为什么原因突然行为变了。Small Hermes 的设计倾向是相反的——所有状态都摊在文件系统上，所有进化都需要你确认。

整个项目，11 个 crate，约 4000 行核心 Rust 源码。没有任何重型依赖。冷启动到首次响应 < 2 秒，运行时内存占用 ~15MB。

### 我的整体架构：11 个 crate，编译期守住边界

```
hermes-core        类型系统：Session / Message / ToolHost trait / 上下文压缩
hermes-llm         LLM 适配：Anthropic / OpenAI 兼容 / DeepSeek，流式 SSE
hermes-turn        执行引擎：单轮 tool loop + 权限 + 并行 + 取消
hermes-tools       13 个内置工具：read/write/edit/bash/grep/glob/git/think/todo/...
hermes-mcp         MCP 协议：stdio + Streamable HTTP，工具生态无限扩展
hermes-memory      记忆宫殿：zone 分区 + supersedes 链 + 效果追踪
hermes-skills      技能系统：触发匹配 + 效果追踪 + BM25 检索
hermes-reflect     自省引擎：micro 反思 + 全量反思 + profile 编译
hermes-store       持久化：JSONL 会话 + frontmatter 解析
hermes-cli         命令行前端：REPL + slash commands
hermes-gui         桌面前端：Tauri 2 + React + Channel 流式  ← 这次的新成员
```

依赖是严格单向的。`hermes-memory` 不知道 `hermes-tools` 的存在，`hermes-turn` 不关心你用的是哪个 LLM。换个 provider？改一行配置。加个前端？写一个新的 `*-cli` 风格的 crate，把核心 trait 实现一遍。

**这是 Rust workspace 的哲学：编译器帮你守住边界。** 当你的依赖图出现循环，Cargo 会拒绝编译。这种"硬约束"是 Python / TypeScript 框架里靠 code review 才能勉强维持的。


### 在来回顾回顾我们之前做的工作

老读者可以跳过这节，新朋友建议读一下——后面 GUI 那些屏幕，承载的都是这些设计。

#### Small Hermes 的记忆是宫殿，不是数据库，可以说是认知架构

记忆按 **zone** 分区：

```
core/          用户身份、偏好、核心原则（几乎不变，永远 pin 到 system prompt）
work/          当前焦点、近期决策（中频更新）
project:xxx/   项目级约定和上下文（按项目隔离）
episode/       会话摘要（高频写入）
general/       未分类（兜底）
```

**Supersedes 链**：当 Agent 学到新东西，它不删除旧记忆，而是创建一条新记忆，标记 `supersedes: [old-id]`。`list_active()` 会自动过滤掉被传递性超越的记忆。意味着——记忆的**历史**被保留了；更新是**原子的**；最坏情况只是多了一条冗余记忆，不丢数据。

**效果追踪**：每条记忆有两个事件——`Loaded`（被注入到 context）和 `Referenced`（LLM 的回复中真的引用了内容）。基于 Referenced / Loaded 比率算出一个 effectiveness factor（0.5 ~ 1.0），低效记忆在检索时被降权，但**永远不会降到 0**——它可能只是暂时不相关。这是"自然选择"，不是"清除"。

#### 双轨反思：进化也要算成本

```
全量反思（Session End）
  → 处理完整对话记录
  → 成本：~2000-5000 tokens in, ~500-1000 tokens out
  → 产出：技能候选 + 记忆候选 + 冲突候选

微反思（Per-Turn，异步）
  → 只看最近一轮交互，启发式触发
  → 成本：~500 tokens in, ~200 tokens out
  → 产出：最多 1 个记忆 + 1 个技能，confidence ≤ medium
  → 不阻塞用户输入
```

微反思的启发式条件：满足"最近 3 轮没反思过 + 用户说了'记住'/'以后'/'偏好' 这类词，或者发生过工具调用，或者有写操作"才触发。大部分对话不会反思——因为大部分对话确实不产生新知识。

#### 工具并行：不是优化，是正确

LLM 在一轮回复里请求 3 个 `grep` 调用，为什么要等第一个完成再开始第二个？

```
Phase 1: 分类
  ├── Denied（权限拒绝）→ 立即返回错误
  ├── Safe（read/grep/glob/git/think/...）→ 收集到 safe_calls
  └── Dangerous（bash/write/edit/MCP/memory_*）→ 收集到 confirm_calls

Phase 2: 安全工具并行
  futures::future::join_all(safe_calls)
  tokio::select! 配合 cancel，随时可中止

Phase 3: 危险工具串行确认
  for call in confirm_calls {
      ask_user_confirmation();
      if approved { execute(); }
  }
```

核心实现只有 30 行 Rust。能这样写是因为类型系统在编译期就保证了：

```rust
pub trait ToolHost: Send + Sync { ... }
F: Fn(TurnEvent) + Send + Sync
```

不需要 `Arc`，不需要 `Mutex`，不需要 `unsafe`。编译过就是正确的。

#### 三级权限，信任但验证

```rust
pub fn is_dangerous_tool(name: &str) -> bool {
    matches!(name, "bash" | "write" | "edit" | "memory_save" | "memory_delete")
        || name.contains("__")  // MCP 工具一律视为危险
}
```

5 行代码，意味深长：`bash` 可以执行任意命令；`write` / `edit` 可以改任何文件；`memory_*` 可以改 Agent 的"大脑"；MCP 工具来自第三方，不可信。

非危险工具直接执行，零延迟。危险工具弹确认。

#### 上下文管理，要学会和遗忘做朋友

128K 上下文听起来很多，但几轮工具调用就吃掉大半。small Hermes 的策略不是粗暴截断，而是 **LLM 驱动的摘要**：

```
压缩提示词：
"保留以下信息：关键决策、工具结果及其结果、用户声明的偏好、
用户明确要求记住的事实。简洁——目标是原长度的 1/5。"
```

旧消息被替换为 `[Context Summary]`，最近 4 轮保持原文。阈值是动态算的：把 system prompt + 历史 + 工具定义全部估进去，留 18% headroom 给 LLM 生成回复。

#### MCP，让工具生态可以无限扩展

Small Hermes 实现了完整的 [MCP 协议](https://modelcontextprotocol.io/)，支持 stdio 和 Streamable HTTP 两种传输。在 `~/.small-rust-hermes/mcp.json` 里加一个 server，Agent 就立刻多了一整组工具。命名空间用 `server__tool` 隔离，不会冲突。

这意味着 Small Hermes 不需要自己实现"一切工具"。文件系统操作有 `@modelcontextprotocol/server-filesystem`；GitHub 集成有 `@modelcontextprotocol/server-github`；自家服务写个 Python MCP server 也就十几行。Agent 核心循环根本不知道工具从哪里来——只看 trait。


### 终于到 GUI。下面挨个屏幕讲一下，GUI 长什么样



#### 主聊天界面


这一屏的细节：

- **流式渲染**：用 Tauri 的 `Channel<ChatStreamEvent>` 把后端事件推到前端。每个 token 立刻显示，跟 ChatGPT 一致的体验。
- **Thinking 块可折叠**：模型的思考过程默认收起来，想看就点开。
- **Tool 调用可展开**：工具名、参数、输出都在那，点开看完整内容，不点就只占一行。
- **Stop 按钮**：流式回复中按下，立刻取消（后端的 cancel 信道一路通到 LLM provider）。
- **会话侧栏**：自动派生标题（首条用户消息的前 60 字符），按时间倒序，点击切换会话。新对话第一条发出后，标题立刻刷新。

#### 工具确认弹窗

危险工具触发时弹出：



三个动作：

- **允许这次**：本次放行，下次还问
- **本次会话总是允许**：把这个工具加进当前 session 的 allow 列表
- **拒绝**：可附理由——理由会作为 `ToolResult` 返回给 LLM，让它知道你为什么拒绝，下一轮可以换个思路

这是个细节但很重要的设计：拒绝不只是 yes/no，是一次反馈。

#### 记忆面板



左侧 zone 导航，每个 zone 显示数量。右侧是记忆列表，支持全文搜索（前端做 token overlap，秒级响应）。

**新建记忆**：右上角按钮，弹出表单：fact 正文 + scope（user/project）+ zone + tags + pinned。提交后立刻写盘，立刻可见。

不需要打开 `$EDITOR`，不需要懂 YAML frontmatter——但你想这么干也可以，因为文件就在 `~/.small-rust-hermes/memories/` 下，随时可以手动编辑，GUI 下次加载会读到。

#### 技能面板



完整 CRUD：列表、查看、新建、编辑、删除。**内嵌编辑表单**——name / description / triggers / body 四个字段，body 是一个 textarea 直接编辑 Markdown。不用切到外部编辑器。

技能的 effectiveness factor 也在这里显示（"used 7 / matched 12 = 0.58"）——你能直观看到哪些技能值钱、哪些是摆设。

#### 反思冲突 UI

这是 GUI 里最有意思的一块。反思发现新候选与现有记忆冲突时：

```
┌─ Conflict Detected ──────────────────────────────────────┐
│                                                          │
│  现有记忆 (mem_001)                                       │
│  ┌──────────────────────────────────────────────────┐   │
│  │ 用户偏好使用 Python 进行数据分析。                  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  新候选                                                   │
│  ┌──────────────────────────────────────────────────┐   │
│  │ 用户已转向 Rust，不再使用 Python。                  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  冲突说明: 语言偏好已变更                                  │
│                                                          │
│  ─────────────────────────────────────────────────────  │
│  [保留新记忆]  [保留旧记忆]  [合并]  [作用域拆分]  [跳过]   │
└──────────────────────────────────────────────────────────┘
```

五个动作，跟 CLI 完全一致：

1. **保留新记忆** → 新记忆写入，`supersedes: [mem_001]`，旧记忆变为非活跃
2. **保留旧记忆** → 新候选丢弃
3. **合并** → 打开内嵌的 textarea，人工融合两条信息
4. **作用域拆分** → 写到对立 scope（user / project），两条都保留
5. **跳过** → 进入延迟队列，下次会话启动时再问

复杂决策直接可视化。比 CLI 的 a/r/k/m/s 单字母提示直观得多。

#### 设置面板


完全可编辑。写回 `~/.small-rust-hermes/config.toml` 时用 `toml_edit` 做原地修改——**保留你原来的注释和未知字段**。你手动写的注释不会因为 GUI 保存了一次就被冲掉。

API key 是 write-only：填了就写进去，读回来永远是占位符。这样防止从 GUI 的右键检查里把 key 扒出来。

配置变更**下次启动生效**——没有热重载，因为这关系到 LLM provider 实例化，热重载需要小心处理正在跑的请求。

### 为什么是 Tauri 不是 Electron

发布前问过自己这个问题。

```
            Electron           Tauri 2
   ───────  ────────────       ────────────
   语言     Node.js + 前端      Rust + 前端
   渲染     Chromium 整套      系统 WebView
   包大小   150-250 MB         8-30 MB
   内存     200-500 MB         50-100 MB
   启动     2-4s               <1s
```

但更重要的不是数字，是**架构契合度** ，去他妈的兼容性，我只要性能：

- Small Hermes 引擎本来就是 Rust。Tauri 让前端和引擎在**同一进程**里跑，前端通过 `invoke` 直接调用 Rust 函数。不需要单独跑一个 Node.js 后端转译。
- 流式数据用 Tauri 2 的 `Channel<T>`——比 Electron 的 IPC 更类型安全。
- 跨平台只是一个 `cargo build --release`。macOS / Linux / Windows 一份代码。

整个 GUI 后端只有 23 个 Tauri 命令：

```
chat:     send_message / cancel_stream / respond_confirm
session:  list / new / load / delete
memory:   list / create / delete / toggle_pin
skills:   list / get / save / delete
mcp:      list_servers / list_tools
config:   get / update
reflect:  run / accept_skill / accept_memory / handle_conflict
```

每个命令都是一个 `#[tauri::command] async fn`。前端 `await invoke("send_message", { ... })`，就这么简单。

### 死守一个原则：GUI 不会比 CLI 多知道任何东西

这是 GUI 设计阶段定下的硬约束。

**任何 GUI 功能，都不能依赖 CLI 没有的状态。** 反过来也成立：任何 CLI 创建的数据，GUI 都能识别。

意味着：

- 没有"GUI 数据库"——所有持久化走同一个 `~/.small-rust-hermes/`
- 没有"GUI 配置"——所有设置写回 `config.toml`，CLI 下次启动读到一样的
- 没有"GUI 专属技能"——技能就是 Markdown 文件，跨前端共享
- GUI 不维护自己的 session 索引——直接扫 `~/.small-rust-hermes/sessions/` 目录

副作用：你完全可以**早上 CLI 工作、下午 GUI 接着干**。会话还在，记忆还在，技能还在。GUI 只是给 CLI 配了一张脸。

这个原则的代价：GUI 不能做任何"只在 GUI 里有意义"的优化（比如本地索引加速）。但好处是——你永远知道 GUI 在干什么，因为它在干的事 CLI 也能干。没有黑盒。


### 咱们现在有些已知不完美

诚实是好习惯。这次发布的 GUI **不是完美的**，有这么几个已知缺口：

- **MCP 在 GUI 里只读**。能看到配了哪些 server、暴露了哪些工具，但增删改还得手工编辑 `mcp.json`。不是技术难度，是优先级——MCP 配置变更频率低，CLI/编辑器够用。
- **没有全局快捷键**。Cmd+N 新建对话、Cmd+K 命令面板——还没做。
- **没有代码块语法高亮**。Markdown 已经渲染了，但代码块是单色 monospace。
- **macOS 标题栏没定制**。是标准 Tauri 标题栏，不是无边框 traffic-light 风格。

不过，我理解，这些玩意都是些 polish，不是 blocker。核心闭环——聊天、工具调用、确认、记忆/技能管理、反思审批——全部可用。

下个版本会逐步补，但**不会为了赶 polish 牺牲 CLI/GUI 数据互通这个原则**。


### 怎么用

```bash
# 1. 克隆 + 构建
git clone https://github.com/coder-brzhang/small-rust-hermes.git
cd small-rust-hermes
cargo build --release

# 2. 配置（CLI 和 GUI 共用）
mkdir -p ~/.small-rust-hermes
cat > ~/.small-rust-hermes/config.toml << 'EOF'
default_provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
EOF
chmod 600 ~/.small-rust-hermes/config.toml

# 3a. 开发模式（vite + Tauri，热重载）
cd crates/hermes-gui/ui
npm install      # 首次
npm run dev      # 终端 1
cd -
cargo run -p hermes-gui   # 终端 2

# 3b. 打 macOS DMG
scripts/build-dmg.sh
# 产物：target/release/bundle/dmg/Hermes_<version>_<arch>.dmg
```

CLI 还是老样子：

```bash
hermes ask "解释一下这个函数"
hermes chat
hermes agent "给这个模块加上单元测试"
```

CLI 和 GUI 哪个顺手用哪个。它们看的是同一个 `~/.small-rust-hermes/`。


### 写在最后

做这个 GUI 之前，纠结过一个问题：**Agent 真的需要 GUI 吗？**

终端足够强大，CLI 已经能完成所有任务，加 GUI 是不是在做无用功？

最后说服自己的理由有两个：

**第一，自我进化的本质是"人类在回路中"，回路要顺手。** CLI 里反思候选审批是一个 a/r/d 字母提示，记忆冲突解决是 k/m/s 单字母选择。能用，但不直观。反思候选的内容多到一行装不下时，CLI 体验会陡降。GUI 让"看清候选、做出决定"这件事变成 5 秒的事，而不是 30 秒——这直接影响你愿不愿意频繁审批。

**第二，Agent 的状态值得被"看到"。** 记忆宫殿、技能列表、effectiveness factor、反思日志——这些都是 Agent 的"身体"。CLI 里你可以 `hermes memory list`、`hermes skills show xxx`，但看到的是一堆文本。GUI 把它们组织成可浏览的视图，让你随时知道 Agent 当前的"认知状态"。这种透明度是自进化系统的安全基础。

3000 行变成 4000 行，9 个 crate 变成 11 个 crate。但核心理念没变：

- 同一个引擎，两种皮肤
- 所有进化必须人类点头
- 所有状态摊在文件系统上
- 简单到能读完，强大到能用

终端用户和 GUI 用户共享同一个 Agent。它在 CLI 里学到的，在 GUI 里也记得；它在 GUI 里养成的习惯，CLI 里也带着。

少即是多。一份引擎，两种入口。


*项目地址：[small-rust-hermes](https://github.com/coder-brzhang/small-rust-hermes)*
