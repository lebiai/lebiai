# 变更记录：项目组 · 第 3 期 —— 它得先学会你（教学回路 + 三档注入的「自己」）

> **For agentic workers:** 本文件同时是**实施计划**。按 `- [ ]` 逐条执行；每步都能单独验证。

**Goal：** 你在组里说一句「往期公告都吃 1/3」，**这句话要落得下来、看得见、下一期照它干**：
落成**这个栏目的标准**（不是某个人的、更不是全局的），带上**谁说、何时、为什么、第几版**；
删掉新版时上一版自己回来。同时把注入补齐：组会话里每个人看见
**全局 + 本项目组 + 自己（这一轮说话的人）**——手艺不再被锁在工位里看不见。

**Spec：** [`../spec/projects.md`](../spec/projects.md) §4.2 / §5.1 · [`../spec/personas.md`](../spec/personas.md) §5.2 / §5.4

---

## Global Constraints

1. 中文主词用**「对话」**；关系词用**「搭子」**。
2. **判据只有一处**：写入归属 = `hermes_memory::resolve_owner`；可见性 = `visible_to`；
   会话 → 归属 = `persona::memory_owner_for` / `memory_owners_for`。**不许在调用点再写一份。**
3. **不点头不落盘**不变：候选进 inbox，用户点头才成为记忆。
4. **自带角色 / 无人物 → 全局**不变（工具人李现在、资料员小文、大导演，以及自由对话）。
5. **注入隔离不变**：别人的专业口径一条都不给。
6. 不新增依赖；根目录只允许 4 个 md。
7. **不提交**——等用户点头。
8. 质量门槛：`cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
   `cargo test --workspace` · `tsc + vite build` 全绿。

---

## 0-fp. 第一性原理（本期）

- **拒绝的类比：**
  1. **不是**「再做一个知识库/文档管理」——那是把「标准」搬出记忆另起一套存储，
     结果是注入面要读两个地方、归属要判两次、蒸馏管不到它。标准就是记忆的一种，不新开系统。
  2. **不是**「让模型自己写 owner」——模型连自己的人物 id 都不该知道（§0c）。
     它只能说「这是这份活的口径 / 这是关于用户本人的」，**选归属是引擎的事**。
  3. **不是**「给标准做一个审批流」——v1 的点头点只有一个：候选进来你点头。
     多一个审批层就是多一处用户要学的规矩。
- **拆出的真：**
  1. **「标准」不是「偏手」也不是「全局」**：关于你本人的偏好 → 全局（在哪儿都算数）；
     关于**这个栏目**的口径 → 这个组；关于**这顶帽子**的手艺 → 那个人（§4.2 判据：
     换个项目还算数吗）。**今天这三档是错的**——组里的标准会因为 `zone=standards`
     被判成全局，**串味到每一个工位**（比放错工位严重）。
  2. **记不住出处 = 学不会**：一条标准必须带着「谁说的、什么时候、因为什么」，
     否则用户没法判断它学没学会，也没法改。**今天的 `rationale` 在落盘时被丢掉了。**
  3. **可修订的才是标准**：新一条取代旧的（`supersedes`），旧版留着——链挂在**新**那条上，
     所以删掉新版，**上一版自己回来**（回退不需要第二个状态字段）。
  4. **手艺要跟着人走**：接了棒的人，他的口径必须看得见，否则组里每个人都成了「只有组规的同一个大脑」。
- **如何从真推出：**
  - 归属判定改一处（`resolve_owner`）：会话是组时，偏好仍全局、标准归组、**点名成员 → 归他**，
    判不准 → 组（宁可窄）。
  - 注入改成**一组 owner**（全局 + 组 + 说话人），判据仍是 `visible_to`，只是「任一命中」。
  - 出处落盘：frontmatter 加 `because`（写入只有候选这一路）。
  - 版本 = `supersedes` 链，界面上显示第 N 版与「取代了谁」，回退 = 删新版。

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（默认路径），在《财富早知道》里日复一日调教这几个人。
- **解决什么痛点：**
  ① 组里说「公告吃 1/3」→ 这句话**悄悄变成了全局记忆**，你和小金聊林碳时它也端出来；
  ② 王海燕采料的纪律（手艺）在组里**看不见**——她进了组就像忘了自己怎么干活；
  ③ 教完只有一句「记下了」，**说不清归谁、哪一版、因为什么**；想改只能自己去找文件。
- **用完后用户多得到什么：**
  ① 教了之后那条记得住**归谁**（《财富早知道》的标准 / 王海燕的手艺），下一期照它干；
  ② 「它记得的」里每条能看到**归属、因为什么、第几版、取代了谁**，还能**退回上一版**；
  ③ 组里接了棒的人，手艺跟着他上桌（别人一条不串）。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（还是 md 文件，加一个可选字段）
  - [x] 不需要用户做操作（没教东西时不产生任何新界面）
  - [x] 不增加确认（点头点还是那一处：候选进来你点头）
  - [x] 空 / 载 / 错态完整（没有归属列 → 显示「全局」；链断了 → 第 1 版）
  - [x] 高频路径步骤数：教一句 = 说一句 + 点一次头（与今天相同）

## 0b. 产品经理视角

- **场景：** 在《财富早知道》里说「往期的公告都吃 1/3，别再给我排两条」→
  出一份候选 → 你点头 → 一条安静回执：`记下了 · 《财富早知道》的标准（第 1 版）`。
  第二天王海燕采料、吕老师报题，**排的就是一条公告**。你问「这条规矩哪来的」，
  去「它记得的」看得见：归属《财富早知道》、因为「用户说往期公告都吃 1/3」、09-17、第 1 版。
  过了两天你说「公告还是给两条吧，一条太薄」→ 新版取代旧版 → 管理页上那条写着
  `第 2 版 · 取代了 09-17 那版`，旧版还看得见；点「退回上一版」，**09-17 那版自己回来**。
- **看起来怎么样（都是既有界面的小改）：**
  - 回执：`记下了 · 归《财富早知道》`（一行灰字/一条 toast，不弹窗、不打断）。
  - 「它记得的」每行加两样小字：**归属标签**（`全局` / `财富早知道` / `情报王海燕`）与
    **版本**（`第 2 版`，只在 >1 时出现）；展开或 hover 看「因为什么」。
    行尾在有 `supersedes` 时多一个 `退回上一版`（次要按钮，不是红的）。
  - 归属筛选：`全部 / 全局 / 各项目组 / 各人物`——沿用现有筛选器的样子。
- **空 / 载 / 错态：**
  - 空：一条记忆都没有 → 既有空态，不动。
  - 没有归属 → `全局`（不是空白，也不写 `None`）。
  - 归属指向一个这个版本不认识的 id（老码下线的人）→ 显示 `旧版角色`，**不静默**。
  - 版本链断了（旧版被删过）→ 第 2 版照旧显示，不报错。
- **成功标准：** 组里教一句 → 回执说清归谁；管理页看得见四样（归属 / 因为 / 何时 / 第几版）；
  删掉新版旧版回来；组里换成王海燕说话时，她的手艺在注入里（别人的不在）。
- **明确不做什么：** 自动改归属（只显示，不改）· 跨组喊话 · 标准的审批流 ·
  「出活标明按哪一版」的强制工具化（只靠注入里带版本 + 组规一句话）· 教学候选的批量合并。

## 0c. 架构师视角

- **根因层级：**
  1. `resolve_owner` 把 `zone=standards` 一律判成全局——这是**为工位场景写的**规矩，
     项目组出现后它变成**串味通道**（组里的口径漏进所有工位）。
  2. 可见面是**单个 owner**（`views_for(active, owner)`）——组会话只能二选一，
     要么看得见组、要么看得见手艺。
  3. `frontmatter_for` 只搬 zone/tags/confidence/supersedes，**`rationale` 落盘即丢**。
- **正确的长期默认路径：**
  - 归属判定仍是**一处**（`resolve_owner`），只是它现在**先问会话是不是组**；
  - 可见性仍是**一处判据**（`visible_to`）+ 一处装配（`views_for_any`）；
  - 版本是**链**（`supersedes`），不是状态字段；
  - 出处是**frontmatter 的一个可选字段**（`because`），写入只有候选这一路。
- **边界：** 动 `hermes-memory`（resolve_owner / visible_to_any / views_for_any / topics / frontmatter）·
  `hermes-core::persona`（`memory_owners_for`）· `hermes-reflect`（候选的 owner 路由 + because）·
  `hermes-gui`（注入装配 + inbox 回执 + 管理页四样 + 退回）· `hermes-server`（注入装配同轴）·
  `hermes-cli`（装配同轴）。**不动**：提示词顺序、硬拦、MCP、渠道。
- **如何防复发：**
  ① 组里「标准 → 组」的测试钉死（今天会红）；
  ② 「别人的手艺不许进注入」测试钉死；
  ③ owner 只许引擎两个值（组 id / 桌上成员 id），模型给别的 → 归组（测试钉死）。
- **为何这不是补丁：** 三条都是**通用**修正（归属语义、可见面、出处字段），
  没有一处为《财富早知道》写特判；组仍然只是数据。

---

## Task 1：归属（写侧）—— 组里标准归组、手艺归点名的人

**Files：** `crates/hermes-memory/src/scoped.rs`（`resolve_owner` + tests）

- [ ] **Step 1（先红）**：写测试：组会话 + `zone=standards` → 归**组**（今天是 `None`，会红）；
  组会话 + `asked=桌上成员` → 归**成员**；组会话 + `asked=桌外人` → 归**组**；
  组会话 + `zone=preferences` → **全局**；工位会话行为**逐条不变**。
- [ ] **Step 2**：改 `resolve_owner`：会话是组时按上表；否则原逻辑。
- [ ] **Step 3**：把「组 id / 成员 id」两个概念的判定收在 `hermes_core::team::{get, member}`。

## Task 2：注入（读侧）—— 全局 + 组 + 说话人

**Files：** `hermes-memory/src/memory.rs`（`visible_to_any` / `views_for_any`）·
`hermes-memory/src/topics.rs`（多 owner 的卡视图）· `hermes-core/src/persona.rs`（`memory_owners_for`）·
`hermes-gui/src/commands/chat.rs` · `hermes-server/src/routes/chat.rs` · `hermes-cli/src/commands/chat/mod.rs`

- [ ] **Step 1**：`visible_to_any(m, owners)`——判据仍是 `visible_to`，只加「任一命中」；
  `views_for_any(active, owners)` 是唯一装配口，`views_for` 委托给它。
- [ ] **Step 2**：`persona::memory_owners_for(persona, team, speaker)`：
  组 → `[组, 说话人（若是桌上成员）]`；否则单值。**会话 → 归属的转换只此一处。**
- [ ] **Step 3**：三处装配改用它（GUI / server / CLI），卡面同轴。
- [ ] **Step 4**：测试：组会话看得见「全局 + 组 + 说话人」，别人的一条不给；工位会话不变。

## Task 3：出处与版本 —— 谁说的 / 何时 / 为什么 / 第几版

**Files：** `hermes-memory/src/memory.rs`（`because` 字段）· `hermes-reflect/src/candidate.rs`（写入）·
`hermes-gui/src/commands/memory.rs`（视图）· `ui/.../MemoryPanel.tsx`

- [ ] **Step 1**：frontmatter 加 `because: Option<String>`（可选、跳过空值），候选落盘时写入 `rationale`。
- [ ] **Step 2**：`list_memories` 返回 `owner / ownerName / because / supersedes / version`；
  版本 = 沿 `supersedes` 往前数（只数这条链上的祖先，环就停）。
- [ ] **Step 3**：管理页：每行一个**归属标签** + `第 N 版`（仅 >1）+ `因为…`；筛选器加归属一档。
- [ ] **Step 4**：`退回上一版` = 删掉这条 → 链断 → 上一版回到 active（+ 一句确认，说清会发生什么）。

## Task 4：教学的两条路 —— 提示词里给两个 id

**Files：** `hermes-reflect/src/micro.rs` · `hermes-reflect/src/prompt.rs` · 调用点

- [ ] **Step 1**：反思提示词改成「两条路」：**这份活的标准 → 组 id**；**这个人的手艺 → 说话人 id**；
  关于用户本人的 → 不写 owner。
- [ ] **Step 2**：调用点传两个 id（组 + 说话人；工位会话两者都可能是「工位 id + 无」）。
- [ ] **Step 3**：测试：提示词里有这两个 id；候选带越界 owner → 落盘时归组（Task 1 已钉）。

## Task 5：回执 —— 教完说清归谁

**Files：** `hermes-gui/src/commands/inbox.rs`（返回归属标签）· `ui/src/store/chatStore.ts` · `i18n.ts`

- [ ] **Step 1**：`accept_pending_review` 返回 `{ ownerId, ownerName }`。
- [ ] **Step 2**：回执文案 `记下了 · 归《财富早知道》` / `记下了 · 归「情报王海燕」` / `记下了 · 全局`。

## Task 6：文档 + 台账

- [ ] **Step 1**：`docs/spec/projects.md` §13 回填第 3 期；`docs/spec/personas.md` §5.2 补「组」那一档
- [ ] **Step 2**：`docs/records/README.md` 加一行
- [ ] **Step 3**：回填本记录 §3 / §4

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 1 | 组里的标准不串味 | 组里教一句标准 | 归属是《财富早知道》，不是全局 | ✅ `scoped::on_a_team_standards_belong_to_the_team_and_craft_to_the_named_member` |
| 2 | 手艺跟着人 | 组里对王海燕说「采料先分级」 | 归属是王海燕 | ✅ 同上（点名桌上成员 → 归他） |
| 3 | 判不准归组 | 教一句说不清是标准还是手艺的 | 归组（宁可窄） | ✅ 同上（桌外人 → 组）+ `scoped::the_write_guard_clamps_to_the_views_own_persona_never_to_global` |
| 4 | 用户偏好还是全局 | 组里说「交付一律给 Word」 | 全局 | ✅ 同上（`preferences` 在组里仍是全局） |
| 5 | 手艺看得见 | 接棒给王海燕后看注入 | 全局 + 组 + 王海燕的；别人一条没有 | ✅ `persona::a_team_view_sees_the_team_and_the_one_holding_the_baton` · `gui::chat::visible_memories_keeps_globals_and_own_only` · `channel::a_team_pinned_memory_shows_the_round_it_replaced_and_why` |
| 6 | 说得清出处 | 管理页看那条标准 | 归属 / 因为 / 何时 / 第几版 | ✅ 后端 `memory::because_is_optional_and_never_invented` · `gui::memory::the_owner_label_says_the_table_the_person_or_that_it_is_an_old_role` · 界面**待目视** |
| 7 | 改一版 | 再说一句相反的 | 新版取代旧版，标 `第 2 版` | ✅ `memory::version_counts_the_supersedes_chain` · `candidate::supersedes_and_zone_travel_with_the_candidate` |
| 8 | 退回上一版 | 点退回 | 上一版回来，新版没了 | ✅ 机制 `store::supersedes_filters_active` + `scoped::superseding_an_invisible_memory_never_retires_it`；界面**待目视** |
| 9 | 教完看得见 | 点头一条候选 | 回执说清归谁 | ⚠️ `accept_pending_review` 返回 `AcceptOutcome`（**无单测**，靠目视） |
| 10 | 老规矩没破 | `cargo test --workspace` | 全绿 | ✅ 见下 |

- **自动化：** `cargo fmt --all -- --check` ✅ · `cargo clippy --workspace --all-targets -- -D warnings` ✅ ·
  `cargo test --workspace` ✅ · `npx tsc --noEmit && npm run build` ✅
- **手工（待目视）：** GUI 里教一句、看回执、看管理页四样（归属 / 因为 / 第几版 / 退回上一版）、
  组里换人接棒时注入里手艺跟着换
- **测试结论：** [x] 自动化全绿；[ ] 目视待做（第 6 / 8 / 9 条）

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☐ | 教一句 → 归对人 → 下一期照它干 |
| 开箱即用未破坏 | ☑ | 工位 / 自由对话的归属与注入逐条不变（`scoped::resolve_owner_only_isolates_when_the_session_has_a_persona`、`channel::no_team_leaves_the_companion_prompt_byte_identical`） |
| 本地优先未破坏 | ☑ | 还是 md 文件，加一个可选字段（`because`）；没新增依赖 |
| 测试通过 | ☑ | fmt / clippy `-D warnings` / `cargo test --workspace` / `tsc + vite build` 全绿 |
| 记录完整 | ☑ | 本记录 + spec §13 / `personas.md` §5.1–5.2b + README |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补 | ☑ | 三处通用修正：归属语义 / 可见面 / 出处字段；无组特判 |
| 代码卫生 | ☑ | 装配口只留一处（`views_for_any`）；`cards_for_view` 收进 `cards_for_view_any`，`views_for` 已无调用点 → **删除**（含导出），旧调用点全部改用 |
| 操作与视觉 | ☐ | 回执 / 归属标签 / 第 N 版 / 退回上一版 目视 |
| 第一性原理三步写全 | ☑ | §0-fp |

- **验收人：** 用户
- **结论：** ☐ 待目视
- **遗留项：** 第 4 期（长会话按天归档 + 按需读）· 内联产物卡 · 头像 · 决定的历史版本回看 ·
  「出活标明按哪一版」目前只靠注入带版本 + 组规一句话，不做工具强制

---

## 5. 实施偏差（诚实记录）

原方案（§0c / Task 1–5）之外，实施时多做了三件——都写在这里，不留在暗处：

1. **GUI / server 的记忆工具面从来没有收窄过**（原方案没列这一条）。
   `PersonaToolHost` 只在 CLI 里包了一层；GUI 与 server 直接把 `FsMemoryStore` 交给模型，
   于是「在工位里教一句」也能落成全局。本期补上：三处都用
   `PersonaToolHost::with_owners(inner, store, owners)` 包一层（写侧与注入同轴）。
   这是原方案的**漏项**，不是新增需求——不收窄它，三档归属只在读侧成立。
2. **组会话的 pinned 注入带上「哪一版 · 因为什么」**（`companion_context.rs`）。
   原方案只说管理页看得见；但 §5.1 规矩 3「按最新版干活」要求**模型**看见的是当版，
   所以组会话的 pinned 行渲染成 `[id · 日期 · 取代了上一版 · 因为：…]`。
   工位 / 无组会话逐字节不变（`no_team_leaves_the_companion_prompt_byte_identical` 盯着）。
3. **`because` 字段**：原方案的「出处」写的是界面显示；落盘需要一个字段承载，
   于是 frontmatter 加可选 `because`（老文件缺栏照读，不迁移）。

一处**按规格收敛**的取舍：归属指向**已下线**的角色（小谢 / 小金 / 小乐）时，
标签显示 `旧版角色` 而不是角色名——它们的定义还在仓库里，但名册上没有这个工位，
报一个用户点不过去的名字才是骗人（`gui::memory::the_owner_label_...` 钉住）。

**未做（明确留给后面）：** 归属的**编辑**（只显示，不改）· 跨组喊话 · 标准的审批流 ·
「出活标明按哪一版」的工具强制 · 教学候选的批量合并。
