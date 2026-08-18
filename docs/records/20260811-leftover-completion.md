# 变更记录：审计遗留项全部闭环

| 字段 | 内容 |
|------|------|
| **编号** | `20260811-leftover-completion` |
| **日期** | 2026-08-11 |
| **状态** | **工程通过**（与 `20260811-full-audit-hardening` 合并为一批；产品手测 ⬜） |
| **关联** | `20260811-full-audit-hardening` |

---

## 0. 本批补齐的「遗留」

| 遗留 | 处理 |
|------|------|
| bash OS 沙箱 | macOS `sandbox-exec` seatbelt；Linux `bwrap`（有则用）；否则 `sandbox=none` 诚实标注 |
| WS 长期 `?token=` | `POST /api/v1/ws-ticket` 60s 单次 ticket；Flutter 优先 ticket；legacy token 兼容 |
| ContextSources×3 | GUI/server 删除分叉实现，统一 `hermes-channel::companion_context` |
| Flutter Evolve | server inbox REST + 抽屉「进化收件箱」页（接受/拒绝） |
| 文档 1:1 残留 | project-map / flutter-progress / state 头注释 |

## 1. 工程验证

- bash / companion_context / auth(9) / turn / tools 相关测试通过
- `cargo check -p hermes-gui -p hermes-server -p hermes-cli` 通过

## 2. 产品手测

见主台账 `20260811-full-audit-hardening` §6 + 下文统一测试建议。
