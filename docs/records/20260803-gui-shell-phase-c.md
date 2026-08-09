# 变更记录：GUI Phase C（进化审阅 UI + 聊天可靠性）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-shell-phase-c` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok（用户确认 C1+C2） |
| **关联** | 前端再审查；Phase A/B |

---

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（进化确认 + 日常对话）
- **痛点：** 候选审阅像旧原型/硬编码英文；接受失败静默；首条消息标题不更新；流式时离开无提示
- **收益：** 进化路径可读可审有反馈；会话标题正确；流式阻塞可感知

---

## 0b. 产品经理

- **成功标准：**
  1. ProposedSkill + ReflectionReview 中英 i18n + token 样式
  2. 接受/拒绝技能记忆有 Toast；失败不丢卡片
  3. 首条消息后侧栏标题更新（含默认标题变体）
  4. 消息列表稳定 key
  5. 流式中新建/切换/关窗有 Toast 说明
- **不做：** 附件、键盘捷径全套、虚拟列表、MCP 配置 GUI

---

## 0c. 架构师

- **根因：** 进化 UI 未跟壳层；title 硬编码英文；key=index；isStreaming 静默 return
- **默认路径：** 公共 `isDefaultTitle`；toast 统一反馈；accept 先 invoke 成功再 onChange
- **非补丁：** 一次收敛进化呈现 + 聊天可靠性

---

## 2. 实施

- **摘要：**
  1. `utils/sessionTitle.ts`：`isDefaultTitle` / `deriveSessionTitle`
  2. `chatStore`：首条消息用 isDefaultTitle 更新标题；流式离开 Toast；accept skill 成功才移除 + Toast
  3. `ChatView`：稳定 messageKey；共用 sessionTitle
  4. `App`：关窗流式时 Toast
  5. `ProposedSkillModal`：i18n + token
  6. `ReflectionReview`：token + Toast + 失败不删卡片
  7. Reflect / SessionEnd：evolutionHint 统一文案
- **偏离：** 无

---

## 3. 测试

| # | 用例 | 结果 |
|---|------|------|
| 1 | `npm run build` | **通过** |
| 2 | 首条消息侧栏标题 | **通过**（用户） |
| 3 | 流式点新建 → Toast | **通过**（用户） |
| 4 | 接受/拒绝候选 Toast | **通过**（用户） |
| 5 | 提议技能弹窗中文 | **通过**（用户） |

---

## 4. 验收

| 门槛 | 状态 |
|------|------|
| 用户价值 / 双视角 / 非补丁 / 卫生 | ✅ |
| 测试 | ✅ build + 用户手测 |

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留：** C3/C4（复制代码块、快捷键、删技能确认、ErrorBoundary 等）——未开工
