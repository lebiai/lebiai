# 变更记录：修复质量门槛回归（测试编译 / clippy / fmt / 环境依赖测试）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-fix-quality-gates` |
| **日期** | 2026-08-06 |
| **状态** | **已验收** |
| **负责人** | Agent（Codex 会话） |
| **关联** | 本仓库全仓深度学习验收时发现的质量门槛问题 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 开发者 / 协作者（Agent 与 CI）
- **解决什么痛点：** 全仓质量门槛（P1 强制）当前不绿：
  1. `cargo test --workspace` 编译失败（`hermes-gui` 测试引用未声明的 `serde_yaml`）
  2. `cargo clippy --workspace --all-targets -- -D warnings` 失败（`hermes-turn` 一处 match、`hermes-reflect` 一处 sort_by）
  3. `cargo fmt -- --check` 失败（未提交工作区整体未格式化）
  4. `hermes-tools` 一个测试依赖真实 markitdown sidecar（本机/CI 环境碰巧才有），出现过瞬时失败，CI 上则永远跳过、从未真正覆盖 data_bin 转换路径
- **用完后用户多得到什么：** 开发者可以依赖「fmt/clippy/test 全绿」判断改动安全；CI 能确定性覆盖文档导入转换路径；消除环境运气依赖（P0 第七条）。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（命令行为与 P1 质量门槛一致）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更快或更省心（CI 不再跳过关键路径）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** Agent/开发者按 P1「验收前 fmt+clippy+test 必须全绿」执行质量门槛，当前直接红灯，无法判断交付健康度。
- **路径变化：** 改前：三条门槛命令红灯；文档导入测试依赖真实机器环境（未装 sidecar 则跳过、装了但瞬时故障则红）。改后：三条门槛全绿且确定性复现。
- **成功标准：** `cargo fmt -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 三命令全绿。
- **明确不做什么：** 不改产品行为；不重装/修复本机 markitdown 环境（那是机器环境问题，代码层面以确定性测试替代环境依赖）；不动未提交工作区里的功能逻辑。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  1. 依赖声明缺失（`hermes-gui` 测试模块用 `serde_yaml` 但 Cargo.toml 未声明）→ 配置层
  2. clippy 新规则触发（`match_like_matches_macro` / `unnecessary_sort_by`）→ 代码风格层
  3. 工作区未跑 rustfmt → 工程基线层
  4. 测试与环境耦合（真实 sidecar 探测 + 转换）→ 测试隔离层
- **正确的长期默认路径：**
  1. 测试需要的 crate 一律走 `[dev-dependencies]`（工作区已统一 `serde_yaml` 版本，声明即用）
  2. 能用 `matches!` / `sort_by_key(Reverse)` 表达的写法不绕弯（回归由 `-D warnings` 兜底）
  3. 任何改动合入前先 `cargo fmt`（CI 已含 fmt 检查，以 CI 行为为准）
  4. 单元测试用**假 sidecar**（tempdir 内可执行脚本：`--version` 返回成功、`src -o dest` 产出 Markdown），确定性覆盖 `resolve_converter → run_markitdown_blocking → import_document` 全链路；不再探测真实用户数据目录
- **与引擎/各入口边界：** 不动 `hermes-core` 协议与各入口接线；只修 crate 自身的测试/依赖声明/风格。
- **安全影响：** 无（不改运行时代码路径；测试用临时目录，不触碰用户数据）。
- **如何防复发：** CI 已含 fmt/clippy/test 三步（`.github/workflows/ci.yml`），绿后即自动兜底；测试不再依赖用户目录内容。
- **为何这不是补丁：** 每处都是根因修复——缺依赖补声明、lint 用规范写法、测试与环境解耦为可复现的假件。

---

## 1. 方案（Plan）

- **目标：** 质量门槛问题修复，全仓 fmt/clippy/test 全绿。
- **范围：**
  - 做：`crates/hermes-gui/Cargo.toml` 加 `serde_yaml` dev-dependency；`crates/hermes-turn/src/lib.rs` C-CARE 判定改 `matches!`；`crates/hermes-reflect/src/inbox.rs` 排序改 `sort_by_key(Reverse)`；`crates/hermes-tools/src/document_import.rs` 环境依赖测试改假 sidecar 确定性测试（unix）；全仓 `cargo fmt`。
  - 不做：不重装本机 markitdown；不改产品运行逻辑；不整理未提交工作区其他功能改动。
- **用户路径变化：** 无（纯工程/测试修复）。
- **技术要点：** dev-dependency 用 workspace 版本；假 sidecar 为 `#!/usr/bin/env bash` 脚本，`--version` 与 `src -o dest` 两种调用形态；脚本用 `r##"..."##` raw string（内含 `"#` 序列，`r#"..."#` 会提前终止）；测试 `#[cfg(unix)]` 防护。
- **风险与回滚：** 低；均为独立小改动，可单独 revert。
- **方案确认：** [x] 已对照 P0/P1（第七条「禁止修修补补」、第九条「改完即清旧」）· 2026-08-06 · Agent

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `crates/hermes-gui/Cargo.toml`：新增 `[dev-dependencies] serde_yaml = { workspace = true }` → 测试模块可解析 `serde_yaml::Mapping`
  2. `crates/hermes-turn/src/lib.rs`：`produced_deliverable` 双层 match 改为 `matches!`（行为不变，消除 `match_like_matches_macro`）
  3. `crates/hermes-reflect/src/inbox.rs`：`sort_by(|a,b| b.created_at.cmp(&a.created_at))` → `sort_by_key(|b| Reverse(b.created_at))`
  4. `crates/hermes-tools/src/document_import.rs`：删除依赖真实 sidecar 的 `import_csv_via_data_bin_when_present`，替换为 `#[cfg(unix)] import_csv_via_fake_data_bin_sidecar`（tempdir 假脚本，确定性覆盖 data_bin 全链路）
  5. 全仓 `cargo fmt`：把未提交工作区（含本次改动）格式化为 rustfmt 基线，满足 CI fmt 门槛
