# 变更记录：项目组 · 第 4 期 —— 一条会话跑一年（按天分层 + 翻旧账）

> **For agentic workers:** 本文件同时是**实施计划**。按 `- [ ]` 逐条执行；每步都能单独验证。

| 字段 | 内容 |
|------|------|
| **编号** | `20260918-projects-group-phase4` |
| **日期** | 2026-09-18 |
| **状态** | **已验收**（2026-09-19 统一签收 · 依据与未验项见 [`20260919-acceptance-sweep`](./20260919-acceptance-sweep.md)）；原状态：待验收（自动化全绿 · 待目视） |
| **负责人** | 用户 + Agent |
| **关联** | [`../spec/projects.md`](../spec/projects.md) §4.4 / §9 第 7 条 · 第 1–3 期台账 |

---

## 0-fp. 第一性原理（本期）

- **拒绝的类比：**
  1. **不是**「把会话切成很多条小会话（按天一条）」——那会打破「一个工位只有一条会话」这条已经定死的规矩，
     而且拆完就再也拼不回来：上下文、接力、这一期，全都跨天。
  2. **不是**「给会话文件做索引数据库」——今天不需要（见下），而且新增一份必须与文件保持一致的派生数据，
     正是本项目一直在避免的那种「总有一天对不上」。
  3. **不是**「把旧消息删掉/只留摘要」——旧账是资产：用户问「上周三为什么把半导体压在末尾」，得翻得到。
- **拆出的真（先量，不猜）：**
  1. **读取今天不是瓶颈。** 实测（debug 构建，真实最大会话 834 KB / 289 事件）：
     `read_session` **17 ms**、`read_session_listing` 0.06 ms、序列化成 IPC 载荷 4.4 ms。
     所以「慢」不是今天要抢救的火——**要治的是增长曲线**：文件只增不减，界面一次全量收，
     一年后是 20 MB 与几千条气泡。
  2. **一条长会话缺的是「导航」，不是「速度」**：一天一天往下走，昨天的活和上个月的活长得一样，
     用户找不到「那天」。
  3. **模型侧的账会丢**：上下文压缩把旧轮次换成摘要，摘要是有损的——用户问细节时，
     模型手里已经没有原文（文件里有）。
  4. **下标即地址（危险）**：编辑/重发今天靠「消息下标」截断会话文件（`truncate_session(keepCount)`）。
     一旦界面只拿窗口，下标基准就变了——**会安静地删掉一段历史**。这是本期最硬的约束：
     任何窗口化都必须带着「窗口前面还有多少条」这个偏移，且语义必须与截断同源。
- **如何从真推出：**
  - **读**：打开会话只把**最近窗口**交给界面；更早的按天折成一条条横条，
    点开才取那一天（`load_session_day`）。窗口与天索引都在**内存会话**上切——
    与 `truncate_session` 吃的是同一个数组，下标不会错位。
  - **偏移**：载荷带 `baseOffset`（窗口前有多少条）；编辑重发那一处把它加回去，语义不变。
  - **旧账**：模型侧多一个 `conversation_recall` 工具，按关键词/日期翻**本会话文件**（含被压缩掉的原文）；
    界面上它就叫「翻旧账」，慢也是看得见的慢。

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（默认路径），一个工位/一个项目组日复一日地往下干。
- **解决什么痛点：**
  ① 一条会话越来越长，打开越来越重，昨天今天前天糊成一片，找不到「那天」；
  ② 想回到某天看一眼当时怎么定的，只能一直往上滚；
  ③ 问「上周三为什么把半导体压在末尾」——旧轮次早被压缩成摘要，模型手里没有原文。
- **用完后用户多得到什么：**
  ① 打开长会话**一眼看见最近**，更早的收成 `9 月 16 日 · 41 轮`，点开就能看；
  ② 翻旧账**看得见**：条上写「正在翻 9 月 16 日…」，模型翻的时候工具卡写着「翻旧账」；
  ③ 旧账**只是回看**（v1）：过去的日子不提供编辑/重发——不会一不小心动到今天的历史。

## 0b. 产品经理视角

- **场景：** 用户在《财富早知道》里干到第 12 天。打开会话 → 底部是最近 80 条（今天为主），
  往上是一条条横条：`9 月 16 日 · 41 轮`、`9 月 15 日 · 33 轮`…
- **怎么走完：** 点 `9 月 16 日 · 41 轮` → 条上变 `正在翻 9 月 16 日…` → 那天的气泡就地展开在横条下面，
  横条变成可收起的标题；再点收起。整个过程**不离开这条会话**，不新开窗口。
