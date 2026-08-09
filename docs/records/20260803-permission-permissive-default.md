# 变更记录：执行权限常态放行 + 特别危险才授权说明

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-permission-permissive-default` |
| **日期** | 2026-08-03 |
| **状态** | **已验收** |
| **负责人** | Grok（用户拍板 1–4 后实施） |
| **关联** | 用户确认：write/edit 开 · bash 常态开 · memory_save 开 · skill_install 始终确认 |

---

## 0. 用户价值

- **谁用：** GUI / CLI 本机用户
- **痛点：** 写文件、普通 shell、存记忆等常态操作频繁打断
- **用完后：** 常态自动跑；仅特别危险弹窗且写清「为什么危险」
- **好用性：** 无额外运行时；步骤更少；硬边界仍在

## 0b. 产品经理

- **场景：** 日常改 workspace、跑 `ls`/`cargo test`、存记忆 — 不问；`rm -rf` / `sudo` / 远程装技能 — 要授权
- **成功标准：**
  1. write/edit/memory_save/普通 bash 无弹窗
  2. bash 黑名单（rm -rf、sudo、curl\|sh…）弹窗且有 reason
  3. skill_install 始终弹窗
  4. 确认文案含「原因」
- **不做：** 放开 IM 白名单里的 bash/write；不改 reflection 候选默认不写入

## 0c. 架构师

- **根因：** 「危险 = 工具级 requires_confirmation」过粗
- **默认路径：** `assess_confirmation`（hermes-turn/danger）= 工具标志 + bash 命令风险 + skill_install 强制；config deny/allow 仍优先；workspace 硬边界不变
- **防复发：** 单测覆盖常态开与 bash/skill 闸门；P0/P1 条文同步收紧「危险」定义

---

## 1. 方案

用户拍板：1 是 2 是 3 开 4 是。

## 2. 实施

| 项 | 改动 |
|----|------|
| 策略 | 新增 `hermes-turn/src/danger.rs` |
| 回合 | Prompt 时走 `assess_confirmation`；ConfirmRequest/事件带 `reason` |
| 工具标志 | write/edit/bash/memory_save/skill_create → false；skill_install/delete、memory_delete、subagent 仍 true |
| UI | ConfirmModal 展示 why；i18n 中英 |
| 权威 | P0/P1 文案对齐「特别危险才确认」 |

## 3. 测试

| # | 结果 |
|---|------|
| `cargo test -p hermes-turn danger` | **通过**（7） |
| `cargo check` gui/server/cli | **通过** |
| 真机：普通 write/bash 不弹 | **通过** |
| 真机：`rm -rf` / skill_install 弹且有原因 | **通过** |

## 4. 验收

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留：** 无
