# 变更记录：记忆「主题卡」蒸馏 —— 取代粗槽位「一槽只留一条」

| 字段 | 内容 |
|------|------|
| **编号** | `20260914-memory-topic-cards` |
| **日期** | 2026-09-14 |
| **状态** | **已验收**（用户 2026-09-14 复看 GUI 主题 tab 后点头） |
| **负责人** | Codex（代行）· 用户拍板 · 用户验收 |
| **关联** | 用户 9/14 讨论「给记忆加一个蒸馏」；上承 `20260913-list-readability-tabs-and-paging` |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：**
  1. 不是「把相似度阈值调低一点」。把「相关」当「重复」，是这次最贵的错。
  2. 不是「再给记忆加一个 summary 字段」。
  3. 不是「上了向量库就解决了」——换尺子不换维度，同一把错尺子。
  4. 不是「卡片就是记忆的浓缩版，所以优先读卡」。**卡是有损的**，把它当条文用比没有它更危险。
- **拆出的真（全部在真实数据上实测，不是推演）：**
  1. **用户要的是「同一主题的多个侧面能被一次看全」，不是「条数变少」。**
     `hermes distill` 在真实库（20 条 active）上：阈值 0.55 → 0 簇；0.30 → 0 簇；
     0.20 → 1 簇且是误合并（「具身智能口径」↔「具身智能文章情节」，相似度 0.22）；
     0.12 → 3 簇，含把「偏爱 vim」和「写文档用短句、先结论」并成一条的垃圾簇。
     库里唯一一组真重复（两条「今天摸鱼」）**早就被合并了**（`mem_e9aa5bb8` 的
     `supersedes` 指向 `mem_ffab34cf`）。剩下的 20 条是互补侧面，不是重复。
  2. **今天真正在丢记忆的不是「没合并」，是「合错了维度」。**
     `living_rules`（`crates/hermes-memory/src/slot.rs:207`）按 7 个粗槽位、
     每槽只留一条。真实数据实测：**20 条 active → 注入只剩 14 条，静默丢 6 条**，
     丢掉的是：「公众号写作思路四原则」「短句、先结论后细节」「普法号方向」
     「轻松幽默闲聊」「具身智能情节」「反网络暴力选题情节」——全是承重条目。
  3. **丢条的根因是槽位算错了东西。** `infer_slot` 把「法务季度工作总结模板」
     判成 `identity`，于是它和「普法号方向」抢同一个位置，后者被挤掉。
     槽位衡量的是「**怎么干活**」（写成品 / 查事实 / 交付 / 口吻…），
     用户要的是「**在说什么事**」（财经内容 / 公众号写作 / 具身智能）。**两把尺子。**
  4. **界面上那份「概览」既不是蒸馏也不是全文。** GUI 注入的 `palace-index.md`
     是代码生成的截断清单（每 zone 只列前 5 条、每条截 80 字、`... (5 more)`）。
     而 LLM 版蒸馏器 `compile_palace_index`（`crates/hermes-reflect/src/compile.rs:61`）
     早已写好，却只有 CLI 的 `/palace compile` 能触发，GUI / 手机路径从来没接。
  5. **记忆里承重的是措辞本身。**「只写作已发生的事实陈述，不做趋势判断」
     「不写保证胜诉/绝对化用语」「具身智能=大脑和AI的部分」——差一个字就是事故。
     所以「用卡」和「用原文」不是同一种用途：卡说**哪里有什么**，原文说**具体怎么写**。
  6. **两处「用户说了不算」今天就存在**（都在注入链上）：
     (a) `crates/hermes-channel/src/context.rs:56` 是 `if palace_index / else if profile /
     else pinned`——只要 palace index 存在，**pinned 的全文注入整段被跳过**；
     GUI 路径永远有 palace index，所以用户 pin 过的条目今天不会被完整注入。
     (b) `list_active()` 会过滤「无用壳」，而 `list_pinned()` 建在它上面，
     于是 **用户 pin 的条目可能被无用过滤静默否决**：实测库里 2 条 pinned
     （onboarding 种子「用户的工作场景：。」）在 `memory list --pinned` 里显示为「无」，
     而 `memory list --all` 里明明带着 ★——两个视图自相矛盾。
