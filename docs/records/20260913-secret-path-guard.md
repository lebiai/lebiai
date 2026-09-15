# 变更记录：产品密钥一律不进工具 —— 一份清单，五处共用

| 字段 | 内容 |
|------|------|
| **编号** | `20260913-secret-path-guard` |
| **日期** | 2026-09-13 |
| **状态** | 工程通过 · 待目视/用户验收 |
| **负责人** | Codex（代行）· 用户验收 |
| **关联** | 用户 9/13 会话两条报错排查（`read` 越界 + bash 密钥闸）；上承 `20260913-skill-index-freshness` |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不是「再加几个子串匹配」，不是「用户把数据根挪到 `~/Documents` 是用户自己的事」，
  也不是「bash 已经有字符串闸了就等于守住了」。拒绝把「秘密保护」当成某个工具自己的事。
- **拆出的真：**
  1. `read`（以及 `grep`）**没有任何秘密清单**：`read.rs:43` 只调 `safety::resolve()`；
     `read` / `write` / `edit` 三处都没有秘密判定（全仓 grep 为证）。唯一有清单的是 `open`，
     而那份清单（`is_secret_open_path`）里**没有 `config.toml`**。
  2. `resolve()` 有一条**无条件的导出目录豁免**（`safety.rs:14-21`）：只要路径是绝对路径、
     落在 `Desktop/Documents/Downloads` 下就放行。而本机数据根是
     `/Users/aodun/Documents/codeINDEx/test` —— **正在 `~/Documents` 里**。
     于是 `read /Users/aodun/Documents/codeINDEx/test/config.toml` 会被放行，
     **静默返回 API Key**。这不是推演：今天密钥就是这样进到会话记录里的。
  3. 同一条豁免还制造了形状不一致：**同一个技能文件，相对路径被拒、绝对路径放行**
     （9/13 会话 22 行与 19 行）。
  4. 同一件事（什么是秘密）在四处各写一遍，且互不相同：
     seatbelt 用 `hermes_core::data_root()` 派生（**对**）· bash 字符串闸写死
     `.lebi-ai`/`.lebi-law`（只护旧路径）· `open` 一份半个清单 · `read` 没有。
  5. 数据根会迁移（本机就从 `~/.lebi-ai` 迁到 `…/codeINDEx/test`），
     所以「只认当前根」和「只认旧根」**两个方向都会漏**：前者漏旧密钥，后者漏新密钥。
  6. 判定顺序也是错的：`resolve_for_open` 先查「文件是否存在」再查秘密 ——
     不存在的路径与符号链接指向的路径都能绕过。
- **如何从真推出：** 真 1+4 → 秘密判定**只能有一处**，且必须被所有入口调用；
  以 seatbelt 那份（已由 `data_root()` 派生）为基准反向统一。真 5 → 根集合 =
  **当前数据根 + 已知旧根**，两个方向都不漏。真 6 → 判定必须**提前到存在性/白名单之前**，
  并且在拿到规范化路径后再判一次（挡住符号链接）。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 每一个用户。本机密钥是这台机器上最该守住的资产。
- **解决什么痛点：** 模型能把 API Key 读进对话（**已发生一次**）；而用户以为有闸，
  实际三条读写路径里只有一条有闸。
- **用完后用户多得到什么：** 密钥与渠道凭证一律不进对话；越权尝试得到一句能照做的话。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（交付物读写零变化）
  - [x] 不增加无意义确认或噪音
  - [x] 空/载/错态完整（无 UI 变化）
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户问「帮我看看配置 / 为什么报错」，模型顺手动到 `config.toml`；
  或模型为定位问题去 `cat` 数据根下的凭证。
- **怎么走完：** 用户要改密钥 → 设置页（本来就在那里）。模型被拒后应转述
  「这项要在设置里改」，而不是绕道。
- **看起来怎么样：** **用户可见面零变化**；只有越权时多一句点名文件的拒绝话术。
- **空 / 载 / 错态：** 读写自己的交付物（workspace / 桌面 / 文档）**不受影响**；
  碰凭证 → 一句可执行的话（去设置），不是沉默。
