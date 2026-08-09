# 变更记录：上传文档 Phase A（初版）— **已否决 · 由合规方案 supersede**

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-upload-phase-a-markitdown` |
| **日期** | 2026-08-03 |
| **状态** | **已否决**（未达 P0/P1；用户 2026-08-03 选选项 1 纠偏） |
| **负责人** | Grok |
| **关联** | 继任方案：[`20260803-document-import-compliant`](./20260803-document-import-compliant.md) |

---

## 否决原因（对照权威文档）

| 门槛 | 问题 |
|------|------|
| P0 单一二进制 / 开箱即用 | 默认依赖本机 Python `markitdown`（PATH 碰运气） |
| P0 第三条 多入口 | 逻辑仅在 `hermes-gui`；`hermes-server` 无 1:1 |
| P0 第七条 默认路径 | 开发机碰巧有 DevKit 环境 |
| P1 变更流程 | 台账未按模板写全 §2–4；未更新 `records/README.md` |
| P1 质量门槛 | 未跑通 `cargo clippy/test --workspace` 全绿 |
| 用户价值闭环 | 无 GUI 入口，仅 `invoke` |

## 代码处置

- 现有 `crates/hermes-gui/src/commands/upload.rs` **视为实验草稿**，不得标已验收。
- 实施合规方案时：逻辑迁入共享层 + server 1:1 + 捆绑转换器默认路径；**同次清理** GUI 独占实现与过时注释（P0 第九条）。
- 在继任方案验收前：**不得**对外声称「文档上传可用」。

## 可复用部分（经合规方案改造后）

- 目录约定：`uploads/{session_id}/`、只存 MD + meta、成功后删临时原件
- 白名单类型、20MB、错误码语义
- 单元测思路（session 消毒、扩展名、过大拒绝）

细节与正确默认路径见继任方案。