- **看起来怎么样：**
  - 横条：整行、浅底、左对齐日期、右侧一条小字 `41 轮`，与「这一期」带同一套克制风格；
    hover 有反馈，展开后左侧竖线（表示这里是旧账，不是现在）。
  - 旧账里的气泡**没有**编辑/重新生成/截断按钮（不是禁用，是不出现——禁用的按钮只会让人问为什么）。
  - 空态：没有更早的天 → **什么都不出现**（不出现空横条）。
  - 载入：条上 `正在翻 9 月 16 日…`（不转圈、不弹窗）。
  - 错态：读不出来 → 条上灰字 `这一天读不出来了 · 再试一次`，点一下重来；**不弹红**。
- **成功标准：** 30 天的会话打开时不假死；能两步内翻到任意一天；模型被问旧事时翻得到，且用户看得出它在翻。
- **明确不做什么：** 按天拆成多条会话 · 编辑/重发旧账 · 旧账跨会话搜索（只翻本会话）·
  给旧账做索引数据库 · 「第 N 期」的期号进横条（要按天读决定文件，v1 不做，横条只写日期与轮数）。

## 0c. 架构师视角

- **根因层级：** 数据**读的粒度**（整个文件 → 整个界面）与**增长曲线**，不是某个函数慢。
- **正确的长期默认路径：**
  - 分层的判据只有一处：`hermes_store::session_days`（按「用户消息的本地日期」切轮、切天）。
  - 窗口的判据只有一处：`window_split(messages, WINDOW_MESSAGES)` → `(base, days)`；
    `base` 同时就是载荷里的 `baseOffset`——**窗口与截断吃同一个数组、同一个下标**。
  - 旧账读取只走 `load_session_day`；模型侧只走 `conversation_recall`（读文件，不受压缩影响）。
- **边界：** 动 `hermes-store`（分层与检索）· `hermes-channel`（`SessionRecallHost`）·
  `hermes-gui`（命令 + 前端）· `hermes-server` / `hermes-cli`（只接 `conversation_recall` 工具面，
  窗口化暂不接——它们不是默认入口，今天的数据量也不成问题，写进遗留项）。
  **不动**：会话文件格式（零迁移）· 压缩 · 提示词顺序 · 硬拦。
- **安全影响：** 无新增外部面；`conversation_recall` 只读本会话文件，路径由引擎给，**不接受模型传路径**。
- **如何防复发：**
  ① 窗口化后「编辑重发」的截断测试（带 baseOffset 的用例，错的基准必须红）；
  ② 天分层的判据测试（跨天、同一天多轮、无 `at` 的老消息 → 归上一轮/「更早」）；
  ③ 短会话（< 窗口）行为逐字节不变：没有折叠条、消息一条不少。
- **为何这不是补丁：** 三件事都是通用能力（分层、窗口偏移、本会话检索），没有一处为《财富早知道》写特判。

---

## Task 1：分层与窗口（`hermes-store`）

**Files：** `crates/hermes-store/src/session_days.rs`（新）· `crates/hermes-store/src/lib.rs`

- [x] **Step 1**：`day_of(message)` 判据：用户消息且 `at` 有值 → 本地日；无 `at` 的用户消息 → 归**上一轮**；
  第一条有日期的用户消息之前的所有消息 → `None`（界面上叫「更早」）。
- [x] **Step 2**：`day_groups(&[Message]) -> Vec<DayGroup { day: Option<String>, label, turns, from, messages }>`
  （`from` = 该组在会话数组里的起点；组内只含从 `from` 到下一组起点之间的消息）。
- [x] **Step 3**：`window_split(messages, window) -> (base, Vec<DayGroup>)`：
  `base = len.saturating_sub(window)`；只保留 `from < base` 的组，并把跨过 `base` 的组**截到 base**。
- [x] **Step 4**：`recall_in_session(path, query, limit) -> Vec<RecallHit{ day, index, role, text }>`：
  读文件（含被压缩掉的原文），按**整词/子串**匹配用户与助手文本，返回命中及其所在天。

## Task 2：GUI 命令

**Files：** `crates/hermes-gui/src/commands/session.rs`

- [x] **Step 1**：`LoadedSessionData` 加 `baseOffset: usize` 与 `days: Vec<SessionDayData>`；
  `load_session` 只返回窗口。
- [x] **Step 2**：新命令 `load_session_day(sessionId, day)` → 该天被折叠那一段的 `MessageData[]`。
- [x] **Step 3**：测试：短会话 `baseOffset == 0 && days.is_empty()`；长会话 `baseOffset > 0`。

## Task 3：界面按天收起（前端）

**Files：** `ui/src/types/index.ts` · `ui/src/store/chatStore.ts` · `ui/src/components/chat/ChatView.tsx` ·
`ui/src/components/chat/DayFold.tsx`（新）· `ui/src/i18n.ts`

- [x] **Step 1**：类型 + store：`days` / `baseOffset` / `expandedDays: Record<string, DisplayMessage[]>` / `dayLoading` / `dayError`。
- [x] **Step 2**：`editAndResend` 的 `keepCount = rawStart + baseOffset`（**唯一的偏移点**，写注释）。
- [x] **Step 3**：`DayFold` 组件（收起/载入/展开/错态四态）+ 插到时间线顶部；展开的内容**只读**。
- [x] **Step 4**：i18n 中英：`day.turns` / `day.loading` / `day.error` / `day.retry` / `tool.recall`。

