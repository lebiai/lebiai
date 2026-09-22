# 变更记录：第二批 · 看得见的地方 + 两条 P0（试用不能本地重置 · P0 收编团队）

| 字段 | 内容 |
|------|------|
| **编号** | `20260919-batch2-visible-fixes` |
| **日期** | 2026-09-19 |
| **状态** | **已验收**（2026-09-19 统一签收 · 依据与未验项见 [`20260919-acceptance-sweep`](./20260919-acceptance-sweep.md)）；原状态：待验收（门禁全绿；等用户目视） |
| **负责人** | Agent（主线）· 用户裁决批次 |
| **关联** | [`20260918-reaudit`](./20260918-reaudit.md) §七「第二批」；P0-3 / P0-5（第一批结转） |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：**
  1. 不是「审计报告列了一条，就照着那一行去改那一行」——症状与根因不在同一层。
  2. 不是「红叉换个颜色 / 折叠条数字调一调 / 给产出列表多塞一个目录」这种按位置打补丁。
  3. 不是「试用期是『用户说没试过就没试过』」——把本机信任当默认。
- **拆出的真：**
  1. **红叉与数字都是「系统在喊疼」**：用户看见的每一次刺眼/重复/缺项，背后是「同一件事有两套判据」或「写死了一个值」。改判据，不改像素。
  2. **试用起点是一个事实，不是一条记录**：事实住在机器上（锚），记录住在数据根（`license.json`）。记录可以被删、被改、被换目录；事实不能。所以事实必须放在数据根**之外**，且只许往前钉。
  3. **团队的实现早已存在**（工位 / 项目组 / 授权码点名），但 P0 一个字没写、还反而把「分工干活」列进「明确不是」。文档与实现相反时，**两个都要改，以产品目标为准**——所以升 P0，不改实现。
- **如何从真推出：**
  - 折叠区取数改从「折叠前缀分组」里找那一天 ⇒ 返回区间必然 ⊂ 折叠区，重复从判据层消失（不是截断数字）。
  - 试用判定改读**数据根外的锚文件**，且「现在」取墙上时间与上次见到的较晚者 ⇒ 删文件、换 `LEBI_DATA_DIR`、回拨时钟三条路一起堵；同时 release 版不再接受任何环境变量开关开发者重置。
  - P0 新增第十二条「一支 AI 团队」，把工位（身份·职责边界·归属）、项目组（一会话·一记忆·一文件·接口人）、授权点名、过期只锁能力写成现行规则，并同步 AGENTS / DEVELOPMENT_RULES / docs 版本号。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 桌面 GUI 用户（默认路径）；P0-5 面向「试用期到期仍可重置」的商业事实。
- **解决什么痛点：**
  - 翻旧账时同一个 80 条被数两遍，用户以为会话坏了（实测 275 条会话点开返回 275 条、其中 80 条重复）。
  - 对话里一有小工具失败就满屏红叉红底，用户觉得「怎么这么多错误」。
  - 「我的材料 → 我产出的」里混进上一代律师版的 `output/` 产物（串味），而真正交付在**工作区根**的文件反而看不见；切回该 tab 也不刷新。
  - 仓库里躺着 600+ 行前端/后端死代码（SkillPanel + skills 命令 + i18n 整段 + MotionCard），改代码的人会被带偏。
  - 试用期能被删文件/改钟/换数据根重开 —— 商业门禁形同虚设。
  - P0 没写工位与项目组，任何人（含 AI）照 P0 干活都会把已有能力当违规。
- **用完后用户多得到什么：** 翻旧账不再重复计数；失败是文字不是红叉；产出列表只显示「真的产出的」且切 tab 就刷新；代码面少 600+ 行噪音；试用期是真的 3 天；文档与实现同向。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期
  - [x] 不增加无意义确认或噪音（反而减少噪音）
  - [x] 主操作一眼能找；空/载/错态完整
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在日常对话/翻旧账/看产出/粘贴授权码时。
- **怎么走完：**
  1. **翻旧账**：长会话 → 点「更早的日子」→ 选某一天 → 只出现那一天的条目，条数与折叠条上的数字对得上；不在折叠区的那天给一句「这一天不在折叠区里」。
  2. **失败**：任何工具/请求失败 → 顶部一条深色提示，左边一道玫红细边 + **文字**（「没成 / Didn't work」），卡片内容说清哪一步为什么；反思审阅那四个动作按钮全部带字（接受/拒绝），拒绝不再用红色。
  3. **我产出的**：切到 `我产出的` tab → 列表按天倒序，含 `outputs/<日期>/…` **和**工作区根的直接交付文件；代码文件与目录不出现在这里；每次切回该 tab 重新取一次。
  4. **试用**：装完首次可用起算 3 天；删 `license.json`、换 `LEBI_DATA_DIR`、把系统钟往回拨，都不能重新开始试用。
