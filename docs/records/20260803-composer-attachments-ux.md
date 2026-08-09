# 变更记录：Composer 附件体验（拖入 · 气泡卡 · 状态）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-composer-attachments-ux` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03「通过」） |
| **负责人** | Grok |
| **关联** | 文档导入已验收；用户选「做 2」；捆绑 markitdown 待打包测试 |

---

## 0. 用户价值

- **谁用：** GUI 聊天用户，导入判决书/合同后对话
- **痛点：** 只能点 📎；发送后消息是生硬 `[attachments]` 文本；转换中状态弱
- **得到：** 拖入文件、composer chip、气泡附件卡、转换中/失败更清晰
- **好用性：** 不增运行时；步骤可感知；少噪音

## 0b. 产品经理

- **路径：** 拖入/选择 → chip 显示 → 转换中 → 发送 → 气泡见附件卡（非原始清单为主视觉）
- **成功标准：** 拖入高亮；多文件 chip；气泡解析 attachments 块并卡片化；正文不重复刷路径墙
- **不做：** 正式 FileRef ContentBlock 协议；图片多模态；Finder 打开（可后置）

## 0c. 架构师

- **根因：** UI 层未消费已有路径清单语义
- **默认路径：** 仍发文本清单给 Agent（兼容）；展示层 `parseAttachmentsBlock` 拆分
- **边界：** 仅 hermes-gui/ui；不改引擎 import
- **非补丁：** 统一附件展示组件，composer/气泡共用

## 1. 方案确认

[x] 用户「做 2」· 2026-08-03

## 2. 实施

- `utils/attachments.ts`：解析/格式化 `[attachments]`、错误文案、扩展名校验
- `AttachmentCards.tsx`：composer / 用户气泡共用 chip
- `InputArea`：拖入高亮、多文件 pending/ready/error chip、toast 净化
- `MessageBubble`：用户消息拆分正文与附件卡（Agent 仍收完整清单）
- i18n：dropHint / attachUnsupported / inputHint
- **拖入修复：** Tauri 默认 `dragDropEnabled` 会抢走 OS 拖放，HTML5 `drop` 收不到文件 → `tauri.conf.json` 设 `dragDropEnabled: false`，改走前端 File API

## 3. 测试

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | tsc | 通过 | **通过** |
| 2 | 拖入（dragDropEnabled=false） | 可用 | **通过** |
| 3 | 多文件含空格名 | 气泡外全部显示 | **通过**（修 \S+ 解析） |
| 4 | 附件在气泡外 | 常见 Chat 布局 | **通过** |

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过
- **遗留：** 无（发布捆绑 markitdown 仍为「待测试·打包后再测」）
