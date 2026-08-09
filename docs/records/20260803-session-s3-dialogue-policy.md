# 变更记录：S3 对话策略（身份纪律 · 停止落盘 · 话术边界）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-session-s3-dialogue-policy` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok（用户「做 S3」） |

---

## 0. 用户价值

- 不再被错误人设（律师等）强绑定
- 点停止后有明确提示，半截内容尽量落盘
- 模型不空喊不存在的 UI 能力

## 0b. 成功标准

1. System prompt 含记忆/身份谦逊纪律 + 话术边界
2. 取消时：Cancelled 事件；有部分文本则落盘并带「已停止」标记
3. 前端对停止友好（Toast + 斜体提示，非 Error: cancelled）

## 0c. 不做

- 改 reflection；完整多语言 system prompt；改记忆检索算法

---

## 2. 实施

| 项 | 实现 |
|----|------|
| 身份/话术 | GUI + server `context.rs` system 段；CLI `build_session_system` 前缀 |
| 停止落盘 | `hermes-turn` 累积 delta，cancel 时 flush 部分 assistant + `*(Generation stopped.)*` |
| 停止 UI | `TurnEvent::Cancelled` → `ChatStreamEvent::Cancelled`；i18n + toast |

## 3. 测试

| # | 结果 |
|---|------|
| cargo check turn/gui/server/cli | **通过** |
| hermes-cli context tests | **通过**（改 empty 断言） |
| npm run build | **通过** |
| 真机：停止按钮 | **通过**（用户） |
| 真机：人设不乱贴 | **通过**（用户） |

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