- **看起来怎么样：** 失败提示深底 + 玫红细边 + 文字标记，不再是红底大叉；反思审阅按钮一行字，不再是一个光秃秃的符号；产出列表把「我带来的 / 我产出的」分两 tab，各自倒序分页。
- **好走 / 好看：** 失败提示不再抢视觉；产出列表不再需要用户自己猜哪些不是交付物。
- **成功标准：** 用户不再觉得「系统报了一堆错」；产出列表里找不到代码文件，且根目录的交付物看得见；试用重置三条路都堵上。
- **明确不做什么：**
  - 不动 P1-4（查重覆盖所有落盘路径）· P1-5（双队列合一）· P1-7（发布链）· P1-8（server TLS）—— 属第三批。
  - 不删 `embed` feature 与 `pub fn` 级死代码 —— 属第三批候选（本轮定为遗留项，见 §4）。
  - 不改「我产出的」里旧律师产物的**处理方式**（是否清理由用户定，见 §5）。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  - 折叠重复 = **分组判据不一致**（一处只对前缀分组、一处对全量分组后切片），不是渲染问题。
  - 红叉 = **前端表现层**统一走一个 toast 组件 + 两处图标按钮无文字。
  - 产出 = **路径来源写死**（`OUTPUT_ROOTS` 含上一代 `output/`）+ **前端只在挂载取一次**。
  - 试用 = **状态存储单点**（全部信任数据根里的 `license.json`）+ **release 版仍受环境变量控制**。
  - P0-3 = **权威文档与实现不同向**（方向变更未先升 P0，违反 AGENTS 第 106 行）。
- **正确的长期默认路径：**
  - 折叠区取数只从折叠前缀分组里找 ⇒ 结构上不可能越界。
  - 试用起点 = 数据根外的锚 + 单调不减的「现在」；debug-only 的开发者工具不再受环境变量影响。
  - 产出 = 白名单根（`outputs`）+ 工作区根直接文件（跳隐藏、跳代码文件）；tab 激活即重取。
- **与引擎/各入口边界：** `window_day` 落在 `hermes-store`（GUI 与 server 共用）；锚落在 `hermes-core::paths`（各入口同一份）；`license_file_path` 恢复 `pub` 供 GUI 装配层使用，但真正的路径注入仍走 `*_at` 变体。无入口独占逻辑。
- **安全影响：** 锚文件 `0600`；`license_file_path` 只读导出，不新增写入面；release 版路径不再接受 `LEBI_DEV_TOOLS`。
- **如何防复发：** 见 §3 自动化用例（含「折叠组永不与最近窗口重叠」「回拨时钟不买时间」「删文件不生成新试用」「只读检查不建任何东西」）。
- **为何这不是补丁：** 每条都是把「同一件事的两套判据」收敛成一套（折叠的前缀分组、试用的锚 + 单调现在、产出的白名单根），不是给症状加特判。

---

## 1. 方案（Plan）

- **目标：** 收掉第二批（④ 红叉改文字 · ⑤ 折叠旧账去重复 · ⑥「我产出的」修正 · ⑦ 死代码清理）+ 结转的 P0-3、P0-5。
- **范围：**
  - 做：折叠取数收敛 · toast/反思按钮去红叉 · 产出根白名单 + 工作区根交付物 + tab 重取 · 删 SkillPanel/skills 命令/i18n `skills.*`/MotionCard/过期注释 · 试用锚 + 时钟单调 + release 无开发者后门 · P0 升 v0.12 并同步下游。
  - **不做：** 第三条（P1-4/5/7/8 等）、`embed` feature、pub fn 级死代码（遗留，见 §4）。
- **用户路径变化：**
  - 翻旧账：改前点开某天会多出 80 条重复 → 改后只出现那一天。
  - 失败：改前红底 + 红叉 → 改后深底 + 玫红细边 + 文字。
  - 产出：改前含旧 `output/`、不含工作区根交付物、切 tab 不刷新 → 改后只含 `outputs/` + 工作区根直接交付、切 tab 重取。
  - 试用：改前可重置 → 改后不可（删文件 / 换目录 / 回拨钟）。
