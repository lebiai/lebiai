# 变更记录：生成物默认目录 `workspace/outputs/`

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-workspace-outputs-default` |
| **日期** | 2026-08-03 |
| **状态** | **已验收** |
| **负责人** | Grok |
| **关联** | 用户确认：1 outputs · 2 编辑不迁入 · 3 用户点名路径优先 |

---

## 0. 用户价值

- **谁用：** 所有入口用户（GUI / CLI / server）
- **痛点：** 生成文件散落 workspace，找不到
- **用完后：** 凡「生成一份…」默认在 `outputs/`；路径清晰可预期

## 0b. 产品（已确认）

| # | 决策 |
|---|------|
| 1 | 文件夹名：`outputs` |
| 2 | 编辑已有文件：**不**改道到 outputs |
| 3 | 用户指定路径：**优先听用户** |

## 0c. 架构

- 常量：`hermes_core::WORKSPACE_OUTPUTS_DIR`
- 纪律：system prompt（GUI/server/CLI）+ `write` 工具描述
- 模板：`template_fill` 本就写 `outputs/`
- 启动：确保 `workspace/outputs` 目录存在
- **非硬编码拦截 write 路径**（避免误伤合理路径）；默认靠契约与提示

---

## 2. 实施

| 文件 | 改动 |
|------|------|
| `hermes-core/paths.rs` | `WORKSPACE_OUTPUTS_DIR` |
| `hermes-tools/write.rs` | 描述中的默认规则 |
| `hermes-gui/context.rs` / `hermes-server/context.rs` | Generated files 节 |
| `hermes-cli/.../system_prompt.rs` | 同上 |
| `hermes-gui/state.rs` | create_dir_all outputs |

## 3. 测试

| # | 结果 |
|---|------|
| cargo check hermes-gui/cli/server | **通过** |
| 真机：生成默认 outputs | **通过** |

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留：** 无