- **关键路径/文件：** 上述 5 处。
- **偏离方案处：** 无（方案外新增：`inbox.rs` clippy 与全仓 fmt，均为「把问题都修复掉」范围内的既有门槛红灯，已一并修）。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | GUI 测试可编译可运行 | `cargo test -p hermes-gui` | context 等 5 测试通过 | **通过** | 原先编译失败 |
| 2 | 工具循环无 lint 违规 | `cargo clippy --workspace --all-targets -- -D warnings` | 全绿 | **通过** | |
| 3 | 文档导入转换路径被确定性覆盖 | `cargo test -p hermes-tools --lib document_import` | 假 sidecar 用例通过、不再依赖真实环境 | **通过** | |
| 4 | 格式基线 | `cargo fmt -- --check` | 无 diff | **通过** | |
| 5 | 全仓回归 | `cargo test --workspace` | 全部通过 | **通过** | 唯一 ignored 为既有 Feishu doc-test（需网络），非本次引入 |

- **自动化：** 上述 5 项全绿。
- **手工：** 无（纯工程/测试修复，无用户可见行为变化）。
- **测试结论：** [x] 全部通过 · [ ] 有已知问题

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 开发者/CI 可依赖质量门槛 |
| 开箱即用未破坏 | ☑ | 无运行时行为变化 |
| 本地优先未破坏 | ☑ | 数据仍在本地明文；未新增出站 |
| 测试通过 | ☑ | fmt/clippy/test 全绿 |
| 记录完整 | ☑ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ☑ | 见 0b/0c |
| 非修修补补（默认路径正确） | ☑ | 根因修复，见 0c |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | 旧环境依赖测试已删除，未留双轨 |

- **验收人：** Agent（Codex 会话）
- **验收日期：** 2026-08-06
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留项：** 无（本机 `~/.small-rust-hermes/bin/markitdown` 曾出现冷启动慢/瞬时失败，属机器环境问题；产品转换路径已有 120s 超时与确定性测试，如需彻底重建 sidecar 可跑 `scripts/setup-markitdown-sidecar.sh`）

---

## 5. 附注

- 本机 DevKit 环境的 `python3`/`apply_patch` 被 SIGKILL（工具不可用），本次改动使用系统 `/usr/bin/python3` 精确替换完成。
- 修复中发现并顺带解决：未提交工作区（2026-08-06 功能批）整体未格式化 + `inbox.rs` 一处 clippy——两条均属既有质量门槛红灯，已一并清零。
