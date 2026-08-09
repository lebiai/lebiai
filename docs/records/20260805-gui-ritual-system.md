# 变更记录：GUI 全站仪式感与视觉统一系统

| 字段 | 内容 |
|------|------|
| **编号** | `20260805-gui-ritual-system` |
| **日期** | 2026-08-05 |
| **状态** | **已验收** |
| **负责人** | agent + 用户 |
| **验收人** | 用户 |
| **验收日期** | 2026-08-06 |
| **验收口令** | 「仪式通过」 |
| **关联** | `20260805-gui-ritual-motion-ux`（P0 启幕/token）；`20260805-gui-ritual-visibility`（欢迎页 Reflect CTA 已纠偏撤回） |

---

## 0. 用户价值

- **谁用：** 个人用户 / 开发者，主交付面 **桌面 GUI**
- **解决什么痛点：**
  - 界面整体偏冷、像功能堆砌，缺少「同一产品」的呼吸感
  - 进化价值（本地、你批准、越用越懂你）只在文案里，不在**整段使用体验**里
  - 曾错误把仪式绑在「反思面板」上，与用户目标（开聊 / 被记住）无关
- **用完后：**
  - **更好用：** 打开→空首页→对话→被记住/完成，全程气质一致、好懂
  - **更高效：** 动效服务注意与确认，不挡输入、不塞假步骤
  - **更能进化：** 在「真正被写入 / 真正完成」时感到掌控，而不是被推去高级功能
- **好用性自检：**
  - [x] 无数据库 / 无额外运行时
  - [x] 不强制教程；启幕可 Skip
  - [x] 不增加无意义确认
  - [x] 不自动写入记忆/技能（P0）

---

## 0b. 产品经理视角

### 用户眼里的产品（不是引擎模块）

| 用户意图 | 产品时刻 | 应有的感觉 |
|----------|----------|------------|
| 第一次打开 | 启幕 | 可信、本地、我做主 |
| 空白新聊天 | 首页舞台 | 漂亮、活、一点就聊 |
| 正在对话 | 消息 / 流式 / 工具 | 流畅、有生命、不吵 |
| 我说「记住…」 | 被接住 | 成功有回声（非黑盒） |
| 偶尔整理知识 | 记忆/技能侧栏 | 同一套 UI，不是另一产品 |
| 换会话 / 告一段落 | 切换与收束 | 轻、干净，非任务弹窗 |

**反思 / Reflect：** 高级、可选能力；**禁止**出现在新聊天 C 位或四卡上方当主 CTA。

### 路径变化（系统目标）

| 改前 | 改后 |
|------|------|
| 局部有动效/启幕，整体仍拼凑 | 全站 **一套** 光、圆角、时长、主色/点缀语义 |
| 仪式≈反思 Accept | 仪式 = **用户时刻**（启幕、首页、对话、写入成功、切换） |
| 首页塞引擎概念 | 首页只服务「开始」+ 品牌氛围 |

### 成功标准（可观察）

1. 新聊天空态：**无**「打开反思」类引擎入口；四卡意图清晰  
2. 全站主按钮/卡片/空态/侧栏 active 态同一 token 与 motion 阶梯  
3. 「被记住」成功（`memory_save` 或 UI 创建记忆）有统一成功反馈（可用落印符号，文案用户语言）  
4. 对话流：新消息入场、流式区微动效，不拖慢输入  
5. 启幕仍可 Skip；设置可重看  
6. `prefers-reduced-motion` 降级；亮暗色一致  
7. 验收：`scripts/run-gui.sh` / npm build + dist  

### 明确不做什么

- 不把 Reflect 做成首页主路径  
- 不积分、排行、强制打卡、音效（本阶段）  
- 不静默自动写入并庆祝  
- 不引入第二套设计系统 / 第三主色 / 多个 motion 库  
- 本系统方案**不**默认做 Flutter/CLI 动效实现（语义可后续对齐）  
- 不改引擎 reflection 协议默认值  

---

## 0c. 架构师视角

### 根因

- 缺 **全站设计系统扩展**（token + 组件族 + 时刻表），只有点状功能  
- 实现者用「Accept 在 Reflect」反推产品入口 → **路径倒置**

### 正确默认路径

```
tokens (index.css)
  → motion/* + ritual/* + ui.tsx 扩展
  → 各 Surface 只组合，不私写魔法数字
  → 时刻钩子：启幕 / 空首页 / 消息流 / memory 成功 / 会话切换
```

| 层 | 职责 |
|----|------|
| **Token** | 色、圆角、阴影、`--motion-*`、`--ease-*` |
| **语义** | Primary=工作台；Accent=「被写下/进化确认」等高潮时刻 |
| **组件** | AmbientStage、MotionCard、RitualMark、消息入场、空态 Hero |
| **时刻** | 钩在真实用户结果上，不钩「请去某面板」 |