- **技术要点：**
  - `crates/hermes-store/src/session_days.rs`（新增 `window_day` + 2 测试）、`crates/hermes-store/src/lib.rs`（导出）
  - `crates/hermes-gui/src/commands/session.rs`（`load_session_day` 改用它）
  - `crates/hermes-gui/ui/src/components/common/ToastHost.tsx`、`reflect/ReflectionReview.tsx`、`i18n.ts`（`toast.failed`）
  - `crates/hermes-gui/src/commands/outputs.rs`、`ui/src/components/materials/MaterialsPanel.tsx`
  - 删除：`ui/src/components/skills/SkillPanel.tsx`、`ui/src/components/motion/MotionCard.tsx`、`crates/hermes-gui/src/commands/skills.rs`（+ `commands/mod.rs`、`main.rs` 注册）、i18n `skills.*`
  - `crates/hermes-core/src/paths.rs`（`system_config_dir` / `trial_anchor_path`）、`crates/hermes-core/src/license.rs`（锚 + `effective_now` + debug-only `dev_tools_gate`）、`crates/hermes-core/src/lib.rs`（导出）
  - `PRODUCT_PRINCIPLES.md` v0.12（第十二条）· `AGENTS.md` · `DEVELOPMENT_RULES.md` · `docs/README.md` · `docs/records/README.md`
- **风险与回滚：** 条款均可在单个 crate 内回退；i18n 重建须 build 通过（见 §3）。
- **方案确认：** [x] 已对照 P0/P1 · 日期/人：2026-09-19 · Agent

---

## 2. 实施（Implement）

- **实际改动摘要：**
  - **P1-3 折叠：** `session_days::window_day(messages, window, day)` 只从 `window_split` 的**前缀分组**里取那一天；GUI `load_session_day` 改调它；错误文案改「这一天不在折叠区里」。
  - **P1-10 红叉：** 错误 toast 去红底红叉 → 深底 + `border-l-4 border-l-rose-400` + 文字 `t("toast.failed")`（en `Didn't work` / zh `没成`）；`ReflectionReview` 四个纯图标按钮补文字标签，拒绝改中性色。
  - **P1-11 产出：** `OUTPUT_ROOTS` 收敛为 `["outputs"]`；新增 `collect_root_deliverables()` 只扫工作区根直接文件（不下钻、跳隐藏、跳 `looks_like_code_file`）；`MaterialsPanel` 加 `active`/`tab` 依赖，切 tab 重取。
  - **P1-12 死代码：** 删 `SkillPanel.tsx`（541 行）、`MotionCard.tsx`（31 行）、`commands/skills.rs` 与注册、i18n `skills.*`（88 行）；`personas.rs` 注释「四个自带」改「三个自带（大导演 / 工具人李现在 / 资料员小文）」；`index.css` 去掉会污染 grep 的规格自查命令。
  - **P0-5 试用：** `paths::trial_anchor_path()`（系统配置目录，数据根之外）；`license.rs` 加 `effective_now` / 锚读写 / `anchor_trial_start`（只往前钉）；`*_full` 变体带锚；`dev_tools_gate(cfg!(debug_assertions))` —— 不再看 `LEBI_DEV_TOOLS`。
  - **P0-3 权威：** P0 升 v0.12（定义卡加「团队」行；新增第十二条），同步 `AGENTS.md`（文首/自检 18·新增 19/架构速记加 team 行/对齐行日期）、`DEVELOPMENT_RULES.md`、`docs/README.md`、`docs/records/README.md`。
- **关键路径/文件：** 见 §1 技术要点。
- **偏离方案处：**
  1. `license_file_path` 一度被改私有 → clippy 编译失败（GUI 装配层要用）→ 恢复 `pub` 并补回文档注释。
  2. **事故：** 清 i18n `skills.*` 时用「按行过滤」，误删 42 行合法续行（多行字符串值 + 文件尾部结构），一度改坏 `i18n.ts`。已用 `ui/dist/assets/index-BLuxmQJ4.js`（改前的构建产物）反解两份完整字典，以 `git show HEAD` 为骨架重建，`npx tsc --noEmit` + `npm run build` + 键位比对（en/zh 各 705 键、代码用到的 542 键零缺失）三重验证。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 翻旧账不重复 | 275 条会话，逐组断言折叠区取回 | 取回区间 ⊂ 折叠区、条数与声称一致 | 通过 | `a_folded_day_never_overlaps_the_recent_window` |
