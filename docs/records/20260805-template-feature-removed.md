# 变更记录：移除文档模板功能

| 项 | 内容 |
|----|------|
| **编号** | `20260805-template-feature-removed` |
| **日期** | 2026-08-05 |
| **状态** | **已实施**（待用户验收） |
| **关联** | 用户明确拒绝「占位符/填槽」产品形态；历史台账 P0–P2 / v2 / office-delivery 全部作废产品路径 |

## 1. 方案

### 问题

文档模板系统采用 `{{placeholder}}` 槽位 + 编译/填空管线，属于**代码式模板**，不是用户期望的 **AI 自行创作/生成** 方式。用户无法接受，要求**全部删除**该功能。

### 决策

- **删除**模板引擎、工具、GUI 面板、IM 白名单、prompt 引导、专用依赖。
- **保留** `workspace/outputs/` 默认生成目录规则（与模板无关）。
- **保留** 文档导入（markitdown）等非模板能力。
- **不自动删除**用户数据目录 `~/.small-rust-hermes/templates/`（若有）；需用户明示再清。

### 成功标准

1. 代码中无 `template_list/get/save/fill` 工具与 GUI 注册。
2. 侧栏无「模板」入口。
3. 系统提示不再引导 `template_fill`。
4. `cargo check` / 相关 crate 编译通过。

## 2. 实施

### 已删除（此前 + 本记录）

| 层 | 内容 |
|----|------|
| 引擎 | `hermes-tools`：`template.rs`、`document_export.rs`、`docx_fill.rs`；依赖 `docx-rs` / `genpdf` / `zip` |
| 工具注册 | `lib.rs` 中全部 `template_*` 工具 |
| GUI IPC | `commands/templates.rs`、`main.rs` handler |
| GUI UI | `components/templates/`、`TemplatePanel`、nav `templates`、i18n 模板文案 |
| 权限/渠道 | `danger.rs` 中 template_save/delete；channel 白名单 template_* |
| Prompt | gui/server/cli 中 `template_fill` / 模板交付说明 |

### 文档

| 文件 | 处理 |
|------|------|
| `docs/template-system.md` / `template-system-v2.md` | **已删除**（见 `20260806-doc-hygiene-dead-templates`） |
| 历史 P0–P2 / v2 / office 中间台账 | **已删除**（本文件为唯一墓碑） |
| 本台账 | 记录移除原因与范围；勿再实现占位符填槽 |

## 3. 测试

- [x] `cargo check -p hermes-tools -p hermes-gui -p hermes-cli -p hermes-server -p hermes-turn -p hermes-core` **通过**
- [x] 源码无 `template_*` 工具 / `TemplatePanel` / `docx-rs` 引用
- [ ] GUI 真机：侧栏无模板入口（需重启 GUI）

## 4. 验收

- 用户确认：不再需要模板功能；占位符方案不再恢复。
- 可选：用户要求时删除 `~/.small-rust-hermes/templates/`。