- **如何从真推出：**
  - 真 1+3 → 蒸馏的**单位是主题，不是槽位**；粒度由「在说什么事」定。
  - 真 2+3 → 产物必须是**能同时容纳多个侧面的容器**（一张卡），成员全部继续活着，谁都不删。
  - 由「容器」再推 → 卡是**派生视图**：原记忆是唯一真源，卡可重建、可失效。
    LLM 分错主题只影响视图，不动真相 —— 这是敢用模型干这件事的前提。
  - 真 4 → 注入与界面**共用同一份卡**；旧简版索引在**同一位置被替换**（不是再叠一层）。
  - 真 5 → **优先级分层**：pin 的原文 > 卡（索引）> 按需取回的原文；
    涉及产出 / 交付 / 口径 / 数字 / 禁止项，**必须先取原文再动笔**；冲突时原文赢。
  - 真 6 → 本次一并把注入顺序重排（pinned 永远在最前），并让 **pin = 用户说了算**，
    不被派生逻辑或过滤规则静默否决。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 天天打开桌面 GUI 的人（手机 / IM 走同一引擎，自动同等受益）。
- **解决什么痛点：** 记忆越攒越像一堆散话，看不出记住了几个主题；
  教过的承重条目会被引擎静默丢掉（今天在丢 6 条）；pin 过的东西说了不算。
- **用完后用户多得到什么：** 记忆页能切到「主题」看到几张卡、点开看每个主题下的侧面；
  注入给模型的上下文不再丢承重条目；pin 了就在，且看得见。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（整理 → 看到卡；记忆变了界面明说「可能过期」）
  - [x] 不增加无意义确认或噪音（点「整理」即为点头，不再弹二次确认）
  - [x] 主操作一眼能找；空 / 载 / 错态完整
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

### 场景
用户在对话里教了搭子一阵子，回到「它记得的」，觉得乱，
想知道「你到底记住几个主题、每个主题记全了没有」。

### 界面规划（逐屏 · 复用既有控件，不新造一套 UI 语言）

**入口**：右栏 header 里加一个分段控件，两个 tab（复用材料页那套
`inline-flex gap-0.5 p-0.5 rounded-xl bg-app-muted` + `tabBtn`，见
`crates/hermes-gui/ui/src/components/materials/MaterialsPanel.tsx:263`）：

```
它记得的      [ 逐条 | 主题 ]        🔍 搜索    + 新建
```

**tab「逐条」**：与今天**完全一致**（左栏筛选 + 搜索 + 新建 + 10 条/页 + 分页）。
唯一变化：置顶栏不再漏条（见 0c 修正 B）。

**tab「主题」**：不用左栏（卡的筛选语义不成立），整幅宽度给卡片。

- header：左「主题」+ 灰字 `3 张卡 · 覆盖 20 条`；右 `重新整理`（次要按钮）。
  无卡时右侧变成主按钮 `整理成主题卡`。
- 卡片：**标题 / 概括 / 成员数**三段式，概括限 6 行内：

```
┌────────────────────────────────────────────┐
│ 财经内容（中国财富网）                 5 条 ▾ │
│ 只写已发生事实、不做趋势判断；正式稿生成带格  │
│ 式 Word 放桌面；信源优先级…                  │
└────────────────────────────────────────────┘
```

- 点 `5 条 ▾` 展开成员：复用「逐条」那套行渲染（body + zone/时间 + 置顶/删除，
  pinned 带 ★），**不另造第二套行样式**。
- 过期提示：卡片区顶部一行灰字 + 内联按钮「记忆有更新，卡片可能过期 · 重新整理」。
- 空态：Brain 图标 + 标题「还没整理过」+ 主按钮「整理成主题卡」
  + 一行说明「把 20 条记忆按主题收成几张卡，原记忆不变」。
- 载态：「正在整理 20 条记忆…」+ 按钮禁用（防连点）。不做假进度条。
- 错态：「整理失败：<原因>」+「重试」。**不喊红叉**（沿用 9/13 约定）。
- 卡成员掉光：卡上显示「这张卡没有成员了 · 重新整理」——**不自动调 LLM**。

### 文案
tab 名默认 `逐条` / `主题`；实施时可按目视微调，但不得引入违禁词（「聊天」「搭档」）。

### 好走 / 好看
主操作唯一（整理 / 重新整理）；三态齐全；不靠教程能看懂；卡片区留白足、不像调试台。

