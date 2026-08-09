# 变更记录：GUI 壳层 Phase A（设计 token · 会话常驻 · Welcome · Chat 观感）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-shell-phase-a` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok（用户委托 · 选项 1 开工） |
| **关联** | 前端审查；P0 好用/高效；律师版 `20260725-gui-shell-ia` 可复用壳层思路 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 个人用户 / 开发者，桌面 GUI 主交付面
- **解决什么痛点：** GUI 像灰白开发者原型——会话在非聊天页消失、空态无引导、无品牌与层次、主次导航平铺
- **用完后用户多得到什么：** 打开即像成品 AI 客户端；任意面板可切会话；新会话有欢迎与示例；视觉统一、主路径清晰
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更快或更省心（会话常驻、欢迎一键开聊）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 打开 GUI → 新建/切换会话 → 在记忆/技能/设置间往返仍能回聊天 → 空会话知道怎么问
- **路径变化：**
  - 改前：侧栏会话仅 chat 可见；空态一句灰字；顶栏品牌+token；六项导航平铺；消息无层次
  - 改后：会话列表常驻+搜索；顶栏=会话标题；空态 Welcome+示例；主导航/高级分层；统一色板与卡片层次
- **成功标准：**
  1. 在记忆/技能/设置页侧栏仍见会话，点击进入该会话对话
  2. 对话顶栏显示当前会话标题（草稿为「新聊天」）
  3. 无消息时中间为 Welcome（价值一句话 + ≥3 示例 prompt）
  4. MCP / Reflect 在「高级」折叠下；聊天/记忆/技能/设置为主导航
  5. 全站主色/边框/表面来自 token，非各面板随意 gray/blue
- **明确不做什么：** 不改引擎/reflection 协议；不复制律师行业面板；不引入重型 UI 库；不做主题切换 UI（B 期）；不做 Toast 全套（B 期）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** UI 信息架构（`activePanel` 卸载会话列表）+ 无 design system + 空态双路径
- **正确的长期默认路径：** 应用导航与会话上下文分离展示；Chat 壳统一（有消息列表 / 无消息 Welcome + 单一 Composer）；视觉 token 单一来源
- **与引擎/各入口边界：** 仅 `hermes-gui/ui` 呈现层；不改 Tauri commands / store 业务契约（可加 composer prefill）
- **安全影响：** 无
- **如何防复发：** token 在 `index.css`；会话列表不依赖 `activePanel===chat`；`loadSession`/`newSession` 时 `setPanel("chat")`
- **为何这不是补丁：** 一次收敛壳层默认路径与视觉底座，而非各组件散改 class

---

## 1. 方案（Plan）

- **目标：** Phase A 壳层 + tokens + 会话常驻 + Welcome + Chat 观感
- **范围：**
  - 做：`index.css` tokens；Sidebar；ChatView/Welcome/InputArea/MessageBubble/StreamingBubble；i18n；App 壳
  - **不做：** 引擎、Toast 系统、主题设置项、MCP 配置 GUI、记忆/技能业务重构
- **用户路径变化：** 见 0b
- **技术要点：** Tailwind v4 `@theme`；React 组件；zustand 可增 `composerPrefill`
- **风险与回滚：** UI 源码 git 回滚；需 `npm run build` 后 `scripts/run-gui.sh`
- **方案确认：** [x] 用户选「1 直接开工阶段 A」· 2026-08-03

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `index.css`：`@theme` 设计 token（表面/主色/强调/阴影/字体）
  2. `components/common/ui.tsx`：Button / EmptyState / 共享 class 片段
  3. Sidebar：品牌区 + 新会话主按钮 + **会话列表常驻** + 搜索 + 删除确认 + 主导航/高级折叠
  4. ChatView：顶栏=会话标题；无会话/无消息统一空态；Welcome 示例；浮动 Composer
  5. WelcomeScenes / MessageBubble / StreamingBubble / InputArea / Confirm / SessionEnd 视觉收敛
  6. `uiStore.composerPrefill`：欢迎卡填入输入框；i18n en/zh 补齐
- **关键路径/文件：**
  - `crates/hermes-gui/ui/src/index.css`
  - `crates/hermes-gui/ui/src/components/common/ui.tsx`
  - `crates/hermes-gui/ui/src/components/layout/Sidebar.tsx`
  - `crates/hermes-gui/ui/src/components/chat/*`（含 WelcomeScenes.tsx）
  - `crates/hermes-gui/ui/src/store/uiStore.ts`、`i18n.ts`、`App.tsx`
- **偏离方案处：** 无（删除确认为轻量 popover，未做全站 Toast）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 会话常驻 | 进记忆页 | 侧栏仍有会话列表 | **通过** | 用户验收 |
| 2 | 点会话回聊天 | 非 chat 页点会话 | 进入对话且 panel=chat | **通过** | 用户验收 |
| 3 | 空态 Welcome | 新会话 0 消息 | 见标题+示例卡 | **通过** | 用户验收 |
| 4 | 示例开聊 | 点示例 | 填入输入框 | **通过** | 用户验收 |
| 5 | 高级导航 | 折叠 MCP/Reflect | 默认可折叠 | **通过** | 用户验收 |
| 6 | 前端构建 | `npm run build` | 通过 | **通过** | 2026-08-03 |

- **自动化：** `cd crates/hermes-gui/ui && npm run build` ✅
- **手工：** 用户真机确认「通过」
- **测试结论：** [x] 全部通过 · [ ] 有已知问题

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 壳层+观感主路径 |
| 开箱即用未破坏 | ✅ | 未改引擎/配置 |
| 本地优先未破坏 | ✅ | |
| 测试通过 | ✅ | build + 用户手测 |
| 记录完整 | ✅ | |
| 产品+架构两视角齐全 | ✅ | |
| 非修修补补（默认路径正确） | ✅ | 会话常驻+token 单一来源 |
| 代码卫生 | ✅ | 旧侧栏会话卸载逻辑已删除 |

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留项：** Phase B（Toast、主题切换、首启 Key banner、记忆/技能面板视觉对齐）——未开工，另立台账

---

## 5. 附注

审查结论见会话：无 design system、会话随 panel 卸载、空态弱、导航平铺。