### 边界

- 仅 `crates/hermes-gui/ui`（必要时极薄状态）  
- 引擎：`memory_save` / accept 已有；前端挂成功反馈  
- 与 server：DTO 不因仪式分叉  

### 防复发

- 禁止业务页「再塞一个高级功能条」  
- 新 UI 过统一检查清单（见下）  
- 改完清旧：删误导入口与死文案 key  

### 为何不是补丁

先 **系统契约（视觉+时刻）**，再分阶段落地页面，而不是继续给 Reflect 加按钮。

---

## 1. 方案：全站时刻表与分阶段

### 1.1 视觉语义（冻结）

| 语义 | 用途 |
|------|------|
| **Primary 蓝** | 导航、主 CTA、用户消息、日常卡片 |
| **Accent 紫** | 仅：启幕契约强调、**写入成功/被记住** 高潮、micro 候选条（若出现） |
| **中性** | 正文、边框、侧栏底 |

### 1.2 动效语义（冻结 · 纯 CSS/WAAPI）

| Token | 用途 |
|-------|------|
| motion-fast ~150ms | hover、按钮 |
| motion-base ~260ms | 入场、面板 |
| motion-ritual ~500–1100ms | 写入成功高潮 |
| ease-out-soft | 默认曲线 |
| reduced-motion | 关位移与循环 |

### 1.3 用户时刻 → 反馈（产品语言）

| 时刻 | 反馈 | 禁止 |
|------|------|------|
| 首次打开 | 全屏启幕（已有，可打磨） | 多步强制教程 |
| 空新聊天 | Ambient + 四卡 + 品牌 | 推销 Reflect/MCP |
| 发出首句 / 新消息 | 轻 fade-up | 全屏动画 |
| 流式输出 | 区稳定、微指示 | 闪屏 |
| **被记住成功** | 统一成功高潮（落印可复用） | 要求用户懂「反思」 |
| 打开记忆/技能面板 | 同壳、同空态、同入场 | 另一套皮肤 |
| 切换/新建会话 | 内容区轻过渡 | 假 loading |
| 会话收束（可选） | 轻 toast/过渡 | 「请完成反思任务」口气 |

### 1.4 实施阶段

#### 阶段 A — 纠偏与骨架统一（优先 · 本方案确认后第一刀）

- [x] 移除欢迎页 Reflect CTA（已做）  
- [x] 审计并去掉其余「首页/主路径引擎黑话」（欢迎仅四卡用户语言；Reflect 在 Advanced 侧栏）  
- [x] 统一空态：`EmptyState`（icon+title+desc+action+tone）用于 Memory / Skill / Reflect / MCP  
- [x] `PanelShell` 统一 Reflect / MCP 页壳；Memory / Skill 用 `ui.header` + `ui.page`  
- [x] 侧栏 active / 会话项 / header 对齐 `ui.navItem*`、`ui.sessionActive` + motion-fast  
- [x] i18n：`*emptyTitle` / MCP 标题副标题 / Reflect 软文案（Extract / 提炼，非引擎推销）  

#### 阶段 B — 对话过程「活」起来

- [x] 用户/助手消息入场 fade-up（**仅新条**：session 切换 seed 历史不播；`msg-enter`）  
- [x] 流式画布 `stream-enter` 单次入场；思考/工具 `fold-panel` grid 折叠 + motion token  
- [x] 发送/停止 `btn-press`（active scale）；Button 统一 `motion-fast` 色变 

#### 阶段 C — 「被记住」自然路径

- [x] `memory_save` 成功或 GUI 创建记忆成功 → `notifyRemembered`（落印 + 用户语言 toast）  
- [x] 记忆侧栏新条目 `mem-highlight` + `highlightMemoryId`（可从聊天写入后点开看到）  
- [x] **不**引导「去反思才能记住」；微反思 auto-accept 仅 seal + 既有 toast  

#### 阶段 D — 启幕与空首页质感打磨

- [x] 启幕：glass 面板 + rich Ambient + mark ring；Skip 仍在；批准文案去引擎黑话  
- [x] 空首页：`AmbientStage rich`、四卡间距/padding/暗色对比、mark ring primary  
- [x] 老用户时段问候 `welcome.return*`（非 Reflect）  

#### 阶段 E — 收束与高级能力（后置）

- [x] 会话切换：`session-enter` 内容区轻过渡；面板切换 `panel-enter`  
- [x] 会话收束文案用户语言（可选、非任务口吻）；后台 chip 中性  
- [x] Reflect：secondary CTA、Advanced 标签、切换会话清结果；token 一致  

### 1.5 统一检查清单（任何 UI PR）