### 成功标准
用户不靠人教，15 秒内看懂「搭子记住了 3 个主题、每个主题下有哪些条目」；
pin 过的东西一定看得见。

### 明确不做什么
不做拖拽分组、不做卡编辑 / 改名 / 排序、不做自动定时重算、
不删任何原记忆、不改左栏结构、不在记忆页加新的筛选维度。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

### 记忆使用优先级（本次定规 · 核心）

| 级别 | 内容 | 形态 |
|------|------|------|
| 1 | 用户 pin 的原文 | 全文，始终在场，**不被任何摘要分支吞掉** |
| 2 | 主题卡 | 索引：主题 + 概括 + 成员 id |
| 3 | 按需检索到的原文 | `memory_search` / `palace_read_zone` |
| 4 | 无 | 说不知道，**不拿卡的概括当事实** |

- **硬规则 1：** 卡里的硬约束**原文引用、不改写**（数字、「不许」、「必须」保持原措辞）。
- **硬规则 2：** **卡不进 `memory_search` 的结果**——它只说「哪里有什么」，
  不许被当作「用户说过的话」引用。
- **硬规则 3：** 卡与原文冲突 → **原文赢**，并提示重新整理。
- 与技能索引同构：卡之于记忆 = `name + description` 之于 `SKILL.md`（P0 第八条）。

### 根因层级
记忆的组织维度（`hermes-memory`）+ 上下文装配（`hermes-channel`）+ 界面（`hermes-gui`）。
三层都要动，缺一层不成立。

### 正确的长期默认路径
- `hermes-memory/src/topics.rs`（新）：
  `TopicCard { id, title, summary, members: Vec<String>, built_at }`，
  存 `~/.lebi-ai/topics.json`（明文、原子写，形态对齐 `profile.rs` / `palace.rs`）。
  **成员是引用不是归属：一条记忆可被多张卡引用**（「交付方式」同时属于财经带与公众号带）。
- `hermes-reflect/src/topics.rs`（新）：`build_topic_cards(provider, memories)`，
  一次调用生成。硬约束：成员 id 必须存在于输入（幻觉 id 丢弃）、卡数上限 8、
  概括不得发明事实、硬约束原文引用。
- **融合 vs 重洗**（回答「再来一批相似记忆怎么办」）：
  - 默认**融合**：只处理「未归卡的记忆」= active − ∪(所有卡成员)；
    输入 = 未归卡记忆原文 + 各卡（标题/概括/成员 id）；输出 = 并入哪张卡 / 新建卡 / 提议拆卡。
    成本 O(新增条数 + 卡数)，不是 O(全部记忆)。
  - **重洗**（全量重建）三个触发：某卡成员 > 12 条 / 未归卡记忆超阈值 / 用户点「重新整理」。
    重洗后按成员重叠 > 50% 继承旧卡 id 与 `built_at`，界面不会看到卡「换了一批」。
  - 成员 > 12 条时重洗**优先拆卡**：宁可多一张卡，也不要一张卡覆盖二十条侧面而概括只剩空话。
  - **卡会收缩**：成员被 supersede / 删除后自动掉成员，掉到 0 删卡。
  - 过期判定从「记忆集合指纹」改为「**未归卡记忆集合**」：只在真有新记忆没归卡时提示。
- `living_rules` 改「**按槽位分组、一条不丢**」；`hermes-reflect/src/prompt.rs` 的
  反思纪律文案从「同槽位必须 supersede」改为「同槽位可互补；只有真重复才 supersede」。
- 注入：`ContextSources` 的 `palace_index` 字段换成主题卡渲染；
  无卡时回落到 pinned + active 列表。

### 顺带修正的两处优先级错误（今天就在错）
- **修正 A —— pinned 被摘要分支吞掉**：`crates/hermes-channel/src/context.rs:56` 的
  `if palace_index / else if profile / else pinned` 改成「pinned 全文永远在最前」，
  其后才是主题卡 / profile / active 列表。
- **修正 B —— pin 被无用过滤静默否决**：`list_active()` 对
  `is_worthless_for_living` 的过滤**对 `pinned == true` 豁免**
  （pin 的语义就是「用户说了算，永远在场」，代价最多几十字）；
  同时修掉 `memory list --pinned` 与 `memory list --all` 的 ★ 自相矛盾。
  现存 2 条 onboarding 空壳 pin 会因此出现在「置顶」栏里，由用户自行删。