- **成功标准：**
  1. 任何工具都读不到 / 写不了已知数据根下的 `config.toml` / `wechat.toml` /
     `feishu.toml` / `telegram.toml` / `mcp.json` / `server.token`；
  2. 正常文件读写零回归。
- **明确的取舍（必须写清）：** 模型因此**看不到配置内容**是**故意的** ——
  用户要看就自己打开或去设置页。这是安全优先的取舍，不是能力缺失。
- **明确不做什么：**
  - 不动导出目录豁免本身（桌面/文档交付仍允许）
  - 不动技能目录可达性（`SKILL-BASH-BYPASS` 另立台账）
  - 不做数据加密、不改 GUI/CLI 协议
  - 不改 bash「整条命令拒绝」的语义（理由见 §1）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 权限判定层 —— 同一判定在五处各写一遍，且**判定发生在白名单之后**。
- **正确的长期默认路径：** `hermes-tools/src/safety.rs` 内**一份清单 + 一份根集合 +
  一个判定**（`is_secret_path`），被 `read` / `grep`(经 `resolve`) / `open` / `write` /
  `edit` / bash 预筛 / seatbelt 七处共用；判定**提前到存在性与白名单之前**，
  并在规范化之后**再判一次**（挡符号链接）。
- **与引擎/各入口边界：** 只动 `hermes-tools`（引擎能力层）。不改入口、不改能力矩阵、
  不改 `companion` 协议。
- **安全影响：** 全面收紧。旧数据根（本机 `~/.lebi-ai` 的旧密钥）也一并收口 ——
  现在只有 bash 字符串闸护着它，而那是可绕过的。
- **误伤风险与反证：** 会不会把用户**真实交付物**当成密钥？不会 ——
  判定要求「父目录 = 某个已知数据根」，所以 `workspace/config.toml`、
  `~/Desktop/config.toml` 都不受影响。这条要进测试。
- **回滚：** 还原 `safety.rs` + `bash_sandbox.rs` 两个文件。

---

## 1. 方案（Plan）

- **目标：** 已知数据根下的凭证文件，**任何工具都读不到、也写不了**；判定只有一份。
- **范围：**
  - **做：** `PRODUCT_SECRET_FILES` 清单（6 个）+ 根集合（当前根 + 4 个已知旧根）+
    `is_secret_path()`；接入 `resolve`（`read`/`grep`）、`resolve_for_open`（`open`）、
    `resolve_for_write`（`write`/`edit`）、`bash_secret_read_blocked`（预筛）、
    `seatbelt_deny_secrets()`（内核层）；判定顺序提前 + 规范化后复判。
  - **不做：** 导出目录豁免本身、技能目录可达性、数据加密、bash 分段过滤（见下）。
- **诚实修正（上一轮对话里的承诺）：** 我说过「改成按解析后的真实路径判定，就能只拒那个
  `cat`，`ls` 照跑」。**做不到**，现在更正：bash 拿到的是一个**字符串**，要可靠地判断
  「哪一段读了哪个文件」等于现场写一个 shell 解析器；写不对就是安全洞。
  所以 bash 仍然是**整条命令拒绝（fail-closed）**。这次改的是两件实事：
  ①覆盖**当前**数据根（原来只认 `~/.lebi-ai`，护错了目录）；
  ②拒绝话术**点名是哪个文件**。你那条命令里确实有 `cat config.toml`，
  整条被拒是**正确**行为，问题在话术没告诉你是哪句。
