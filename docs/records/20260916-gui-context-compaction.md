# 变更记录：上下文压缩接进 GUI / server（会话不再只增不减）

| 字段 | 内容 |
|------|------|
| **编号** | `20260916-gui-context-compaction` |
| **日期** | 2026-09-16 |
| **状态** | 已验收 |
| **负责人** | Codex |
| **关联** | 诊断会话（2026-09-15「交流忽然很慢」）；`docs/records/20260913-*` 同批次 |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不是「CLI 有、GUI 也照抄一份」——那会是同一段判据的第 3、4 份拷贝
  （`hermes-cli/commands/chat`、`hermes-turn/agent` 已经各有一份），下次改阈值必然只改一处。
- **拆出的真：** 「这段会话该不该压、压完留几条」只取决于四样东西——系统提示词、会话消息、
  工具 JSON、上下文配置。四个入口（GUI / CLI / agent / server）**都有这四样**，差别只在
  「压完怎么告诉用户」：CLI 打一行字，agent 发 `AgentEvent`，GUI 发流事件，server 发 WS 帧。
  另外真的一点：**压缩必须落盘**。会话是 append-only JSONL，不记一笔的话，重启后
  从文件回放又是全长上下文——那就等于没修，只是把慢推迟到下次开 App。
- **如何从真推出：** 判据上收到 `hermes-core::compaction::maybe_compact` 一处，四个入口都调它，
  删掉两份手写判据；落盘上加 `SessionEvent::Compaction`，回放时按序把「前 N 条消息」换成摘要。
  回放是顺序的，连续压两次也能逐条重放出同样的内存状态。

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 桌面 GUI 用户（默认路径）、Flutter/server 用户（同引擎）。
- **解决什么痛点：** 一段会话聊到后面越来越慢——实测单次调用 prompt 从 10.9k 涨到
  **288,550 tokens**，同一类问题从 19 秒变成 107 秒；一条消息最多 26 次 LLM 往返，每次
  都重发整段历史。
- **用完后用户多得到什么：** 长会话不再退化；同一句话在会话第 1 轮和第 26 轮的等待时间接近。
- **真实会话实测**（用户数据 `test/sessions/2026-09-15T04-23-04-0bc1eef3.jsonl`，275 条消息）：
  现 317,262 估算 tokens → 折叠后约 **6,318**（最近 8 条仅 4,270）。这是把 107 秒的
  一轮拉回常速的依据。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（用户什么都不用做，压缩自动发生）
  - [x] 不增加无意义确认或噪音（不弹窗、不要求点确认）
  - [x] 主操作一眼能找；空/载/错态完整
  - [x] 高频路径步骤少、界面干净

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在某个工位（如情报王海燕）连着干了一上午，同一条会话已经很长。
- **怎么走完：** 用户照常打字 → 发送后**什么都不用做** → 若这次发送触发了整理，
  对话流里出现一条**安静的一行提示**（如「已整理较早的上下文，保留最近几轮」）→
  回答照常流式输出。
- **看起来怎么样：** 提示是一条**居中的细灰文字**，不是气泡、不是红字、不占卡片；
  放在被整理之后、本轮回答之前。没有整理时**完全不出现**（不能每次都说）。
  加载态 = 提示出现后正常转圈；错误态 = 压缩失败**不打断回答**，只在日志留痕，用户无感。
- **好走 / 好看：** 不需要用户做任何决定；不新增开关（阈值走既有 `[context]` 配置）。
- **成功标准：** 长会话第 N 轮的单次输入 token 回到 `model_limit` 以内；用户能感知到
  「它自己整理过了」但不需要理解细节。
- **明确不做什么：** 不做「压缩前让你确认」；不做手动「压缩」按钮（v1）；
  不改阈值默认值；不动记忆/技能的注入。

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 引擎能力未被默认入口使用 + 状态机缺一个事件 + 持久化缺一种记录。
  三段都在，只是没接上。
- **正确的长期默认路径：** 判据只有一处（`hermes-core::compaction`）；任何入口想跑一轮
  对话，先过这一处；压缩结果必须能被 JSONL 回放出来，不依赖「进程还活着」。
- **与引擎/各入口边界：** 共享 `hermes-core`，不 fork。GUI / CLI / agent / server 都是
  同一个 `maybe_compact` 的调用方，各自只翻译事件。
- **安全影响：** 压缩摘要会发给用户自己配置的模型服务商（与普通对话同一条通道，
  不引入新出口）；摘要纯文本落本地明文，与既有会话一致；无新增网络端点。
- **如何防复发：** 回放测试（写 compaction 记录 → 读回来必须是摘要 + 最近若干条）+
  阈值测试（低于阈值必须 `None`，不得每次调用都压）。
- **为何这不是补丁：** 它删掉了两份重复判据、补上了缺失的持久化事件，让四个入口共用一条路径；
  不是在某个入口加一个 if。

---

## 1. 方案（Plan）