### 与引擎/各入口边界
生成与渲染都在引擎里 → GUI / server / IM / CLI 同时受益，**不为某一个入口写私有逻辑**。

### 安全影响
纯本地明文；除用户自己的 AI API 外不外发；不新增网络面、不新增需安装的组件、
不动 token / TLS / 0600 任何既有约定。

### 如何防复发
渲染三态有测试；成员 id 幻觉有测试；pinned 豁免有测试；
真实数据回归：20 条注入后**必须仍是 20 条可达**，且 pinned 全文在场。

### 为何这不是补丁
它换掉的是**组织维度本身**（槽位 → 主题）与**注入优先级**（摘要可吞 pin → pin 永远第一），
并把旧的简版索引在同一位置替换、清理，不是给旧逻辑叠一层。

---

## 1. 方案（Plan）

- **目标：** 给记忆加一层可重建的「主题卡」蒸馏；原记忆一条不删；
  注入按「pin 原文 > 卡 > 检索原文」分层；pin 不再说了不算。
- **范围：**
  - **做：**
    1. `hermes-memory` 新增 `topics.rs`：结构 + 读写 + 未归卡判定 + 渲染 + 融合/重洗规则。
    2. `hermes-reflect` 新增 `topics.rs`：LLM 生成（融合 / 重洗两种模式，形态对齐 `compile.rs`）。
    3. `living_rules` → 分组不丢条；同步改反思提示词纪律文案。
    4. `hermes-channel/src/context.rs`：修正 A（pinned 永远在最前）+ 注入换成主题卡（stale 诚实标注）。
    5. **修正 B**：`list_active()` 的「无用壳」过滤对 pinned 豁免；`memory list --pinned` 与 `--all` 一致。
    6. GUI 记忆页 `逐条 / 主题` 双 tab + 「整理成主题卡 / 重新整理」+ 三态 + 两条 command。
    7. CLI：`hermes topics`（列出）/ `hermes topics --build`（生成）。
    8. 清理：`build_palace_index_simple` 在**注入处**的用途删除。
  - **不做：**
    1. 不改 `infer_slot` 的分类规则（槽位仍供检索分组；本次只停止用它删条）。
    2. 不自动合并 / 删除任何记忆。
    3. 不做后台定时重算、不做增量重算（融合是手动触发的）。
    4. 不做卡片手工编辑 / 改名 / 拖拽 / 排序。
    5. 不删 `palace-index.md`、`palace_zones` / `palace_read_zone` / `palace_recall`
       —— zone 检索是另一条能力，不属本次。
- **用户路径变化：**
  - 改前：打开「它记得的」= 20 条流水；模型上下文里 20 条只进去 14 条（6 条静默消失）；
    pin 了在「置顶」栏也看不到。
  - 改后：可切到「主题」看到 3 张卡、点开看成员；模型上下文 pin 全文在最前 + 3 张卡，
    20 条全部可检索到；pin 了就在、看得见。
- **技术要点：** `crates/hermes-memory` · `hermes-reflect` · `hermes-channel` ·
  `hermes-gui`（`src/commands/memory.rs`、`ui/src/components/memory/MemoryPanel.tsx`、
  `ui/src/i18n.ts`）· `hermes-cli`（新子命令 + `main.rs`）
- **风险与回滚：**
  - LLM 主题划分不稳 → 纯派生视图，删 `topics.json` 即回落旧行为。
  - 弱化槽位去重纪律可能让记忆变多 → 真重复仍由写入口 dedup（0.55）拦，
    **观察两周**；若反弹，把「同槽位必须 supersede」收敛为「同卡内必须 supersede」。
  - pin 豁免可能让垃圾 pin 常驻提示词 → 界面上可见可删；成本上限几十字。
  - token 变化 → 记录改前 / 改后系统提示实测字数。
- **方案确认：** [x] 用户 2026-09-14 拍板「开工」· 拍板人：用户

---

## 2. 实施（Implement）

### 落地清单（按 crate）

