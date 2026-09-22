# 变更记录：项目组 · 第 1 期 —— 项目组真的存在（会话 / 侧栏 / 名册 / 缺席）

> **For agentic workers:** 本文件同时是**实施计划**。按 `- [ ]` 逐条执行；每步都能单独验证。

**Goal：** 让《财富早知道》**作为一个东西存在**：侧栏里有它、点进去是**它自己的那一条**会话、
会话里的人知道自己在哪张桌子上、**组里缺谁看得见**。

**Architecture：** 项目组是**编译期产品设定**（`crates/hermes-core/src/teams/*.md` + `team.rs`），
与人物定义同一个做法（`include_str!`、不进数据根）。会话多一层归属：`SessionMeta.team`
与 `persona` **并列且互斥**。提示词多一块：`team::block(...)`，位置固定在**人物块之后、记忆之前**。

**Spec：** [`../spec/projects.md`](../spec/projects.md)（设计源）· [`../spec/personas.md`](../spec/personas.md)（人物规格）

---

## Global Constraints（每条任务都隐含遵守）

1. 中文主词用**「对话」**；关系词用**「搭子」**。
2. 项目组定义**编译进二进制**，**不落数据根**；文件名 stem = `id`。
3. **一个项目组只有一条会话**，日复一日（与工位同一条规矩）。
4. 提示词顺序即断言：**搭子协议 → 人物块 → 组块 → 在办 → base → 记忆与卡**。
5. **缺人不许静默**：缺席显示为缺席，不假装有人，也不悄悄藏掉（规格 §7.1）。
6. **不串味**：组会话的记忆归**组**，不进任何人的工位会话（规格 §4.2）。
7. 不新增依赖；根目录只允许 4 个 md。
8. **不提交**——等用户点头（本项目长期约定）。
9. 质量门槛：`cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` 全绿。

---

## 0-fp. 第一性原理（本期）

- **拒绝的类比：**
  1. **不是**「给侧栏加一个分组标题」——分组只是外观；用户要的是点进去**只有一条会话**，
     不是又冒出几十条按日期排的对话。
  2. **不是**「把 7 个人物的提示词拼在一起塞进去」——拼在一起等于谁都是谁，硬拦当场失效
     （`personas.md` §6 是本产品的立身之处）。
  3. **不是**「组内成员不受授权码控制」（规格 §7.1 明确拒绝过的第一条）——演示最顺，
     但不买也能用，绕开了卖人物的模式。
- **拆出的真：**
  1. **一张桌子需要一个名字**：会话得知道自己是「谁的」。现有 `SessionMeta` 只有 `persona`
     一档，项目组会话落进去就变成「无人物会话」，和旧会话、自由对话混成一类。
  2. **一条会话一次只由一个人接**：v1 没有接力（第 2 期），所以「谁在说」有唯一答案——
     **接口人**（规格 §5：没人被 @ 时吕老师接）。先把这条做对，接力才有落脚点。
  3. **缺席是信息，不是故障**：用户要能一眼看出「这活为什么跑不起来」（缺谁、从哪得到）。
- **如何从真推出：**
  - `SessionMeta` 加 `team`，与 `persona` 互斥；**归属转换只有一处**（`memory_owner_for`
    从「只认人物」变成「认人物或组」），否则组会话的记忆会串到所有人头上。
  - 组块只写**事实**：组名、这一期、桌上坐着谁（含缺席）、你这一轮是谁。
    它不合并任何人的职责——每个人物仍然是它自己。
  - 缺席由**授权与勾选**算出来（既有判据 `items_at`），不另立一套。

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（默认路径）。
- **解决什么痛点：** 规格里那个组今天**在界面上不存在**——用户没法「点进项目组」，
  只能一个个去戳工位；而「这活谁接」在界面上没有任何表达。
- **用完后用户多得到什么：** 侧栏多一节「项目组」；点进去是一条**日复一日的组会话**；
  会话里的人知道自己是《财富早知道》的谁；缺人时**当场看得见缺谁**。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 不需要用户做任何操作（换版本即生效）
  - [x] 不增加确认或噪音
  - [x] 空 / 载 / 错态完整（空：还没开工；载：沿用既有；错：缺席说清缺谁）
  - [x] 高频路径步骤数不变（点一下进组）

## 0b. 产品经理视角

