# 变更记录：反思落盘不得写入进程数据根 —— 测试隔离 + 单次原子追加

| 字段 | 内容 |
|------|------|
| **编号** | `20260913-reflect-write-isolation` |
| **日期** | 2026-09-13 |
| **状态** | **已验收**（2026-09-19 统一签收 · 依据与未验项见 [`20260919-acceptance-sweep`](./20260919-acceptance-sweep.md)）；原状态：工程通过 · 待目视/用户验收 |
| **负责人** | Codex（代行）· 用户验收 |
| **关联** | 用户 9/13 会话排查（数据根污染）；上承 `20260807-codebase-hygiene-30-issues` |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不是因为「CI 上跑得通所以没事」，也不是因为「别的项目测试也这样」。
  拒绝把「测试要写盘」当成必须复用生产数据根的理由。
- **拆出的真：**
  1. `deferred.jsonl` / `reflect-log.jsonl` 的**落盘目标是靠进程全局隐式解析**的
     （`hermes_core::data_path()`）；函数签名里没有目标，调用方无法指定。
  2. `data_root()` 的解析顺序里，**第 3 优先级是「用户自选数据根」指针文件**
     （`paths.rs:100`）。一旦用户迁移过数据位置，该指针就长期生效。
  3. 因此**任何没有设置 `LEBI_DATA_DIR` 的进程（含测试二进制）都会写进用户的真实数据**。
     用户在 CI 上观察不到（runner 没有指针文件），只在真实机器上发生。
  4. 真实伤害已发生：用户数据根 `…/codeINDEx/test` 的 `reflect-log.jsonl` 被写入
     `label="The CI host is named build-prod-01 exclusively"`、`session_id="sess"` 的
     假审计条目；`deferred.jsonl` 96 行中 61 行为测试夹具（`rationale:"test"`）。
  5. 追加语义是错的：`writeln!(f, "{line}")` 对**无缓冲 append File** 会拆成两次
     `write_str`；多线程并发时两次写交错 → `{对象A}{对象B}\n`，读回时整行解析失败并被
     **静默 debug 跳过**（`deferred.rs:58`），数据丢失无人知。
- **如何从真推出：** 真 1 → 落盘目标必须由**调用方显式给出**（与既有正确模式
  `FsMemoryStore::new(dir)` / `FsSkillStore::new(root)` 一致），函数不再自己找全局路径。
  真 3 → 默认值仍由应用侧解析一次 `data_root()`（用户路径不变、零行为回归），
  但测试可以传 tempdir，于是「测试写真实数据」从**可能**变成**不可能**。
  真 5 → 一次 `write_all` 写完「JSON + 换行」，使 append 成为单次系统调用，
  在 `O_APPEND` 下原子。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 桌面 GUI / CLI 用户（本机开发者尤其，因为开发机同时是数据机）。
- **解决什么痛点：** 用户真实数据被测试假数据污染；审计日志不可信；队列静默丢条目。
- **用完后用户多得到什么：** 数据诚实（审计日志里只有真实发生过的事）；跑测试不再有副作用；
  队列不再静默丢数据。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（行为对外零变化）
  - [x] 不增加无意义确认或噪音
  - [x] 主操作一眼能找；空/载/错态完整（本次无用户界面变化）
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在桌面 GUI 里正常使用（批准记忆、看审计）；开发者在同一台机器上跑 `cargo test`。
- **怎么走完：** 用户点「批准」→ 写 `memories/` + `reflect-log.jsonl`；用户忽略候选 →
  进待审队列。全程无新增步骤。
- **看起来怎么样：** **用户可见面零变化**。这是数据诚实修复，不是新功能；不新增控件、不改文案。
- **好走 / 好看：** 不适用（无界面变化）；成功标准是「界面一模一样，但记录是真的」。
- **成功标准：** 跑完 `cargo test` 后，用户数据根的文件大小与内容**逐字节不变**。
- **明确不做什么：**
  - 不动 `~/.lebi-ai` / 指针文件 / 用户数据根位置（那是用户自己的选择）
  - 不改任何用户可见行为、不改 `deferred` 的消费语义
  - 不在本次顺带做「技能目录可见性 / bash 绕过确认」（另立台账）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 存储路径解析（配置层）——隐式全局依赖，而非显式注入。
- **正确的长期默认路径：** 落盘目标由调用方传入；应用侧在**一处**解析 `data_root()` 并向下传递。
  与 `hermes-memory` / `hermes-skills` 的既有注入模式统一。
- **与引擎/各入口边界：** 只动 `hermes-reflect`（引擎能力层），不新增入口、不改能力矩阵。
  `MicroApplyConfig` 已是从 GUI/server/CLI 到引擎的策略传递物，注入点落在它上面，三个入口
  的既有构造方式**不需要改**（默认值 = 进程数据根）。