- 新增 `crates/hermes-memory/src/topics.rs`：`TopicCard { id, title, summary, members, built_at }`
  与 `TopicCards`；常量 `MAX_CARDS = 8` / `SPLIT_THRESHOLD = 12` / `UNGROUPED_TITLE = "未归类"`；
  函数 `path` / `load` / `save`（`<数据根>/topics.json`，临时文件 + rename 原子写）/
  `prune`（成员死了就掉，掉到 0 删卡）/ `unassigned`（active − ∪成员）/
  `is_stale`（**过期判定 = 未归卡记忆非空**）/ `render_for_prompt` / `render_from_disk`。
- `crates/hermes-memory/src/palace.rs`：**删** `build_palace_index_simple` / `load_palace_index` /
  `save_palace_index` / `palace_index_path` 与对应旧测试，只留 `group_by_zone` / `get_zone`
  （zone 检索是另一条能力，不属本次）。
- **修正 B**：`store.rs::list_active()` 的「无用壳」过滤对 `pinned == true` 豁免；
  `memory list --pinned` 与 `memory list --all` 的 ★ 不再自相矛盾。
- `slot.rs::living_rules` 改为「只剔无用壳、一条不丢」；单测改成断言同槽位的两条互补记忆都在。
- 新增 `crates/hermes-reflect/src/topics.rs`：`build_topic_cards(provider, active, existing, rebuild)`
  （`rebuild=false` 融合 / `true` 重洗）+ `finalize_cards`（幻觉 id 丢弃、空卡丢弃、id 唯一、
  未覆盖的记忆落到「未归类」）+ `inherit_ids_by_overlap`。
- **删** `compile_palace_index`（`compile.rs`）与 smoke 管线里的 palace index 重建（`micro_run.rs`）。
- `crates/hermes-reflect/src/prompt.rs` 反思纪律：由「同槽位必须 supersede」改成
  「槽位是一种**活**、不是一条事实；同槽位条目通常是互补侧面，全部保留；只有真正取代才 supersede」。
- `crates/hermes-channel/src/context.rs`：字段 `palace_index` → `topic_cards`；
  **pinned 全文块提到最前**，任何索引都不能吞掉它；无卡时才回落 profile / active 列表。
  `companion_context.rs`（GUI/server 共用）同步；`channel.rs` 的 `ServeCtx` 字段改名。
- 各入口接线，全部走同一个函数 `hermes_memory::topics::render_from_disk(&active)`：
  GUI `state.rs` / `commands/chat.rs`、server `routes/chat.rs`、CLI `chat/mod.rs`（两处）/ `agent.rs`。
- GUI 命令层 `commands/memory.rs`：新增 `list_topic_cards` / `build_topic_cards`（`rebuild` 区分融合与重洗）
  与 `TopicCardsView`（含 `stale`）；`main.rs` 注册两条命令。
- CLI 新增 `crates/hermes-cli/src/commands/topics.rs`：`hermes topics` / `--build` / `--rebuild`。
- UI：`MemoryPanel.tsx` 加 `逐条 / 主题` 分段控件（复用材料页 tab 样式），主题页三态 + 卡片三段式，
  成员展开复用「逐条」的行渲染；`i18n.ts` 加 11 个 key（en + zh）。

### 顺带清理（凡旧必清）

- CLI `/help` 里 `palace compile` 那一行删掉——子命令已经不存在，帮助里还写着就是谎。
- 内置技能 `memory-palace` 升到 **0.3.0**：正文不再指「palace index」而指主题卡；
  开头「one living rule per kind of work」改成「同一主题可以有多个互补侧面」。
  `bundled.rs` 的保鲜判据同步收紧（`version == 0.3.0` 且正文不含旧导航句）——
  旧判据只认 0.2.0，若不改，**已装用户永远拿不到新文案**，那就是一次静默不升级。
- `companion.rs` 连续性一段：`a memory-palace index` → `topic cards (主题卡)`。

### 与方案的偏离（四处）

1. 加 `UNGROUPED_TITLE = "未归类"` 兜底卡（方案没有）。不加的话，模型漏掉的记忆在卡视图里不可达，
   而且 build 完仍显示「可能过期」。
2. `CARDS_MAX_TOKENS` 4096 → **16384**。4096 时 `deepseek-v4-flash` 的 reasoning 吃光预算，
   返回 `MaxTokens` 且文本为空，报错是误导性的 `EOF while parsing`。
   同时补显式分支：空文本时报 `model returned no text (stop=…, in=… out=… tokens)`。
