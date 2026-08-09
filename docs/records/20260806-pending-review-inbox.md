# 变更记录：安静进化 · 待审收件箱

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-pending-review-inbox` |
| **日期** | 2026-08-06 |
| **状态** | **已实施** |

## 产品决策

- **提炼**后台安静跑；**确认**进统一「待审」，默认**不打断**离开对话  
- 质量门：空洞/Care 污染/「见会话记录」不入队  
- 去重：fingerprint  
- `reflect.pop_inbox_on_leave` 默认 false（开则旧弹窗）  

## 实现

| 层 | 内容 |
|----|------|
| `hermes-reflect/inbox.rs` | `pending-review.json` 存取 |
| 会话结束 | `Enqueued { added, total }` |
| GUI | 导航「待审」+ 角标 + InboxPanel |
| 接受/拒绝 | `accept_pending_review` / `reject_pending_review` |

## 用户路径

离开对话 → 可选轻 toast「已加入待审 n 条」→ 侧栏角标 → 打开待审批处理  

## 测试

- [x] inbox gate / fingerprint  
- [x] hermes-gui 编译  
- [ ] dist 需 npm build  