- **场景：** 用户打开 App，看侧栏；点「财富早知道」说一句「今天开工」。
- **怎么走完：** 侧栏 `工位` 下面多一节 `项目组`，只有一行：**财富早知道**，
  一行小字写出这一期的位置（`今天这期 · 还没开工`）。点它 → 直接进**那一条**组会话。
- **看起来怎么样：** 与工位行**同一套视觉**（同一行高、同一套字号），只有两处不同：
  ① 首字用方块底（事，不是人）；② 小字写的是**这一期**，不是「他是谁」。
  组内成员**不进侧栏**——他们是这张桌子上的人，点开组才看见名册。
- **组内缺席怎么看：** 组会话顶部一行名册：`桌上 5 人 · 缺席 2 人`，点开列全 7 人，
  缺席的那行灰掉并写明`缺席 · 需要「主编吕老师」`。**不做可点的假按钮**——
  点了没反应比没有更让人生气。
- **空 / 载 / 错态：**
  - 空：没说过话 → 一行「今天还没开工。说一句，或者等第一份情报进来。」
  - 载：沿用既有载态，不新增假进度条。
  - 错：缺人不喊红叉——**灰掉 + 一句人话**（2026-09-13 约定）。
- **成功标准：** 侧栏看得到「财富早知道」；点进去**只有一条**会话；组里说话的人标着名字；
  把一个人从授权里拿掉，他当场变成「缺席」。
- **明确不做什么：** 接力与「决定」载体（第 2 期）· 教学回路与标准版本（第 3 期）·
  长会话按天归档与按需读（第 4 期）· 产物卡片（第 2 期）· 头像。

## 0c. 架构师视角

- **根因层级：** 「一个会话属于谁」现在只有一档（人物）。项目组逼出第二档，
  而**归属是记忆隔离的输入**——漏改一处，组里的口径就会以「全局」的身份灌进每个人的工位会话。
- **正确的长期默认路径：** 组是**编译期定义 + 由它派生一切**（名册、缺席、提示词组块、
  侧栏那一行）。加一个组 = 加一个定义文件 + 一处数组，别处不再写第二份判据。
- **边界：** 本期动 `hermes-core`（team 模块、SessionMeta、归属转换）、`hermes-channel`
  （组块与顺序断言）、`hermes-gui`（命令 + 侧栏 + 会话创建）、`hermes-store`（无改动，
  meta 本来就整份落盘）。**不动**接力、决定、教学、归档。
- **如何防复发：** 顺序断言（组块在人物块之后、记忆之前）· **互斥断言**（persona 与 team
  不许同时有）· 名册断言（成员必须是在册人物）· 缺席断言（码里没有的人不许出现在桌上——
  这是规格 §7.1 拒绝过的第二条）。
- **为何这不是补丁：** 它没有为「财富早知道」写任何特判——组是数据，
  引擎只认「会话有没有组、组里有谁、谁在册」。

## 1. 范围

**做：** `team.rs` + 一个组定义 · `SessionMeta.team` · 归属转换扩一档 · 提示词组块 + 顺序断言 ·
`list_teams` 命令 · 会话按组创建/复用 · 侧栏两节 · 组内缺席显示 · 说话人标签 · i18n · 测试 · 台账。
**不做（留后续期）：** 接力（2）· 决定载体（2）· 产物卡片（2）· 教学回路与标准版本（3）·
三档注入里的「自己」那一档（3）· 长会话归档与按需读（4）· 头像。

---

## Task 1：引擎里立「项目组」

**Files：**
- Create: `crates/hermes-core/src/teams/caifu-zaozhidao.md`
- Create: `crates/hermes-core/src/team.rs`
- Modify: `crates/hermes-core/src/lib.rs`（`pub mod team;` + 导出）
- Test: `crates/hermes-core/src/team.rs`（同文件 `mod tests`）

**Interfaces（Produces）：**
- `team::Team { id, name, role, interface, members, body }`（`members: Vec<TeamMember{ id, duty }>`）
- `team::all() -> &'static [Team]` · `team::get(id) -> Option<&'static Team>`
- `team::ids()` · `Team::member(id)` · `Team::interface()`（解析成 `&Persona`）
- `team::block(team, present, self_id) -> String`（Task 3 用）

- [x] **Step 1：写定义** `crates/hermes-core/src/teams/caifu-zaozhidao.md`

frontmatter：`id / name / role / interface / members / version`；正文 = 这组干什么 + 一期怎么走。