## Task 4：模型侧翻旧账

**Files：** `crates/hermes-channel/src/session_recall.rs`（新）· `hermes-gui` / `hermes-server` / `hermes-cli` 各一处接线

- [x] **Step 1**：`SessionRecallHost::new(inner, path)`：`list_tools` 多一条 `conversation_recall`，
  `call` 命中就翻本会话，其余原样转发（与 `PersonaToolHost` 同形）。
- [x] **Step 2**：三处入口各包一层（GUI / server / CLI）。
- [x] **Step 3**：测试：工具面里有没有这条、命中返回带日期、没有命中不编。

## Task 5：文档 + 台账

- [x] **Step 1**：`docs/spec/projects.md` §14 回填第 4 期
- [x] **Step 2**：`docs/records/README.md` 加一行
- [x] **Step 3**：回填本记录 §3 / §4

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 1 | 短会话一切照旧 | 打开一条不长的会话 | 没有折叠条，消息一条不少 | ✅ `gui::session::a_short_session_is_sent_whole_with_no_folds_and_no_offset` · `session_days::a_short_session_has_no_folds_at_all` |
| 2 | 长会话按天分层 | 打开 12 天的会话 | 底部是最近窗口；上面是 `9 月 16 日 · 41 轮` | ✅ `session_days::the_window_keeps_the_tail_and_folds_everything_before_it` · `a_day_cut_by_the_window_reports_only_its_folded_turns` · `two_days_make_two_groups_and_undated_old_messages_go_to_earlier` |
| 3 | 翻得到那天 | 点 `9 月 16 日` | 那天的气泡就地展开，且**没有**编辑按钮 | ✅ 只读是**构造出来的**（旧账那段不传 `onEditUser`/`onRegenerate`）；取回走 `load_session_day` · **目视待做** |
| 4 | 翻的时候看得见 | 点开的瞬间 | 条上写「正在翻…」，不假死 | ⚠️ 四态在 `DayFold` 里（`dayLoading` / `dayError`）· **目视待做** |
| 5 | 读不出来不装死 | 文件被删/坏 | 条上灰字 + 再试一次 | ⚠️ `toggleDay` 的错态分支 · **目视待做** |
| 6 | 编辑旧消息不错位 | 在窗口里编辑重发 | 截断到正确位置（不会少删/多删） | ✅ `a_long_session_sends_the_window_and_the_offset_that_editing_needs`（窗口 + 偏移 = 全量） |
| 7 | 模型翻旧账 | 问「上周三为什么把半导体压在末尾」 | 命中那天的原文，工具卡写「翻旧账」 | ✅ `tools::session_recall` 两条 + `session_days::recall_finds_what_was_said_and_says_which_day_it_was`；工具卡文案「翻旧账」 |
| 8 | 老规矩没破 | `cargo test --workspace` | 全绿 | ✅ 见下 |

- **自动化：** `cargo fmt --all -- --check` ✅ · `cargo clippy --workspace --all-targets -- -D warnings` ✅ ·
  `cargo test --workspace` ✅（0 failed）· `npx tsc --noEmit && npm run build` ✅
- **手工（待目视）：** 打开长会话看折叠条、翻一天、在**窗口里**编辑重发一次、问一句旧事（看工具卡）
- **测试结论：** [x] 自动化全绿；[ ] 目视待做（第 3 / 4 / 5 条）

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☐ | 打开一眼看见最近；两步翻到任意一天 |
| 开箱即用未破坏 | ☑ | 短会话逐条不变（`a_short_session_...` 两条测试） |
| 本地优先未破坏 | ☑ | 会话文件格式零迁移；没新增外部依赖（`hermes-store` 加了 workspace 内的 `chrono`） |
| 测试通过 | ☑ | fmt / clippy `-D warnings` / `cargo test --workspace` / `tsc + vite build` 全绿 |
| 记录完整 | ☑ | 本记录 + spec §14 + README |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补 | ☑ | 分层/窗口/检索三处通用能力；无组特判 |
| 代码卫生 | ☑ | 偏移只有一处（`editAndResend`）；窗口判据只有 `window_split`；`message_to_data` 收成一个 |
| 操作与视觉 | ☐ | 折叠条 / 载入 / 错态 / 旧账只读 目视 |
| 第一性原理三步写全 | ☑ | §0-fp（**含实测数字**） |

- **验收人：** 用户
- **结论：** ☐ 待目视
- **遗留项：** server/CLI 的窗口化 · 旧账里的期号（第 N 期）· 旧账跨会话搜索 ·
  在旧账处「从这天重新开始」（截断到某天，v1 只读）· 决定的历史版本回看 · 内联产物卡 · 头像
