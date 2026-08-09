# 变更记录：工作与陪伴完整方案（蓝图 + 引擎协议 + GUI 对话化）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-work-companion-complete` |
| **日期** | 2026-08-06 |
| **状态** | **已实施**（代码+文档；GUI dist 需 build；故事 A/B 真机待验） |
| **蓝图** | [`../work-companion-solution.md`](../work-companion-solution.md) |

---

## 0. 用户价值

- **谁用：** 个人用户 / 开发者（非律师业务）
- **痛点：** 产品像聊天工具或工程师 agent；缺「共事 + 记得你 + 帮你变好」的完整定义，后续实现易断片
- **用完后：** 有一份封闭蓝图；所有入口身份协议统一；GUI 称「对话」；记忆可带 zone；reflection 知情节/标准

## 0b. 产品经理

- **场景：** 用户打开 GUI/CLI 共事；希望第二次同类事被再认出、交付后有分寸打磨
- **路径：** 改前（helpful assistant / 聊天）→ 改后（work-and-companion / 对话 + 四环协议）
- **成功标准：** 见蓝图 §2.2 故事 E 必过；A/B 有记忆时行为符合协议
- **不做：** 不做律师库；不做话痨默认；不强制新 DB

## 0c. 架构师

- **根因：** 身份与 Continuity/Care 未产品化；GUI/CLI 提示词分叉；记忆全 general
- **正确默认：** `hermes-core::companion` 单一协议源；GUI/Server 对齐 Progressive Disclosure + relevant 记忆；reflection zone 贯通
- **非补丁：** 一次定义全貌 + 共享协议模块，禁止入口各写叙事

---

## 1. 方案

按蓝图 §9：D0–D1、I0–I3、U1–U2。

## 2. 实施

| 项 | 内容 |
|----|------|
| D0 | `docs/work-companion-solution.md` + 本台账 + docs 索引 |
| D1 | P0 v0.2、AGENTS、README、project-map |
| I0 | `crates/hermes-core/src/companion.rs` |
| I1 | GUI/Server `context.rs`：协议 + relevant 记忆 + 去 skill 内联 |
| I2 | CLI `system_prompt.rs` 使用 companion 协议 |
| I3 | `MemoryCandidate.zone`；reflect prompt；CLI/GUI/server accept；micro_apply/distill |
| U1 | GUI i18n 中英对话化 + 欢迎四卡 |
| U2 | 需 `npm run build` 刷新 `ui/dist` |

## 3. 测试

- [ ] `cargo test -p hermes-core`
- [ ] `cargo test -p hermes-gui --lib`（若有）/ compile
- [ ] `cargo test -p hermes-reflect`
- [ ] `cargo check -p hermes-cli -p hermes-server`
- [ ] 中文 i18n 无「聊天」主词
- [ ] 故事 E：GUI 文案为对话（dist 更新后）

## 4. 验收

| 门槛 | 状态 |
|------|------|
| 用户价值 | ✅ 方向与协议封闭 |
| 本地优先 | ✅ |
| 文档位置 | ✅ |
| 代码卫生 | ✅ 旧 helpful assistant 文案替换 |
| 真机 A/B | ⬜ 待用户有记忆会话验证 |

**遗留（已写入蓝图 §5.4，不丢上下文）：** C-SESS / C-CARE-UI / C-INTENSITY / C-EP-TOOL / C-FLUTTER