1. 是否只用 `app-*` 与 motion token？  
2. 是否避免第三主色 / 私有动画时长？  
3. 是否用户语言（无「请打开某某引擎面板」当主路径）？  
4. 是否 reduced-motion 安全？  
5. 是否改完清旧？  
6. 是否 npm build dist 验收？  

### 1.6 风险

| 风险 | 缓解 |
|------|------|
| 又做成功能促销条 | 清单第 3 条；代码评审拒 Reflect 首页入口 |
| 动效过量 | 时长上限；主路径只 base/fast |
| 范围膨胀 | 严格按 A→B→C→D→E；每阶段单独验收勾选 |

### 1.7 方案确认

- [x] 方向：系统性统一，不围着反思（用户 2026-08-05 同意）  
- [x] 阶段 A 实施启动（用户「做 A」）  

---

## 2. 实施

- **状态：** **阶段 A–E 完成**（2026-08-05）  
- **阶段 A 已落地：**
  - `ui.tsx`：`EmptyState`（icon/tone）、`PanelShell`、`ui.sessionActive` 左边条、nav/header tokens  
  - 空态统一：`MemoryPanel` / `SkillPanel` / `ReflectPanel` / `McpPanel`  
  - Reflect CTA 后经阶段 E 改为 **secondary**（非 accent；非主路径主色抢戏）；空态 `tone="neutral"`  
  - i18n en/zh：empty titles、MCP title/subtitle、Reflect 用户语言（Extract / 提炼）  
- **阶段 B 已落地：**
  - `ChatView`：`knownMsgKeys` + `enteringKeys` — 仅新 turn `msg-enter`；历史 seed 静默  
  - 流式：`stream-enter` 挂载一次；`MessageBubble` Process/Tool 用 `fold-panel`  
  - `InputArea` 发送/停止 `btn-press`；CSS token 在 `index.css`  
  - `prefers-reduced-motion` 关闭 msg-enter / fold transition / btn-press  
- **阶段 C 已落地：**
  - `utils/remembered.ts`：`notifyRemembered` / `parseSavedMemoryId`（450ms 批处理）  
  - 流式 `toolUseResult`：`memory_save` 成功 → 落印 + toast  
  - GUI `create_memory` → 同上 + `highlightMemoryId`  
  - Memory 卡片 `mem-highlight`（accent 高潮语义）；auto-accept 加 seal  
  - 文案：Remembered / 已记住（非引擎黑话、无 Reflect 引导）  
- **阶段 D 已落地：**
  - `AmbientStage`：`rich`（双 blob + vignette）+ 既有 accent  
  - `OnboardingRitual`：毛玻璃主面板、落印 ring、层次加强；Skip 保留  
  - `WelcomeScenes`：rich 舞台、四卡更深间距；`returnGreetingKey` 时段问候  
  - CSS：`ambient-vignette` / `ritual-mark-ring*`  
- **阶段 E 已落地：**
  - `ChatView`：`session-enter` keyed by `activeSessionId`  
  - `App`：`panel-enter` on panel swap  
  - Reflect 安静 secondary CTA + 会话切换清结果  
  - SessionEnd 文案/视觉降调；Done 用 primary（非 accent 高潮误用）  
  - reduced-motion 覆盖 session/panel enter  
- **下一步：** 无默认后续阶段；P1/P2 缺口见 §3.3，按需另开台账  
- **验收命令：** `cd crates/hermes-gui/ui && npm run build`；`scripts/run-gui.sh` 目视  

---

## 3. 测试（按成功标准 §0b + 阶段清单）

### 3.1 自动化 / 结构验收（2026-08-05 agent）

| # | 用例 | 步骤 | 期望 | 结果 |
|---|------|------|------|------|
| A1 | 新聊天无 Reflect 主 CTA | 静态：`WelcomeScenes` 无 `setPanel('reflect')` / 无「打开反思」 | 仅四卡用户语言 | **通过** |
| A2 | 空态同源 | Memory/Skill/Reflect/MCP 使用 `EmptyState` | 同源 API | **通过** |
| A3 | PanelShell / ui shell | Reflect+MCP=`PanelShell`；Memory/Skill=`ui.header`+`ui.page` | 同 token 族 | **通过** |
| A4 | 侧栏 active | `ui.navItem*` / `ui.sessionActive` 用 primary | 非 accent 导航 | **通过** |
| A5 | i18n 对称 | en/zh key 数量一致 | 291=291 | **通过** |
| B1 | 新消息入场 | `msg-enter` + knownMsgKeys seed | 仅新条 | **通过（代码）** |
| B2 | 流式/折叠 | `stream-enter` + `fold-panel` | 有 | **通过（代码）** |
| B3 | 发送停止 | `btn-press` | 有 | **通过（代码）** |
| C1 | memory_save 钩子 | `toolUseResult` → `notifyRemembered` | 有 | **通过（代码）** |
| C2 | GUI 创建记忆 | `create_memory` → `notifyRemembered` + highlight | 有 | **通过（代码）** |
| C3 | 无 Reflect 引导 | `remembered.ts` 注释与实现不跳转 Reflect | 有 | **通过** |
| D1 | 启幕 Skip | `OnboardingRitual` Skip + glass/rich | 有 | **通过（代码）** |
| D2 | 空首页 rich | `AmbientStage rich` + return greeting | 有 | **通过（代码）** |
| D3 | 设置可重看 | Settings `replayOnboarding` | 有 | **通过** |
| E1 | 会话切换 | `session-enter` keyed sessionId | 有 | **通过（代码）** |
| E2 | 面板切换 | App `panel-enter` | 有 | **通过（代码）** |
| E3 | Reflect 安静 | secondary CTA + Advanced + 清结果 | 有 | **通过（代码）** |
| V1 | npm build + dist | `npm run build`；`dist/index.html` 存在 | 构建成功 | **通过** |
| V2 | 无第二 motion 库 | 无 framer-motion / gsap 等 | 纯 CSS | **通过** |
| V3 | auto_accept 默认 | 配置默认 `false` | 非静默默认写入 | **通过** |