- **安全影响：** 无放松。数据仍本机明文；不涉及 token / TLS / 0600。
  反而减少「审计日志被伪造内容污染」这一诚实性风险。
- **如何防复发：** 回归测试 `micro_apply_writes_only_to_injected_root`：注入 tempdir 后断言
  **进程数据根的文件长度前后一致**。该断言在修复前必然失败（本次已用探针复现）。
  另加并发测试 `concurrent_saves_stay_one_object_per_line` 钉死单次写。
- **为何这不是补丁：** 补丁会是「在测试里设 `LEBI_DATA_DIR`」——依赖每个测试作者记得设，
  且 CI 观察不到遗漏。本方案改的是**函数签名**：目标变成必给的参数，
  漏设不再是一种可表达的状态。

---

## 1. 方案（Plan）

- **目标：** 反思相关落盘（`deferred.jsonl` / `reflect-log.jsonl`）目标显式化 + 追加原子化。
- **范围：**
  - **做：** `hermes-reflect` 的 `deferred` / `log` 增加按根路径的 API；`MicroApplyConfig`
    携带 `data_root`；`apply_micro_output` 改用注入路径；两处 `writeln!` 改单次 `write_all`；
    现有 `deferred.rs` 手写文件的测试改为走真实 API；补并发测试与隔离回归测试。
  - **不做：** 其它隐式全局写入点（`profile.md`、`palace-index.md`、`memory-stats.jsonl`
    等）本次不扩散；记入遗留项另立台账。不改用户数据根、不改用户可见行为。
- **用户路径变化：** 改前 = 无变化（用户无感）；改后 = 无变化。差异只在**磁盘上不再出现假条目**。
- **技术要点：** `crates/hermes-reflect/src/{jsonl.rs, deferred.rs, log.rs, micro_apply.rs,
  lib.rs}`；三个入口构造处无需改动（默认值保持）。
- **风险与回滚：** 风险=调用方忘传根导致写到别处 → 由 `new()` 默认 `data_root()` 兜底，
  对外行为不变；回滚 = 还原三个文件。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-09-13 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. **新增 `jsonl.rs`（唯一写入原语）**：`append_line(path, value)` 用
     `serde_json::to_string` + `push('\n')` + **一次 `write_all`** 落盘（`O_APPEND` 原子）；
     `read_lines(path, what)` 统一「缺失=空 / 坏行 debug 跳过」。两个文件不再各写一套，
     原子性只可能在一处被改坏。
  2. `deferred.rs`：新增 `path_in(root)` / `save_at` / `load_at` / `clear_at`（目标必给）；
     `save` / `load` / `clear` 退化为「解析进程根一次」的薄包装，签名不变；
     删除 `default_path()`（隐式全局解析入口连根去掉）。
  3. `log.rs`：同构新增 `path_in` / `append_at` / `read_all_at` / `stats_at` /
     `recent_outcomes_at`；`append` / `read_all` / `stats` / `recent_outcomes` 保持原签名并
     转发；删除 `default_log_path()` **及其 `lib.rs` 公开再导出**（无外部使用者）。
  4. `micro_apply.rs`：`MicroApplyConfig` 增 `pub data_root: PathBuf` + `with_data_root()`；
     `new()` 默认 `hermes_core::data_root()`（对外行为零变化）；4 处 `deferred::save` 与
     1 处 `log::append` 改走 `*_at(&config.data_root, …)`。
  5. 测试：`deferred.rs` 原「手写文件绕过 API」测试改为走真实 API；新增
     `deferred` / `log` 各一条 16 线程 × 8 次、`Barrier` 同起点并发追加测试；
     `micro_apply.rs` 五个既有测试改走新增的 `cfg_in(root, …)`（注入 tempdir）；
     两条进程根零写入回归测试。
- **关键路径/文件：** `crates/hermes-reflect/src/jsonl.rs`（新）、`deferred.rs`、`log.rs`、
  `micro_apply.rs`、`lib.rs`
- **偏离方案处：** 有 1 处，且是**收敛**而非放宽：原方案让 `log.rs`「同构新增」自己的一套
  读写，实施时改为抽 `jsonl.rs` 共享原语。理由 = P0 第七条（禁修修补补）与「无冗余」：
  并发正确性只允许有一个实现，否则今天改好 `deferred` 明天仍可能在 `log` 复发。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 跑测试不再改我的真实数据 | 注入 tempdir 跑 `apply_micro_output`（日志/队列各一条） | 注入根有文件、进程根字节数不变 | 通过 | 修复前用探针复现为「会写」 |
