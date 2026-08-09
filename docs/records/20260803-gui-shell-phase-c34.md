# 变更记录：GUI Phase C3/C4（生产力 + 健壮性）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-shell-phase-c34` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok（用户委托 C3+C4） |
| **关联** | Phase C；前端再审查 |

---

## 0. 用户价值

- **C3：** 代码可复制；删技能/记忆有确认；⌘N 新会话 / Esc 关层；会话按时间分组
- **C4：** 渲染错误不白屏；会话列表加载失败可感知可重试；改 API Key 强提示重启

## 0b. 成功标准

1. Markdown 代码块有「复制」
2. 删技能/记忆需确认
3. ⌘/Ctrl+N 新建会话；Esc 关闭技能提议 / 会话结束审阅 / 删除确认
4. 会话列表「今天 / 昨天 / 更早」
5. ErrorBoundary + sessions 错误重试
6. 保存含 API Key 时 Toast 明确需重启对话能力

## 0c. 不做

- 虚拟列表（无量级痛点，仍后置）
- Provider 进程内热重载（仅文案）
- MCP 配置 GUI

---

## 2. 实施

| 项 | 路径 |
|----|------|
| 代码复制 | `MarkdownContent.tsx` → MessageBubble / StreamingBubble |
| 删除确认 | `ConfirmPopover.tsx` → 会话/技能/记忆 |
| 快捷键 | `App.tsx` ⌘N / Esc |
| 时间分组 | `sessionTime.ts` + Sidebar |
| ErrorBoundary | 侧栏 + 主区 |
| sessions 错误 | `chatStore.sessionsLoading/Error` + Sidebar 重试 |
| API Key | 保存后 `toast.apiKeyRestart` |

---

## 3. 测试

| # | 用例 | 结果 |
|---|------|------|
| 1 | `npm run build` | **通过** |
| 2 | 代码块复制 | **通过**（用户） |
| 3 | 删技能确认 | **通过**（用户） |
| 4 | ⌘N / Esc | **通过**（用户） |
| 5 | 今天/昨天分组 | **通过**（用户） |

---

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留：** 虚拟列表；API Key 热重载（需引擎层）——未开工