3. `render_for_prompt` **不列成员 id**（方案第 145 行写的是「主题 + 概括 + 成员 id」）。
   引擎没有「按 id 取记忆」的工具，注入 id 只是白烧 token；提示词里卡的唯一检索键是**标题**。
   成员 id 仍留在 `topics.json`（UI 展开、`prune`、重叠继承都要用），只是不进提示词。
   测试名即这条约定：`prompt_lists_cards_without_member_ids`。
4. `SPLIT_THRESHOLD`（成员 > 12 建议拆卡）一度是**死常量**；收口时接进 `build_prompt`：
   超阈的卡在提示词里带一行 ⚠ 提示可拆，并加测试
   `a_card_over_the_split_threshold_is_flagged_for_splitting`。

---

## 3. 测试（Test）

| 项 | 命令 | 结果 |
|----|------|------|
| 静态 | `cargo clippy --workspace --all-targets -- -D warnings` | **0 告警** |
| 格式 | `cargo fmt --all --check` | 干净 |
| 全量 | `cargo test --workspace` | **466 passed / 0 failed** |

新增 / 改写的针对性测试：

- `hermes-memory`（49）：`topics.rs` 6 条 —— 一条记忆可被两张卡引用 /
  `nothing_built_is_not_stale` / 提示词不列成员 id / 死成员掉且空卡删 /
  `unassigned_finds_new_memories` / `prune_drops_card_whose_members_all_died`。
- `hermes-reflect`（51）：`topics.rs` 8 条 —— 幻觉 id 丢弃 / 漏掉的记忆落到「未归类」 /
  只含死成员的卡被丢而其余仍可达 / 一条记忆可上两张卡 / 卡数不超上限 /
  重洗按重叠继承 id / 复用的 id 保留 `built_at` / 超阈卡被标记可拆。
- `hermes-channel`（24）：`topic_cards_replace_the_plain_index_but_never_the_pinned_block`、
  `topic_cards_replace_the_flat_index_and_pinned_stays_in_full`——**pin 的全文永远在场**。
- `hermes-skills`（39）：`memory_palace_bundle_parses_and_is_always_active`（新断言：正文必须指主题卡）+
  `auto_install_upgrades_a_protocol_that_still_names_the_palace_index`（0.2.0 旧装必被覆盖）。

真实数据回归（数据根指到 `/Users/aodun/Documents/codeINDEx/test`，22 条 active）：

```
./target/debug/hermes topics --build
# 8 张卡：普法公众号(4) / 具身智能(3) / 财经新闻稿(3) / 交付与工具(3) /
#         沟通偏好(3) / 个人(2) / 未归类(3) / 本人身份(1)
./target/debug/hermes topics
# 卡面与 topics.json 一致；末尾一行为：all 22 active memories are covered.
```

改前那条「20 条 active 注入只剩 14 条」的丢条，今天在卡视图下**不再发生**：
22 条全部被卡引用（含 2 条 onboarding 空壳进的「未归类」）。

---

## 4. 验收（Accept）

- [x] 质量门槛：clippy / fmt / test 全绿（466 / 0）
- [x] 真实数据回归：22 条 active 全部被卡覆盖（`all 22 active memories are covered.`）
- [x] 注入顺序：pinned 全文块在最前，索引吞不掉它（两个 channel 测试）
- [x] pin 不再被静默否决（`list_active` 豁免 + 测试）
- [x] 真机目视（重建二进制后重启 GUI）：`逐条 / 主题` 切换、`8 张卡 · 覆盖 22 条`、
      卡展开后成员行与「逐条」同一套样式、行内 置顶 / 删除 可用
      （截图 `/tmp/gui-topics-cards.png`、`/tmp/gui-topics-expanded.png`）
- [x] 内置技能保鲜在**真机**生效：重建后重启 GUI，磁盘上的副本自动从 0.2.0 升到 0.3.0
      （`<数据根>/skills/memory-palace/SKILL.md`），不是只在单测里成立

**本次没做（诚实清单）：** 卡的手工编辑 / 改名 / 排序；后台定时重算；
把卡喂进 `memory_search` 结果；`palace_zones` 相关的 zone 检索未动。

