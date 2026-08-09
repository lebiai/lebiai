# 变更记录：CLIPPY-1 — clippy -D warnings 修复 + CI 工作流落地

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-clippy-fix-and-ci` |
| **日期** | 2026-08-03 |
| **状态** | **已验收** |
| **负责人** | Codex（用户委托） |
| **关联** | `docs/records/20260803-pre-dev-review-rules.md`（缺口 CLIPPY-1 + CI） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 开发者 / 协作者（人类 + agent）
- **解决什么痛点：** README 声称 `cargo clippy --workspace --all-targets -- -D warnings`
  可用，实际必红；无 CI 兜底，回归只能靠人肉
- **用完后用户多得到什么：** lint 命令真能全绿；push/PR 有自动 check/clippy/test，
  坏了立刻知道
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（CI 状态一眼可见）
  - [x] 不增加无意义确认或噪音（只在 push/PR 时跑）
  - [x] 高频路径比改前更快或更省心（回归自动拦截）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 任何提交 / PR 前后
- **路径变化：** 改前（本地命令红、无自动检查）→ 改后（本地全绿 + CI 自动兜底）
- **成功标准：** `cargo clippy --workspace --all-targets -- -D warnings` 全绿；
  `.github/workflows/ci.yml` 在 push/PR 上跑 check/clippy/test
- **明确不做什么：** 不加 `cargo fmt --check`（仓库尚未 rustfmt 全清，避免 CI 必红——另记缺口）；
  不改任何业务逻辑

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 静态检查违约层（1 处 clippy lint 违例）+ 自动化缺失层（无 CI）
- **正确的长期默认路径：** 修复违例（`sort_by_key(Reverse)`）使本地命令全绿；CI 与
  P1「lint / test 硬门槛」同一条命令，本地与 CI 行为一致
- **与引擎/各入口边界：** 违例在 `hermes-store`（会话列表排序）；CI 覆盖全部 crates 含
  hermes-gui（Tauri Linux 依赖已装入）
- **安全影响：** 无
- **如何防复发：** CI 在 push/PR 强制 check/clippy/test；P1 质量门槛第 4 项已要求全绿
- **为何这不是补丁：** 修复真实违例 + 建立自动化防线，防复发而非遮错

---

## 1. 方案（Plan）

- **目标：** clippy 全绿 + CI 落地
- **范围：** 做：`crates/hermes-store/src/session.rs` 1 行修复；新增
  `.github/workflows/ci.yml`。**不做：** fmt 检查、构建产物、发布流水线
- **用户路径变化：** 见 0b
- **技术要点：** `unnecessary_sort_by` → `sort_by_key(|b| std::cmp::Reverse(b.0))`；
  CI 用 ubuntu-latest + stable + Swatinem/rust-cache，装 Tauri v2 Linux 依赖
- **风险与回滚：** 低；纯检查层改动；git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** session.rs 排序改 `sort_by_key(Reverse)`；新增 ci.yml
- **关键路径/文件：** `crates/hermes-store/src/session.rs:177`、`.github/workflows/ci.yml`
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | lint 全绿 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | 通过 | |
| 2 | 排序语义不变 | `cargo test -p hermes-store` | 会话列表仍按修改时间倒序 | 通过 | `Reverse` 等价 |
| 3 | CI 文件语法 | 本地校验 YAML | 合法 | 通过 | 实跑待 push 后确认 |

- **自动化：** `cargo check --workspace`、`cargo test --workspace`、clippy
- **手工：** CI 首次实跑需 push 到 GitHub（私有仓库）后确认
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（CI 实跑待 push）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | lint 命令真实可用 + 自动防回归 |
| 开箱即用未破坏 | ✅ | 无运行时影响 |
| 本地优先未破坏 | ✅ | 未触数据/密钥 |
| 测试通过 | ✅ | 全工作区 check/test/clippy 全绿 |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | 修复真实违例 + CI 防线 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ✅ | 1 行等价修复，无残留 |

- **验收人：** Codex（用户委托）
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** CI 实跑需 push 后确认；`cargo fmt --check` 未纳入（仓库未 fmt 全清，
  另行处理）

---

## 5. 附注

- 违例：`crates/hermes-store/src/session.rs:177` clippy `unnecessary_sort_by`
- CI：`.github/workflows/ci.yml`（check → clippy -D warnings → test）