- **用户路径变化：** 改前 = 密钥可被工具静默读走；改后 = 读不到，且越权有一句能照做的话。
- **技术要点：** `crates/hermes-tools/src/safety.rs`、`crates/hermes-tools/src/bash_sandbox.rs`。
- **风险与回滚：** 风险 = 误伤同名文件（已用「父目录必须是数据根」排除，并有测试）；
  回滚 = 还原上述两个文件。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-09-13 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要（2 个文件）：**
  1. `crates/hermes-tools/src/safety.rs`
     - 新增 `PRODUCT_SECRET_FILES`（6 个名字）、`LEGACY_DATA_DIRS`（4 个旧根）、
       `product_data_roots()`、`product_secret_paths()`、`secret_paths()`、
       `is_product_secret_path()` / `is_product_secret_path_in()`、`is_secret_path()`、
       `deny_secret()`；原 `is_secret_open_path` 改名 `is_os_secret_path` 并并入
       `is_secret_path`（不再有第二份清单）。
     - `resolve()`（`read` / `grep`）：**先按名字判秘密**（绝对路径在任何白名单之前判，
       相对路径在规范化之后再判一次）——相对/绝对/`~/`/符号链接四种写法都收敛到同一句拒绝。
     - `resolve_for_write()` / `resolve_for_open()`：同样前置判秘密；`open` 的判定
       从「存在性之后」提到「之前」，不再泄露文件是否存在。
     - `bash_secret_read_blocked()`：返回类型 `Option<&'static str>` → `Option<String>`；
       产品分支改为「命令里出现某个**已知数据根**的字符串 **且** 出现某个秘密文件名」，
       根字符串同时含绝对形式与 `~/` 形式（shell 拿到的是模型写的字面量）；
       拒绝话术**点名文件**（`refusing to read a product secret file: <root>/<name>`）。
     - `same_file_path()`：沿用 `strip_macos_private`，让 `/var` 与 `/private/var`
       两种写法等价（否则 macOS 上判定会随机失效——实现时踩到过，见 §5）。
  2. `crates/hermes-tools/src/bash_sandbox.rs`：`seatbelt_deny_secrets()` 改为遍历
     `safety::secret_paths()`——内核层与工具层从此**同一份清单**，旧数据根也一并 deny。
- **偏离方案处：** 无。（口头承诺的修正见 §1「诚实修正」。）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | `read` 读不走我的密钥 | `resolve(ws, "<当前数据根>/config.toml")` | 拒绝，理由点名文件 | 通过 | 修复前放行（数据根在 `~/Documents` 下） |
| 2 | 旧数据根的密钥同样读不到 | `<home>/.lebi-ai/config.toml` | 拒绝 | 通过 | 修复前 `read` 也放行 |
| 3 | 写也不能写 | `resolve_for_write(ws, …/config.toml)` | 拒绝 | 通过 | 修复前仅弹确认 |
| 4 | 不存在的路径也拒绝 | `resolve_for_open` 指向不存在的 `config.toml` | 拒绝（不是「文件不存在」） | 通过 | 修复前先报「不存在」 |
| 5 | 六个文件全都被认出来 | 6 个名字 × 当前根 / 旧根 | 全部 `true` | 通过 | `config.toml` `wechat.toml` `feishu.toml` `telegram.toml` `mcp.json` `server.token` |
| 6 | 同名但不同目录不误伤 | `ws/config.toml`、`~/Desktop/config.toml` | `false`，正常读写 | 通过 | 防误伤反证；`resolve(ws,"config.toml")` 仍成功 |
| 7 | bash 预筛覆盖当前根并点名文件 | `cat <当前根>/config.toml` | 拒绝且话术含路径 | 通过 | 修复前完全放行 |
| 8 | bash 既有行为不变 | `cat ~/.lebi-ai/config.toml` 拒绝 / `ls outputs` 与 `cat workspace/config.toml` 放行 | 同左 | 通过 | 原测试保留未改 |
| 9 | 内核层也挡旧数据根 | `seatbelt_deny_secrets()` 的 deny 行数 | = `secret_paths().len()`，含 `.lebi-ai` | 通过 | 修复前只含当前根 |
| 10 | 既有行为不变 | `cargo test --workspace` | 全绿 | 通过 | 421 → **430 passed / 0 failed**（+9） |
| 11 | 这些测试不是摆设 | 把 `is_product_secret_path_in` 临时改成恒 `false` | 相关用例立即失败 | 通过 | 5 条立刻 FAILED（含 #1 #2 #3 #4 与符号链接那条） |
| 12 | 跑测试仍然不动我的数据 | 无隔离 `cargo test --workspace`，前后全量 SHA-1 | 零变化 | 通过 | 430 个测试，21:37:27 → 21:37:35，逐字节一致 |