- [x] **Step 2：写 `team.rs`（先红：解析 + 名册 + 断言）**
- [x] **Step 3：跑测试** `cargo test -p hermes-core team::`
- [x] **Step 4：收工（不提交）**

## Task 2：会话多一层归属

**Files：**
- Modify: `crates/hermes-core/src/session.rs`（`SessionMeta.team`）
- Modify: `crates/hermes-core/src/persona.rs`（`memory_owner_for(persona, team)`）
- Modify: 15 处调用点（gui chat/reflect/micro/personas · cli reflect/chat · server routes · reflect prompt）
- Test: 各文件既有的归属测试 + 新增互斥与组归属断言

- [x] **Step 1：`SessionMeta.team`**（`#[serde(default, skip_serializing_if)]`，与 `persona` 互斥）
- [x] **Step 2：归属转换扩一档**（**唯一入口**，不许在调用点写第二份判据）
- [x] **Step 3：迁移 15 处调用点**
- [x] **Step 4：跑全量测试**

## Task 3：提示词里的组块

**Files：**
- Modify: `crates/hermes-channel/src/companion_context.rs`（`team` 字段 + 插入 + 顺序断言）

- [x] **Step 1：顺序断言（先红）**：组块必须在人物块之后、在办之前
- [x] **Step 2：`team` 字段与插入**
- [x] **Step 3：`team: None` 逐字节相同**（有/无组之间只多那一段）
- [x] **Step 4：跑测试** `cargo test -p hermes-channel`

## Task 4：GUI 后端

**Files：**
- Create: `crates/hermes-gui/src/commands/teams.rs`
- Modify: `crates/hermes-gui/src/commands/{mod.rs,session.rs,chat.rs}` · `main.rs`（注册命令）
- Test: `crates/hermes-gui/src/commands/teams.rs`

- [x] **Step 1：`list_teams` 命令**（名册 + 缺席 + 这一期）
- [x] **Step 2：会话按组创建/复用**（`new_session(teamId)`，与 persona 互斥）
- [x] **Step 3：`SessionSummary.team`**（前端认领的唯一来源）
- [x] **Step 4：组会话的记忆归属 = 组**（归属转换已在 Task 2 收敛，这里只接线）
- [x] **Step 5：跑测试**

## Task 5：GUI 前端

**Files：**
- Modify: `crates/hermes-gui/ui/src/components/layout/Sidebar.tsx`
- Modify: `crates/hermes-gui/ui/src/store/chatStore.ts` · `types/index.ts` · `i18n.ts`
- Modify: `crates/hermes-gui/ui/src/components/chat/*`（说话人标签）

- [x] **Step 1：侧栏两节**（`工位` / `项目组`；组行小字写这一期）
- [x] **Step 2：点进组 = 那一条会话**（复用 `openStation` 的同一套逻辑，不写第二份）
- [x] **Step 3：组会话顶部名册 + 缺席**（灰掉、不可点、写明缺谁）
- [x] **Step 4：说话人标签**（组会话的回复标出人名）
- [x] **Step 5：`npm run build`**

## Task 6：文档同步 + 台账

- [x] **Step 1：`docs/spec/projects.md`** 若实现与规格有出入，以实际为准回填
- [x] **Step 2：`docs/records/README.md`** 加一行
- [x] **Step 3：回填本记录 §3 / §4**

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 1 | 侧栏看得到项目组，点进去是一条会话 | 打开 GUI | 只有一行「财富早知道」，点开进得去 | 待目视（GUI 已用新二进制打开） |
| 2 | 换一天回来还是那一条 | 关掉重开再说一句 | 同一份会话文件追加，不新建 | 待目视 |
| 3 | 组里说话的人看得出是谁 | 说一句 | 回复标着「主编吕老师」 | 待目视 |
| 4 | 缺人看得见 | 把雨天的勾去掉 | 名册里他变「缺席」，组行小字提示缺人 | 待目视（自动化已覆盖名册与缺席文案） |
| 5 | 组里的记忆不串味 | 组里教一条 | 不进任何工位会话 | 待目视（归属转换有自动化断言） |
| 6 | 老规矩没破 | `cargo test --workspace` | 全绿 | **通过**（0 failed；见下） |

**自动化（本机实跑）**