| 2 | 队列文件每行都是一个完整条目 | 16 线程 × 8 次、`Barrier` 同起点并发追加 | 128 行、128 条可解析 | 通过 | **旧写法实测失败**：128 只剩 83 条 |
| 3 | 审计日志每行都是一个完整条目 | 同上，`log.rs` | 128 行、128 条可解析 | 通过 | **旧写法实测失败**：128 只剩 99 条 |
| 4 | 既有反思行为不变 | `cargo test --workspace` | 全绿 | 通过 | 414 → **419 passed / 0 failed**（+5 为新回归测试） |
| 5 | 写入目标可控 | `save_at` / `load_at` / `clear_at` 循环（另加 `log` 往返 + 异根不串） | 指定目录内往返成功、它根不受影响 | 通过 | 原「手写文件」测试改走真实 API |
| 6 | 真实机器上跑测试确实不再写我的数据 | **不设** `LEBI_DATA_DIR` 直接 `cargo test --workspace`，比对数据根全量指纹 | 5596 文件逐字节不变 | 通过 | 见 §5（这是本次决定性证据） |

- **自动化：** `cargo test -p hermes-reflect`（带 `LEBI_DATA_DIR` 隔离）、
  `cargo clippy --workspace --all-targets -- -D warnings`（零告警）、
  `cargo fmt --all -- --check`（通过）
- **手工（决定性）：** **不设** `LEBI_DATA_DIR` 跑 `cargo test --workspace` ——
  即当初出事的原始场景 —— 再对用户数据根 5596 个文件做全量 SHA-1 比对：`diff` 无差异。
  修复前同样一跑就会在 `reflect-log.jsonl` 留下 `session_id="sess"` 的假条目。
- **测试结论：** [x] 全部通过

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 数据诚实 / 无副作用 / 不丢条目 |
| 开箱即用未破坏 | ☑ | 无新增依赖；用户路径零变化 |
| 本地优先未破坏 | ☑ | 仍本机明文，无出站 |
| 测试通过 | ☑ | fmt 通过 / clippy 零告警 / 419 passed；含「不设隔离」真机一跑（见 §3 #6） |
| 记录完整 | ☑ | 本文件 + 索引 + 遗留项 |
| 产品+架构两视角齐全 | ☑ | 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 目标进签名，漏设不可表达 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理 | ☑ | 写入原语收敛为 `jsonl.rs` 一份；`default_path` / `default_log_path` 及其公开再导出已删；原「手写文件绕过 API」的测试写法已删 |
| 操作与视觉：走查完成，好看、好走 | ☑ | 无用户可见面变化（故不涉及） |
| 第一性原理：三步写全 | ☑ | 0-fp |

- **验收人：** 用户
- **验收日期：** 待定
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  1. `REFLECT-GLOBAL-WRITERS`：其余隐式全局写入点（`profile.md` / `palace-index.md` /
     `hermes-reflect/src/inbox.rs:69`、`ledger.rs:71`、`hermes-memory/src/stats.rs`、
     `hermes-skills/src/stats.rs`）同类风险未收。本次不设隔离跑全量测试**未观察到它们被写**，
     故仍列为遗留而不是已修。
  2. `REFLECT-GLOBAL-READERS`：读侧仍解析进程根（`prompt.rs:88` 的 `recent_outcomes(10)`），
     即 prompt 内容仍受本机历史影响。只读、不写坏数据，故不在本次范围。
  3. `SKILL-BASH-BYPASS`：技能目录对文件类工具不可见（`read` 被 workspace 锁死）→
    模型绕道 `bash + python3` 直接改写 `SKILL.md`，**绕过 `skill_create` 的用户确认**。
    属安全语义变更，另立台账。

---

## 5. 附注

- 污染证据（用户数据根 `…/codeINDEx/test`）：
  - `reflect-log.jsonl` 中 `at=2026-09-13T11:43:14Z / 11:43:40Z`、`session_id="sess"`、
    `label="The CI host is named build-prod-01 exclusively"` = 当日两次 `cargo test --workspace`。
  - `deferred.jsonl` 96 行 / 61 行测试夹具 / 35 行粘连。
- 已完成的用户数据清理：`reflect-log.jsonl` 8901B → 1557B（删 48 行假条目，留 6 行真实）；
  `deferred.jsonl` 13776B → 0B（61 行全为夹具）；备份 `*.bak-20260913`。
- 复现实验（未触碰真实数据）：`LEBI_DATA_DIR=/tmp/… cargo test -p hermes-reflect micro_apply`
  → 立即在 /tmp 生成 `deferred.jsonl` + `reflect-log.jsonl`；连跑 6 次，2 次出现粘连行。
- 修复后决定性证据（2026-09-13，本机）：
  - `cargo test --workspace` **不设 `LEBI_DATA_DIR`** → 419 passed / 0 failed。
  - 前后对 `/Users/aodun/Documents/codeINDEx/test` 全树 5596 个文件做 SHA-1 清单 `diff`
    → **完全一致**（无新增、无修改）。即：真实机器上跑测试已不再产生任何副作用。
  - 回归测试的「有牙齿」验证：把 `jsonl.rs` 的写入临时改回 `writeln!`，两条并发测试
    立即失败（128 → 83 / 99 条），确认不是「写了个永不失败的测试」。