### 3.2 成功标准对照（§0b）

| # | 标准 | 代码结论 | 用户目视 |
|---|------|----------|----------|
| 1 | 新聊天空态无 Reflect 引擎入口；四卡清晰 | **满足** | **通过** |
| 2 | 全站 token + motion 阶梯 | **基本满足**（残余见 §3.3 P1/P2） | **通过** |
| 3 | 被记住统一反馈 | **满足** | **通过** |
| 4 | 对话入场不拖输入 | **满足** | **通过** |
| 5 | 启幕可 Skip；设置可重看 | **满足** | **通过** |
| 6 | reduced-motion + 亮暗色 | CSS 有降级 | **通过**（用户确认气质） |
| 7 | run-gui / npm build + dist | **build 通过** | **通过** |

### 3.3 验收后仍可知的非阻塞偏差（不阻碍已验收）

| 优先级 | 缺口 | 说明 |
|--------|------|------|
| P1 | **Settings 未用 PanelShell** | A 范围原仅 Memory/Skill/Reflect/MCP；Settings 自有布局 |
| P2 | **第四欢迎卡「聊完之后」** | 用户语言、不打开 Reflect；可另开优化 |
| P2 | **micro 条 accent 仍可再降** | 符合进化时刻语义 |
| P2 | **技能 GUI 新建无落印** | C 仅记忆；技能 Accept 已有落印 |

### 3.4 手工验收清单

1. [x] 新建空聊天：无「打开反思」；四卡可点填入输入框  
2. [x] 设置 → 重看欢迎：启幕可 Skip；层次正常  
3. [x] 记忆 / 技能空态：同源空态风格  
4. [x] 发消息：新条轻入场；历史切换不闪全表动画  
5. [x] 「请记住：…」：落印 + toast；记忆列表高亮（若打开）  
6. [x] 切换会话：内容区轻过渡；无假 loading  
7. [x] Advanced → 提炼：安静 secondary；非首页主路径  
8. [x] 暗色主题：首页 / 启幕 / 记忆可接受  
9. [x] （可选）系统「减少动态效果」：纳入用户气质确认  

- **自动化结论：** 结构 + build **全部通过**  
- **手测结论：** **用户「仪式通过」（2026-08-06）**  
- **测试结论：** [x] 自动化通过 · [x] 用户手测通过 · 残余 P1/P2 不阻塞  

---

## 4. 验收（Accept）

- **验收标准（权威）：** §0b 成功标准 1–7 + §1.5 统一检查清单 + §3.4 手测  
- **代码侧：** A–E 实施项已勾选；自动化/结构验收 **通过**（2026-08-05）  
- **产品侧：** **已验收** — 用户 2026-08-06 口令「仪式通过」  
- **验收人：** 用户  
- **验收日期：** 2026-08-06  

| 检查清单 §1.5 | 结果 |
|---------------|------|
| 1 只用 app-* 与 motion token | **通过**（dark:violet-* 为 accent 配套） |
| 2 无第三主色 / 私有动画库 | **通过** |
| 3 用户语言 / 无主路径引擎推销 | **通过**（Reflect 仅 Advanced） |
| 4 reduced-motion 安全 | **通过**（代码降级 + 用户确认） |
| 5 改完清旧 | **通过** |
| 6 npm build dist | **通过** |

---

## 5. 附注

**产品一句话：**  
冷静工作台 + 全站同一呼吸；高潮给「被写下 / 被完成」，不给「请去高级面板」。

**验收口令建议：** 用户完成 3.4 后回复「仪式通过」→ 将本台账状态改为 **已验收** 并填验收人/日期。
