# 变更记录：RULE-ACCEPT — auto_accept 默认值对齐 P0（候选必须用户确认）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-rule-accept-default-false` |
| **日期** | 2026-08-03 |
| **状态** | **已验收** |
| **负责人** | Codex（用户委托） |
| **关联** | `docs/records/20260803-pre-dev-review-rules.md`（缺口 RULE-ACCEPT） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 个人用户 / 开发者，CLI chat 与所有入口
- **解决什么痛点：** 默认会把 medium+ 置信度的记忆**未经确认**写入知识库，用户不知情
- **用完后用户多得到什么：** 知识库只含用户确认的内容；想自动学习可显式开启（配置或 GUI 开关）
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（默认无自动写入；开启后提示「auto-learn on」）
  - [x] 不增加无意义确认或噪音（只对新增候选走确认）
  - [x] 高频路径比改前更快或更省心（默认路径符合产品承诺）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在 chat 中对话，micro-reflection 后台运行
- **路径变化：** 改前（medium+ 记忆静默落盘，仅打一行「💾 learned」）→ 改后
  （候选进入确认流程；用户确认才落盘；显式开启 auto-learn 才自动写）
- **成功标准：** 全新配置（无 `[reflect]` 段）下不会自动写任何记忆；`hermes init` 模板
  `auto_accept_memories = false`；默认配置测试断言为 false
- **明确不做什么：** 不改 GUI/server 的显式开关语义（仍是用户主动开启）；不改 skill 的
  确认流程（skill 一直必须人工）；不动 distill

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 配置默认值层（`ReflectConfig::default()` 与 `default_config_toml()` 双源）
- **正确的长期默认路径：** P0 第一条「候选必须用户确认」= 系统默认路径；
  `auto_accept_memories` 是用户 opt-in 增强，默认必须 `false`；两处默认源保持一致
- **与引擎/各入口边界：** CLI 读同一份 `Config`；GUI/server 只是透传该配置项，无需改
- **安全影响：** 无（不触密钥/网络）；行为更保守
- **如何防复发：** 默认配置模板测试断言 `auto_accept_memories == false`；P1 新增规则
  「配置默认值与权威行为一致」已锁定
- **为何这不是补丁：** 把「默认自动写」从双源（serde 默认 false 与 Default impl true 分叉）
  收敛为单一正确默认，并加回归断言

---

## 1. 方案（Plan）

- **目标：** `auto_accept_memories` 默认 `false`
- **范围：** 做：`crates/hermes-llm/src/config.rs` 两处默认源 + 文档注释 + 模板测试断言。
  **不做：** 不改 GUI/server/IM 代码；不引入新配置项
- **用户路径变化：** 见 0b
- **技术要点：** `ReflectConfig::default()`（行 192）与 `default_config_toml()`（行 348）
  同步改；`default_config_template_loads` 测试加断言
- **风险与回滚：** 低；行为更保守；git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** 改 3 处：Default impl `true→false`；模板 `true→false`；字段文档注释
  补「默认 false（用户 opt-in）」；测试加默认断言
- **关键路径/文件：** `crates/hermes-llm/src/config.rs`
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 全新配置默认不自动写 | 加载无 `[reflect]` 段配置 | `auto_accept_memories == false` | 通过 | |
| 2 | init 模板默认不自动写 | 解析 `default_config_toml()` | 模板值为 `false` | 通过 | |
| 3 | 显式开启仍可用 | 配置 `auto_accept_memories = true` | 解析为 `true` | 通过 | 既有行为 |

- **自动化：** `cargo test -p hermes-llm` + `cargo clippy --workspace --all-targets -- -D warnings`
- **手工：** 不适用（配置层）
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（列出）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 默认路径符合「用户批准」核心价值 |
| 开箱即用未破坏 | ✅ | 配置兼容（serde default false 与 Default 一致） |
| 本地优先未破坏 | ✅ | 未触数据/密钥 |
| 测试通过 | ✅ | llm 单测 + clippy 全绿 |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | 双默认源收敛 + 回归断言 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ✅ | 仅 3 行 + 注释 |

- **验收人：** Codex（用户委托）
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** 无

---

## 5. 附注

- 默认值分叉根因：`#[serde(default)]` 使字段级默认 false，而 `ReflectConfig::default()`
  写死 true——加载无 `[reflect]` 段的配置时走后者，导致「文档说不自动写、代码默认自动写」
