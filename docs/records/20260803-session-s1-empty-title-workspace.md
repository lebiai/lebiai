# 变更记录：S1 会话空草稿清理 · 标题持久化 · workspace 律师残留隔离

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-session-s1-empty-title-workspace` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」；含草稿不落盘） |
| **负责人** | Grok（用户「做 S1」） |
| **关联** | 对话记录审查 |

---

## 0. 用户价值

- 侧栏不再堆「空新聊天」
- 会话标题有意义且刷新后仍在
- 寒暄「你好啊」不当最终标题
- 通用 Hermes 工作区不再被律师文件带偏人设

## 0b. 成功标准

1. meta-only 会话不出现在列表；启动/列表/新建时可清理磁盘空会话
2. 首条有效用户消息后 meta.title 落盘；list 优先读 title
3. 短寒暄跳过，取下一条有效文本
4. 启动时 workspace 顶层律师特征文件迁入 quarantine

## 0c. 不做

- thinking 不落盘、usage 写入、tool 折叠（S2）
- 递归清理 uploads/ 下用户主动上传的材料

---

## 2. 实施

| 项 | 实现 |
|----|------|
| Meta.title | `SessionMeta.title: Option<String>` |
| 智能标题 | `is_trivial_user_text` / `derive_title_from_messages` |
| 写 title | `hermes_store::update_session_title`；GUI/server `send_message` 后 |
| 空会话 | `purge_empty_sessions`；**草稿不写盘**；`new_session` 复用空草稿；列表仅有内容会话 |
| 列表 | 过滤无用户文本；优先 meta.title 否则派生；按 mtime |
| workspace | `quarantine_lawyer_workspace_files` 启动时顶层文件 |
| 附带 | `new_session` provider 用 `config.default_provider` |

## 3. 测试

| # | 结果 |
|---|------|
| hermes-core lib tests | **14 通过** |
| hermes-store lib tests | **通过**（含 update_session_title） |
| cargo check hermes-gui/server | **通过** |
| npm run build | **通过** |
| 真机：空草稿消失 / 标题 / quarantine | **通过**（用户） |
| 真机：连点新建不堆会话；有内容才进列表 | **通过**（用户） |

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留：** S2（thinking/usage/tool 折叠）