### 质量门槛（`DEVELOPMENT_RULES.md` §变更流程 · 验收必须全部为是）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 记忆页能看清「记住几个主题、每个主题有哪些侧面」；注入不再丢承重条目；pin 了就在且看得见 |
| 开箱即用未破坏 | ☑ | 未引入数据库 / 常驻中间件 / 额外运行时；`topics.json` 就是一个明文文件 |
| 本地优先未破坏 | ☑ | 卡与记忆都在本机明文；除用户自己的 AI API 外不外发；token / TLS / 0600 未动 |
| 测试通过 | ☑ | clippy 0 告警 · fmt 干净 · `cargo test --workspace` 466 / 0 |
| 记录完整 | ☑ | 本文件 0-fp / 0 / 0b / 0c / 1 / 2 / 3 / 4 / 5 写全；索引已更新 |
| 文档位置 | ☑ | 根目录仍是 P0/P1/P2/P3 四个 md；本次只改 `docs/records/` |
| 非修修补补 | ☑ | 换掉的是组织维度（槽位 → 主题）与注入优先级（摘要可吞 pin → pin 永远第一），旧简版索引在同一位置被替换并删除 |
| 代码卫生 | ☑ | 删 `build_palace_index_simple` / `load_palace_index` / `save_palace_index` / `palace_index_path` / `compile_palace_index`、smoke 里的 palace 重建、`/palace compile` 帮助行；死常量 `SPLIT_THRESHOLD` 接回提示词，不再是装饰 |
| 操作与视觉 | ☑ | `逐条 / 主题` 双 tab、三态齐全（空 / 载「正在整理 n 条记忆…」/ 错「整理失败：…」+ 重试，不喊红叉）；成员行复用「逐条」样式；真机截图见 §5 |
| 第一性原理 | ☑ | §0-fp：拒绝了 4 个类比、6 条拆出的真（全部在真实数据上实测）、逐条推出做法 |

- **验收人：** 用户（拍板「验收掉」）· 工程/目视由 Codex 代跑
- **验收日期：** 2026-09-14
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留项：** `ONBOARDING-SEED-JUNK`（onboarding 写空壳且自带 `pinned: true`，实测 2 条
  「用户的工作场景：。」）——本次只让其可见可删，**修种子另开一条**；
  另有早前登记未拍板的 `SKILL-BASH-BYPASS`（技能在数据根下，模型可绕 `bash` 直改 `SKILL.md`）。

---

## 5. 附注

本次方案阶段的实测命令（只读，仓库无改动）：

```
./target/debug/hermes distill                      # 0.55 → 0 簇 / 20 条 active
./target/debug/hermes distill --threshold 0.30     # 仍是 0 簇
./target/debug/hermes distill --threshold 0.20     # 1 簇（误合并，相似度 0.22）
./target/debug/hermes distill --threshold 0.12     # 3 簇，含垃圾簇
./target/debug/hermes memory list --pinned         # 显示「无」
./target/debug/hermes memory list --all | grep ★   # 却有 2 条 ★（自相矛盾）
```

`living_rules` 丢条实测：临时探针跑真实数据 → `active=20 living=14`（已还原，仓库干净）。

注：CLI 二进制是 `target/debug/hermes`；`target/debug/lebi-AI` 是 GUI。

### 遗留项（不在本次）
- `ONBOARDING-SEED-JUNK`：onboarding 会写空壳并自带 `pinned: true`
  （实测 2 条「用户的工作场景：。」）。本次只让它们可见可删，另开一条修种子。

### 停用但不删的文件
- `<数据根>/palace-index.md`（实测 2026-08-11 那份，1574 字节）。代码里已经没有
  `load_palace_index` / `save_palace_index` / `build_palace_index_simple`，全仓 `rg` 只剩
  `bundled.rs` 新测试里那句「旧文案」字符串。它是**用户数据**，不是我们的文件，
  本次**不删、不迁移**，只记在这里：它已停用，删掉也不影响任何入口。

### 验收截图（重建二进制 + 重启 GUI 之后拍的）
- `/tmp/gui-topics-cards.png` —— 主题 tab：`8 张卡 · 覆盖 22 条`、卡片三段式（标题 / 概括 / n 条）、
  右上「重新整理」。
- `/tmp/gui-topics-expanded.png` —— 展开「普法公众号」后的成员行：与「逐条」同一套行样式
  （正文 + zone 标签 + 置顶 / 删除），卡概括不含成员 id。