- **自动化：** `cargo fmt --all -- --check` 通过 · `cargo clippy --workspace --all-targets
  -- -D warnings` 零告警 · `cargo test --workspace` 430 passed / 0 failed ·
  `hermes-tools` 内 `98 passed`。
- **测试结论：** [x] 全部通过（真机「模型确实读不到」需用户重启后体感确认）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 密钥不进对话；越权有话说 |
| 开箱即用未破坏 | ☑ | 无新增依赖 |
| 本地优先未破坏 | ☑ | 仍本机明文；不新增出站 |
| 测试通过 | ☑ | fmt 通过 / clippy 零告警 / 430 passed（含 #11 有牙齿验证） |
| 记录完整 | ☑ | 本文件 + 索引 + 遗留项 |
| 产品+架构两视角齐全 | ☑ | 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 五处清单收敛为一处；判定顺序修正；内核层与工具层同源 |
| 代码卫生：旧代码/注释/入口已清理 | ☑ | `is_secret_open_path` 已并入 `is_secret_path`（无第二份清单）；bash 闸话术改为点名文件 |
| 操作与视觉 | ☑ | 无用户可见面变化 |
| 第一性原理：三步写全 | ☑ | 0-fp |

- **验收人：** 用户
- **验收日期：** 待定
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  1. `SKILL-DIR-REACH`：技能目录在数据根下、由文件工具可达（本机数据根落在 `~/Documents`
     走的是导出目录豁免）。属 `SKILL-BASH-BYPASS` 那条设计决策，本次不碰。
  2. `LICENSE-FILE-TAMPER`：`license.json` / `license.dev-backup.json` 不在本次清单内
     （它们不是凭证）。但授权是产品事实，模型若改写它会动摇验签语义 —— 另立台账评估。
  3. `BASH-SEGMENT-FILTER`：bash 仍整条拒绝（见 §1 诚实修正）。
  4. `CHANNEL-ALLOWLIST`：`channel-allowlist.toml` 不是凭证，未纳入；但它决定谁能跟搭子说话，
     写入等于改权限，值得单独评估。

---

## 5. 附注

- 证据（本机，2026-09-13）：
  - 数据根 `/Users/aodun/Documents/codeINDEx/test`（指针文件），落在 `~/Documents` 下。
  - 该目录下确有 `config.toml`（0600，含 provider key）、`license.json`、`server.token` 之类。
  - 会话 `sessions/2026-09-13T12-49-36-5f1eb572.jsonl:22` 相对路径被拒；
    `:60` 那条 bash 命令为 `ls -la ~/.lebi-ai/skills/; …; cat ~/.lebi-ai/config.toml`。
- 判定顺序的实证：`resolve` 在**规范化之前**不查秘密；`resolve_for_open` 在
  **存在性之后**才查 —— 两处都给了绕过空间。
- 实现时踩到的坑（已修，值得记）：`product_data_roots()` 用字面路径，而 macOS 上
  `dunce_canonicalize` 会把 `/var/...` 变成 `/private/var/...`，两侧不一致会让判定
  在 `TMPDIR` 类路径上**随机失效**。已用仓库既有的 `strip_macos_private` 统一
  （`same_file_path`），并保留 `/var` 与 `/private/var` 两种写法的等价性。
- 归因实证（避免把 App 的行为算到测试头上）：21:28–21:29 数据根出现过三处变化
  （`license.json` / `pending-leave.json` / `sessions/2026-09-13T12-57-55-…jsonl`）。
  查证为**运行中的 GUI** 所写（`pending-leave.json` 只有 GUI 命令会写；
  `license.rs` 的测试全部用 tempdir），且与用户当时在 App 里重试「小王干活」的
  会话记录同刻。随后 21:37:27→21:37:35 无隔离跑 430 个测试，数据根**零变化**，
  测试污染已确认与此无关。
- 顺带观察（未处理，非本次范围）：该会话文件里出现**第二个 `meta` 行**——
  `hermes-store` 会逐行应用 meta 事件，属合法形态，不是数据损坏。