| 2 | 短会话没有旧账可翻 | 短会话请求折叠 | 什么都不返回 | 通过 | `a_short_session_has_nothing_to_fold` |
| 3 | 产出不含上一代律师产物 | 构造 `output/` + `outputs/` | 只收 `outputs/` | 通过 | 用例改名收口 |
| 4 | 工作区根脚本/目录不进产出 | 根放 `.py`/目录 | 不出现 | 通过 | `root_level_scripts_and_dirs_stay_out_of_the_deliverable_list` |
| 5 | release 版没有开发者重置 | 直接问门禁函数 | false | 通过 | `release_builds_have_no_dev_tools` |
| 6 | 回拨时钟不买时间 | 把「现在」提前拨回 | 判定不变 | 通过 | `a_rewound_clock_does_not_buy_trial_time` |
| 7 | 删授权文件不生成新试用 | 删 `license.json` 再读 | 不重开试用 | 通过 | `deleting_the_license_file_does_not_mint_a_new_trial` |
| 8 | 锚只往前钉 | 数据根提出更晚起点 | 被夹回锚 | 通过 | `the_anchor_clamps_a_later_trial_start` |
| 9 | 只读检查不建任何东西 | 冷启动只读判定 | 不创建锚/文件 | 通过 | `the_readonly_check_never_creates_anything` |
| 10 | 前端构建无损 | `npm run build` | 成功产 dist | 通过 | 671.81 kB |
| 11 | i18n 键位完整 | 解析 `i18n.ts` 比 en/zh | 各 705 键、差异 0 | 通过 | 自查脚本 |

- **自动化：**
  - `cargo fmt --all -- --check` → exit 0
  - `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
  - `cargo test --workspace` → **exit 0**：39 个测试目标全 ok、**678 条用例通过、0 failed**（`LEBI_DATA_DIR` 指向临时目录，未碰真实数据根）
  - `npx tsc --noEmit` → exit 0 · `npm run build` → exit 0
- **手工：** GUI 重启后目视（翻旧账 / 失败提示 / 产出 tab / 授权码粘贴）。
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（列出）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☐ | |
| 开箱即用未破坏 | ☐ | |
| 本地优先未破坏 | ☐ | |
| 测试通过 | ☑ | fmt / clippy exit 0；`cargo test --workspace` 39 目标 · 678 用例 · 0 failed；前端 tsc + build 通过 |
| 记录完整 | ☐ | |
| 产品+架构两视角齐全 | ☐ | |
| 非修修补补（默认路径正确） | ☐ | |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☐ | |
| 操作与视觉：走查完成，好看、好走（P0 第十一条） | ☐ | |
| 第一性原理：三步写全，不是靠类比开工（P0 第零条） | ☐ | |

- **验收人：**
- **验收日期：**
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  - **P1-12 余项：** `embed` feature（`fastembed`）无人启用 + pub fn 级无引用符号（审计点名为 7 个）—— 待第三批逐条核（删 or 启用）。
  - **第三批其余：** P1-4 查重收进 `FsMemoryStore::put` · P1-5 双队列合一 · P1-7 发布链（upsert / tag 校验 / 架构覆盖 / 签名公证）· P1-8 server TLS + legacy `?token=` + CORS · P1-1/P1-14 引擎侧输入折叠与 `web_fetch` 截断 · P1-13 项目组归档区 · P1-2 桌面端不注入 `profile.md`。

---

## 5. 附注

- **用户可见变化（需知会）：** 「我产出的」现在会显示**工作区根**的直接交付文件。测试数据根 `test/workspace/` 根上存有几个上一代律师版产物（如 `会议纪要_XX律师事务所团队周例会.docx`、`analysis_report.md`），它们现在会出现在产出列表里。是否清理请用户裁决。
- **i18n 事故与修复：** 见 §2 偏离方案处第 2 条。修复后键位比对：en-US 705 / zh-CN 705 / onlyEn 0 / onlyZh 0 / 代码用到 542 键零缺失；`tsc` 与 `build` 均通过。
- 相关：`docs/records/20260918-license-hardening.md`（第一批）、`docs/spec/personas.md`、`docs/spec/projects.md`。