- **目标：** 让长会话的 prompt 回到模型上限以内；四个入口共用一处判据；压缩结果可回放。
- **范围：**
  - 做：`hermes-core`（`maybe_compact` + `SessionEvent::Compaction`）、`hermes-store`（回放）、
    GUI 接入 + 流事件 + 一行提示、server 接入、CLI / agent 改用共享判据。
  - **不做：** 新配置项、手动压缩按钮、压缩前确认、工具结果二次裁剪、缓存字段修复（另开）。
- **用户路径变化：**
  - 改前：GUI 长会话每轮 prompt 无上限增长（实测到 288k）→ 越来越慢。
  - 改后：越过 `model_limit × (1 - headroom)` 时自动整理，保留最近 `keep_recent_turns` 轮；
    对话里出现一行安静提示。
- **技术要点：** `crates/hermes-core/{session,compaction}.rs`、`crates/hermes-store/src/session.rs`、
  `crates/hermes-gui/{events.rs,commands/chat.rs}`、`crates/hermes-server/src/routes/chat.rs`、
  `crates/hermes-cli/src/commands/chat/mod.rs`、`crates/hermes-turn/src/agent.rs`、
  `crates/hermes-gui/ui/src/**`。
- **风险与回滚：**
  - 摘要丢细节 → 保留最近 `keep_recent_turns` 轮原文；摘要是**新增**一条 User 消息，
    旧记录仍在文件里（append-only 没有真删），人工可查。
  - 旧版本读到新事件：`SessionEvent` 未知变体会被既有「跳过坏行」逻辑忽略 → 回落到全长上下文，
    功能不坏、只是慢。可接受。
  - 回滚：删除调用点即可，`SessionEvent::Compaction` 记录可留在文件里不影响旧逻辑。
- **方案确认：** [x] 已对照 P0/P1 · 2026-09-16 · Codex

## 2. 实施（Implement）

### 2.1 判据上收到一处（删掉两份拷贝）

- `crates/hermes-core/src/compaction.rs`
  - 新增 `CompactionPolicy { model_limit, headroom, keep_recent_turns }` 与
    `Compacted { replaced, summary, before_tokens, after_tokens }`。
  - 新增 **唯一入口** `pub async fn maybe_compact(...) -> Result<Option<Compacted>>`：
    `Ok(None)` = 不用压（不产生任何模型调用）；`Err` = 摘要失败且**未改动**会话。
  - `compact_session` 改私有（返回 `replaced + summary`）；`should_compact` 也改私有 —
    公开它就等于欢迎第 3、4 份拷贝。
  - 摘要提示词补一句：**摘要必须与对话同语言**。原先中文会话拿到英文摘要，
    而摘要会作为一条 user 消息回到历史里，模型会跟着改用英文回答。
    （端到端实测发现，见 §3#4）
  - 新增 `pub const SUMMARY_PREFIX: &str = "[Context Summary]"`：摘要的开头**由引擎
    写死**（提示词同时改成「只写正文，不要前缀」，并防御性剥一次模型自带的前缀）。
    落盘与回放用的都是带前缀的这一份，界面才能稳定认出它。
- 删除重复判据：`crates/hermes-cli/src/commands/chat/mod.rs`、
  `crates/hermes-turn/src/agent.rs` 原先各写了一份「该不该压」，现均改为调用
  `maybe_compact`（CLI `mod.rs:394`、agent `agent.rs:226`）。

### 2.2 压缩必须能回放（否则重启等于没修）

- `crates/hermes-core/src/session.rs`：`SessionEvent` 增 `Compaction(CompactionRecord)`；
  新增 `CompactionRecord { replaced, summary, at }`，并由 `lib.rs` 导出。
- `crates/hermes-store/src/session.rs`：回放该事件 = 裁掉当前消息列表最前 `replaced` 条
  （`min` 夹取防坏数据），插入 `Message::user_text(summary)`。顺序回放，连压多次结果一致。

### 2.3 四个入口接线

| 入口 | 判据来源 | 落盘 | 用户可见 |
|------|----------|------|----------|
| GUI `hermes-gui/src/commands/chat.rs:430` | `cfg.context.*` | `ensure_writer()` + `append(SessionEvent::Compaction)` | 流事件 `ContextCompacted`（`events.rs:73`）→ 对话里一行细灰提示 |
| server `hermes-server/src/routes/chat.rs:375` | 同上 | 同上 | WS 帧 `ContextCompacted`（`events.rs:77`） |
| CLI `hermes-cli/.../chat/mod.rs:394` | 同上 | 同上 | 终端一行 `(context compacted: N → summary + M recent, ~A → ~B tokens)` |
| agent `hermes-turn/src/agent.rs:226` | `agent_config.context_*` | 由调用方落盘 | `AgentEvent::Compacted { removed }` |

- 四处均为：**成功**才改写内存历史 + 落盘 + 同步 `propose_messages` 快照；
  **失败**只 `tracing::warn!` 放行 —— 压不了顶多慢，不该让用户这轮说不出话。
- GUI/server 只翻译事件，不重写判据；`SessionEvent::Compaction` 是唯一持久化形状。

### 2.4 UI

