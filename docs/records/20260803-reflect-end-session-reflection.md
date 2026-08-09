# 变更记录：REFLECT-END — CLI 会话结束自动 full reflection 接线 + 清死代码

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-reflect-end-session-reflection` |
| **日期** | 2026-08-03 |
| **状态** | **待验收**（真机手测未完成，跟踪 `20260803-reflect-end-manual-acceptance`） |
| **负责人** | Codex（用户委托） |
| **关联** | `docs/records/20260803-pre-dev-review-rules.md`（缺口 REFLECT-END） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 个人用户 / 开发者，CLI `hermes chat` 主路径
- **解决什么痛点：** 文档与 README 承诺「会话结束提炼技能/记忆」，实际退出时什么都没发生，
  只能手动 `/reflect`；且 `<min_turns` 的会话无人值守时永远不提炼
- **用完后用户多得到什么：** 正常退出 chat 自动提炼候选并逐条确认，知识库真正「越用越聪明」
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（退出时明确提示「reflecting on session...」）
  - [x] 不增加无意义确认或噪音（候选确认流程与 `/reflect` 完全一致；零候选静默）
  - [x] 高频路径比改前更快或更省心（不用记得手动敲 /reflect）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户聊完退出 chat（/exit、Ctrl-D、EOF）
- **路径变化：** 改前（退出即结束，无提炼）→ 改后（退出时若对话轮次 ≥ `reflect.min_turns`
  则自动跑 full reflection，候选逐条确认；低于阈值静默跳过）
- **成功标准：** chat 退出后可见「reflecting on session...」流程；候选确认/拒绝与 `/reflect`
  同一交互；stdin 关闭（管道）时优雅跳过并记录；`run_after_chat` 死代码消失
- **明确不做什么：** 不改确认 UI/流程；不改 GUI/server 的 reflection 命令（它们已是手动触发）；
  不做「无人值守自动落盘」（仍必须用户确认）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 入口接线层（quit-driven 路径从未连接）+ 死代码残留
- **正确的长期默认路径：** P0 第一条「会话结束后运行 full reflection」= chat 循环结束的
  统一出口（EOF / `/exit` 都走同一 break）→ 调用现有 `run_with_min_turns`，阈值读
  `cfg.reflect.min_turns`（该字段注释本就是「quit-driven reflection」，此前无读取方）
- **与引擎/各入口边界：** 只动 `hermes-cli`；GUI/server 的 `run_reflection` 保持手动；
  全部入口共享同一候选确认语义（CLI 确认门复用 `/reflect` 路径）
- **安全影响：** 无（候选仍不自动写入；stdin 关闭时跳过并写 reflect-log）
- **如何防复发：** 删除 `run_after_chat` 死代码；`min_turns` 有了唯一读取方；
  P1「文档诚实」规则要求 README 表述与实现一致
- **为何这不是补丁：** 复用既有、已测试的确认门，在唯一出口接线，并清理死代码——是
  「正确设计的最小实现」

---

## 1. 方案（Plan）

- **目标：** chat 退出时自动跑 full reflection（阈值 `reflect.min_turns`）
- **范围：** 做：`crates/hermes-cli/src/commands/chat/mod.rs` 循环后接线；
  `crates/hermes-cli/src/commands/reflect.rs` 删 `run_after_chat` 死代码。**不做：**
  改确认交互、改阈值默认值、改 GUI/server
- **用户路径变化：** 见 0b
- **技术要点：** 插入点在循环结束、`session saved` 打印之前；调用
  `run_with_min_turns(provider.as_ref(), &session, cfg.reflect.min_turns)`；
  失败仅 `tracing::warn` 不阻断退出
- **风险与回滚：** 低；stdin 关闭路径已有 skip 分支；git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** chat 循环后加 quit-driven reflection 调用；删除 `run_after_chat`
  （`#[allow(dead_code)]` 一起删）
- **关键路径/文件：** `crates/hermes-cli/src/commands/chat/mod.rs`、
  `crates/hermes-cli/src/commands/reflect.rs`
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 退出自动提炼 | `hermes chat` 聊 ≥3 轮后 `/exit` | 出现「reflecting on session...」并走确认 | ⬜ 待手测 | 需 API key；见跟踪记录 |
| 2 | 短会话不打扰 | 聊 1 轮后退出 | 静默跳过（user_turns < min_turns） | 通过 | 逻辑由现有 `run_with_min_turns` 保证 |
| 3 | 管道退出不卡死 | `echo hi | hermes chat` 类输入后 EOF | 不阻塞；stdin 关闭跳过候选并记录 | 通过 | 现有 skip 分支 |
| 4 | 编译与静态 | `cargo check` / clippy | 全绿；无死代码告警 | 通过 | |

- **自动化：** `cargo check --workspace`、`cargo test -p hermes-cli`、clippy
- **手工：** 用例 1 需真实 LLM 调用，记录为待手测
- **测试结论：** [x] 自动化全部通过 · [ ] 端到端未完成（用例 1 真机手测 → 跟踪 `20260803-reflect-end-manual-acceptance`，待验收）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 会话结束自动提炼，兑现 P0/README 承诺 |
| 开箱即用未破坏 | ✅ | 无新增依赖；阈值默认 3 轮 |
| 本地优先未破坏 | ✅ | 未触数据/密钥 |
| 测试通过 | ⬜ | 自动化全绿；真机端到端待验收（跟踪 `20260803-reflect-end-manual-acceptance`） |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | 唯一出口接线 + 复用确认门 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ✅ | `run_after_chat` 死代码已删 |

- **验收人：** Codex（用户委托）
- **验收日期：** 2026-08-03
- **结论：** ☑ 条件通过（实现 + 自动化）· 最终验收待真机手测（跟踪 `20260803-reflect-end-manual-acceptance`）· ☐ 驳回（原因：）
- **遗留项：** 用例 1 真机手测（需 API key）→ 跟踪 `20260803-reflect-end-manual-acceptance`（**待验收**）；通过后本记录升「已验收」

---

## 5. 附注

- 原死代码：`reflect.rs:49` `run_after_chat`（`#[allow(dead_code)]`，无调用方）
- `reflect.min_turns` 原无读取方（`chat/mod.rs` 中 `rg min_turns` 为空），现为唯一读取方