- `cargo fmt --all -- --check` → 通过
- `cargo clippy --workspace --all-targets -- -D warnings` → 通过
- `cargo test --workspace` → 全绿（`hermes-core` 86 · `hermes-gui` 38+2+2 · `hermes-channel`/`server` 等合计 0 failed）
- `cd crates/hermes-gui/ui && npx tsc --noEmit && npm run build` → 通过（`ui/dist` 已重建）

**第 1 期新增/改写的断言**

| 断言 | 在哪 |
|------|------|
| 每个组定义都解析、id 唯一、stem = id、成员必须是**在册人物** | `team.rs` |
| 接口人必须在成员里，且是「没人被点名时接的那个人」 | `team.rs` |
| 组块：桌上的人带那一摊活、缺席的人写「（缺席）」、本轮接棒的人标出来 | `team.rs` |
| 组块位置 = **人物块之后、记忆之前**；没有组时提示词**逐字节相同** | `hermes-channel/companion_context.rs` |
| 组会话的记忆归**组**（两条轴都给时以组为准） | `hermes-core/persona.rs` |
| 空草稿只为**同一身份**复用（换人 / 换桌子都不复用） | `hermes-gui/commands/session.rs` |
| 组会话里「谁在说」= 接口人；不认识的项目组不许凭空长人 | `hermes-gui/commands/personas.rs` |
| 名册：到齐 → 报这一期的位置；缺人 → **说清缺谁**；缺接口人 → 桌子不可跑 | `hermes-gui/commands/teams.rs` |
| 组会话走到回合提示词时带着自己那张桌子 | `hermes-gui/commands/chat.rs` |

- **手工：** 上面第 1–5 条（GUI）——见 §5 手测清单
- **测试结论：** 自动化全绿；手工 5 条待目视

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑（工程） | 组从「规格里的字」变成界面上可点的一条：侧栏一节 + 它自己的会话 + 名册 |
| 开箱即用未破坏 | ☑ | 不需要用户做任何操作；组定义编译进二进制，换版本即生效 |
| 本地优先未破坏 | ☑ | 组会话仍是本机明文 JSONL；不新增依赖、不新增目录 |
| 测试通过 | ☑ | fmt / clippy / test --workspace / tsc / vite build 全绿 |
| 记录完整 | ☑ | 本文件 + `docs/records/README.md` + `docs/spec/projects.md` §11 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补 | ☑ | 组是数据（一个 md + 一处数组），引擎只认「有没有组、组里有谁、谁在册」；没有为《财富早知道》写特判 |
| 代码卫生 | ☑ | 归属转换只留一处（`memory_owner_for`）；`LoadedSessionData` 补回丢失的身份字段；侧栏人物行与组行共用一个 `openWorkplace`；frontmatter 解析收敛到 `frontmatter::split` |
| 操作与视觉 | ☐ | **待目视**：GUI 已用新二进制打开（`LEBI_DATA_DIR=…/test`），侧栏两节 / 名册 / 说话人待你过眼 |
| 第一性原理三步写全 | ☑ | §0-fp |

- **验收人：** 用户（目视）
- **结论：** ☐ 通过（自动化通过；目视待确认）
- **遗留项：** 第 2 期（接力 + 决定载体 + 产物卡片）· 第 3 期（教学回路 + 三档注入的「自己」）·
  第 4 期（长会话按天归档 + 按需读）· 头像 · 项目组归档区（§4.3）

---

## 5. 手测清单（请你过眼）

1. 侧栏 `工位` 下面多了一节 `项目组`，只有一行：**财富早知道**，
   小字 `今天这期 · 还没开工`，首字是**方块**（人用圆的）。
2. 点它 → 进**一条**会话；头部是 `财富早知道` + `今天这期 · 还没开工`；
   头部下方一条细名册：`桌上 7 人`。
3. 点那条名册 → 展开 7 行：名字 · 他那摊活。全是灰蓝（都到齐）。
4. 说一句（例如「今天开工」）→ 回复上方标着 **主编吕老师**；说完撤回窗口再看，
   小字变成 `今天这期 · 进行中`。
5. 退出重开 App，再点这一行 → 回到**同一条**会话（不新建）。
6. 到「设置 → 工位」把**编辑雨天**取消勾选 → 回侧栏：组行小字变琥珀色
   `缺 1 人 · 需要「编辑雨天」`；进组 → 名册变 `桌上 6 人 · 缺席 1 人`，
   展开后他那行灰掉并写 `缺席 · 需要「编辑雨天」`。
