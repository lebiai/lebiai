# 变更记录：GUI 壳层 Phase B（Toast · 主题 · 首启 Key · 面板对齐）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-shell-phase-b` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok（用户委托 · Phase B） |
| **关联** | [20260803-gui-shell-phase-a](./20260803-gui-shell-phase-a.md) 遗留项 |

---

## 0. 用户价值

- **谁用：** 个人用户 / 开发者，桌面 GUI
- **痛点：** 无操作反馈；不能选深浅色；未配 Key 时聊天页无引导；记忆/技能等面板仍像旧灰后台
- **收益：** 成功/失败有 Toast；主题 system/light/dark；未配 Key 横幅一键去设置；副面板与 Phase A token 一致
- **好用性：**
  - [x] 不需要额外运行时
  - [x] 步骤可感知
  - [x] 横幅可关闭当次
  - [x] 主题/语言即时生效

---

## 0b. 产品经理

- **场景：** 保存设置 / 删会话 / 首启未配 Key / 夜间使用 / 浏览记忆技能
- **路径：** 改前（静默/仅系统暗色/聊天才失败/面板花）→ 改后（Toast、可设主题、横幅、统一卡片）
- **成功标准：**
  1. 设置保存 / 删除会话等有 Toast
  2. 设置内主题三项，即时切换且落盘 `[ui] theme`
  3. 无 API Key 时聊天区横幅 → 去设置
  4. 记忆/技能/MCP/设置/反思用 app-* token
- **不做：** 完整 appConfirm 框架；MCP 编辑 GUI；Key 热重载进 LLM runtime

---

## 0c. 架构师

- **根因：** 无全局反馈层；dark 仅媒体查询；Config 无 theme/has_api_key；副面板未跟 Phase A
- **默认路径：** Toast 单例 host；`[ui] theme` + `html.dark` class；`hasApiKey` 只读；`get_config` 优先读盘
- **边界：** GUI + hermes-llm UiConfig + server config 1:1
- **防复发：** `@custom-variant dark`；get_config 读盘避免写后内存陈旧
- **非补丁：** 契约扩展一次到位，非 localStorage 旁路主题

---

## 1. 方案

- **做：** toast + ToastHost；UiConfig.theme；ConfigView.hasApiKey/uiTheme；主题设置；SetupBanner；副面板视觉；get_config 读盘
- **不做：** Flutter 同步；进程内热替换 provider

---

## 2. 实施

- **实际改动摘要：**
  1. `hermes-llm`：`UiConfig.theme` 默认 `system`
  2. `hermes-gui` / `hermes-server` config：`uiTheme`、`hasApiKey`；update 写 `theme`；get 优先 `Config::load_default`
  3. UI：`utils/toast.ts`、`ToastHost`、`theme.ts`、`SetupBanner`、`uiStore` 主题/Key
  4. Settings 主题选择 + Toast；Sidebar 删除 Toast
  5. Memory/Skill/MCP/Reflect/Settings 视觉对齐 token
  6. `index.css`：`@custom-variant dark` class 策略
- **关键路径：** 见 crates/hermes-gui/ui/src/**、config.rs、hermes-llm UiConfig
- **偏离方案处：** 无

---

## 3. 测试

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | `npm run build` | 通过 | **通过** |
| 2 | `cargo test -p hermes-llm` | 通过 | **通过** |
| 3 | `cargo check -p hermes-gui -p hermes-server` | 通过 | **通过** |
| 4 | 设置主题 light/dark/system | 即时切换 | **通过** | 用户验收 |
| 5 | 无 Key 横幅 | 显示并可进设置 | **通过** | 用户验收 |
| 6 | 保存设置 / 删会话 | Toast | **通过** | 用户验收 |
| 7 | 记忆/技能面板 | 与 chat 视觉一致 | **通过** | 用户验收 |

- **测试结论：** [x] 全部通过

---

## 4. 验收

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | |
| 开箱即用未破坏 | ✅ | |
| 本地优先未破坏 | ✅ | theme 进 config.toml 0600 |
| 测试通过 | ✅ | build/test + 用户手测 |
| 记录完整 | ✅ | |
| 双视角 | ✅ | |
| 非补丁 | ✅ | |
| 代码卫生 | ✅ | |

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留项：** 改 API Key 后 LLM runtime 仍可能需重启（既有限制，文案已说明）