- `ui/src/types/index.ts`：`ChatStreamEvent` 增 `contextCompacted` 帧。
- `ui/src/store/chatStore.ts`：新增 `contextCompacted: boolean`，8 处状态重置点全部归零，
  收到帧置 `true`（新会话/切会话不会残留上一次的提示）。
- `ui/src/components/chat/ChatView.tsx`：被整理后、本轮回答前，插一条**居中细灰**一行提示。
- `ui/src/i18n.ts`：`chat.contextCompacted` 中英各一条。
- `ui/src/utils/displayMessages.ts` + `ui/src/components/chat/MessageBubble.tsx`：
  摘要落盘时也是一条 **user** 消息，直接渲染会变成一条「用户从没打过、却像他自己说的」
  气泡。改为按 `SUMMARY_PREFIX` 认出来，渲染成一张**默认收起的说明卡**
  （「较早的对话已整理成摘要——点一下看看」，点开才是摘要正文）；不带编辑按钮。
  顺带修掉一处失效样式类：`text-app-text-soft` / `border-app-line` 不在主题里
  （`index.css` 只有 `app-fg-tertiary` / `app-border`），是上一版提示的无色元凶。

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 短会话不受打扰 | 单测 `short_session_compacts_nothing_and_calls_nothing` | 不触发、**模型调用 0 次**、消息一条不动 | ✅ | 核心防回归：不能每轮都调一次 |
| 2 | 长会话自动整理 | 隔离数据目录 + `model_limit=4000` 合成 12 条长历史，`hermes chat --resume` 发一条 | 打印 `(context compacted: 9 messages → summary + 4 recent, ~13487 → ~10284 tokens)`，回答照常 | ✅ | 真机真模型 |
| 3 | 重启后不退化 | 紧接用例 2 再 `--resume` 同一文件 | 头部显示 `(resumed; 6 prior turns)` = 摘要 + 最近 4 条 + 本轮，而不是 13 条全长 | ✅ | 回放重建成功 |
| 4 | 摘要不换语言 | 用例 2 的中文会话 | 摘要为中文 | ✅（首轮实测为英文 → 改提示词后复测中文） | 见 §2.1 |
| 5 | 坏数据不炸 | 单测 `compaction_record_with_bad_count_is_clamped` | `replaced` 超出条数被夹取、`0` 被忽略 | ✅ | |
| 6 | 多次压缩可回放 | 单测 `compaction_records_fold_prefix_on_replay` | 连压两次 → `["S2","m6","m7","m8"]` | ✅ | |
| 7 | 摘要有稳定开头且只一份 | 单测 `over_threshold_...` 断言 + `compose_summary_strips_a_model_supplied_prefix` | 会话里第一条以 `[Context Summary]` 开头，且只出现一次 | ✅ | 界面据此认说明卡 |
| 8 | 摘要不冒充用户说话 | `tsc --noEmit` + `npm run build` | 摘要渲染为默认收起的说明卡，无编辑按钮 | ⏳ 代码就绪，目视待确认 | `MessageBubble.tsx` |

- **自动化：** `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings`
  - 2026-09-16：`fmt` 干净、`clippy -D warnings` 无告警、`cargo test --workspace` 退出码 0
    （39 个 `test result: ok`，无 `FAILED` / `failures:` / `panicked`）
  - UI：`npx tsc --noEmit` 干净、`npm run build` 通过（`ui/dist` 已更新）
- **手工：** GUI 已重启到含本改动的构建（前端 `npm run build` 后 `cargo build -p hermes-gui`，
  再启 `target/debug/lebi-AI`；进程启动时间晚于二进制链接时间，确认跑的是新构建）；
  真机长会话（王海燕）目视待用户走一遍
- **测试结论：** [x] 全部通过（GUI 目视项见 §4）

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 长会话不再单调退化；实测单次 prompt 曾到 288k tokens |
| 开箱即用未破坏 | ✅ | 无新依赖、无新配置项、无终端要求；阈值沿用既有 `[context]` |
| 本地优先未破坏 | ✅ | 摘要走用户自己的模型通道，明文落本地，无新网络端点 |
| 测试通过 | ✅ | 见 §3 |
| 记录完整 | ✅ | 本文件 + `docs/records/README.md` 索引 |
| 产品+架构两视角齐全 | ✅ | §0b / §0c |
| 非修修补补（默认路径正确） | ✅ | 判据收敛到一处并删掉两份拷贝；补上缺失的持久化事件，而非在某入口加 if |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理 | ✅ | `compact_session`/`should_compact` 收私有；CLI、agent 的重复判据已删 |
| 操作与视觉：走查完成，好看、好走 | ⏳ | 代码与构建就绪，GUI 真机目视待用户确认（一行细灰提示） |
| 第一性原理：三步写全 | ✅ | §0-fp |

- **验收人：** Codex（工程）· 用户（GUI 目视）
- **结论：** ☑ 工程通过，GUI 目视待确认
- **遗留项：** B（每个工位的新对话入口）、C（搜索负缓存）、D（缓存字段）、E（GUI 落日志）
