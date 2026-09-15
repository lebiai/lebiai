# 变更记录：失败不喊、产出按天归位、产出进「我的材料」

| 字段 | 内容 |
|------|------|
| **编号** | `20260913-quiet-failures-and-visible-outputs` |
| **日期** | 2026-09-13 |
| **状态** | 工程通过 · 待目视/用户验收 |
| **负责人** | Codex（代行）· 用户验收 |
| **关联** | 用户 9/13 三条反馈；上承 `20260913-approval-lighter-and-quieter-process` |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不是「把错误藏起来让界面好看」，不是「再给产出加一层目录就算完」，
  也不是「把 workspace 整个塞进材料列表」。拒绝把「少一点噪音」做成「少一点真相」。
- **拆出的真：**
  1. **红色叉不是一个计量单位，却长得像判决。** `MessageBubble.tsx` 的过程折叠里，
     每一行失败工具都挂一个 `XCircle` + 红字。一次长任务里失败两三次本来就正常
     （换个信源重试、路径先试错），但满屏红叉读起来像「处处出错」。
     失败这件事本身必须留痕 —— 只是不该用「警报」的形状表达「过程里的正常一步」。
  2. **产出没有时间维度。** 约定是「新交付物写 `outputs/`」
     （`hermes-core/src/companion.rs:77`、`hermes-channel/src/system_prompt.rs:69`），
     于是所有日期的产出平铺在一个目录里。实测数据根：`workspace/outputs/` 下
     33 个文件横跨 8/3 ~ 9/13，加上平行的 `workspace/output/` 2 个（7/24 ~ 8/11），
     **一个日期文件夹都没有**。
     文件名自带日期全靠模型自觉，人找东西只能靠肉眼扫。
  3. **「我的材料」只认「你带来的」。** 它是 `hermes-sources` 的来源库
     （`SourceStore`，`crates/hermes-sources/src/lib.rs` 开篇写明：「你为以后对话留下的材料」），
     而产出落在 `workspace/outputs/`，两条线互不相认 —— 于是用户干完活，
     在「我的材料」里看不到自己刚产出的东西。
- **如何从真推出：** 真 1 → 把「失败」从**图标**降为**文字**，并且折叠标题上给一句
  带数量的说明（有几处没成），保留可展开的原文明细；不静默、不粉饰。
  真 2 → 约定本身加时间维度：新交付物写 `outputs/<YYYY-MM-DD>/`，
  让「按天找」成为结构而不是命名习惯。真 3 → 在同一个「我的材料」页里补一节
  **我产出的**，按日期分组；**不进** grounding 索引（产出是结果，不是你带来的底料，
  混进检索会把搭子自己的草稿当成依据）。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 每个跑长任务、每天要回看产出的用户。
- **解决什么痛点：** 长任务里正常的重试让人以为「怎么这么多错误」；
  产出全平铺在一个目录、按天找不到；干完活在「我的材料」里看不到自己的东西。
- **用完后用户多得到什么：** 失败可见但不刺眼；产出按天自动归位；
  在「我的材料」里一眼看到今天产出了什么，并能直接打开。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期
  - [x] 不增加无意义确认或噪音
  - [x] 空/载/错态完整（没有产出时明确说「还没有产出」）
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户跑一次采集/写作，跑完回看：这一趟成了没？产出的文件在哪？上次那份还找得到吗？
- **怎么走完：**
  - 失败：过程折叠标题写着「在工作区做事 · 有 2 步没成」；点开能看到是哪两步、
    原文错在哪儿。没有红色叉，也没有弹窗。
  - 产出：搭子把新交付物写进 `outputs/2026-09-13/财经资讯.md`；
    用户进「知识 → 我的材料」，下半节「我产出的」按 `2026-09-13` 分组列出，
    点名字直接用系统默认程序打开。
- **看起来怎么样：** 「我产出的」与「你带来的」在同一页、两块，各有小标题与空态；
  失败提示用中性色文字，不用红色图标；不新增弹窗、不新增页面。
- **好走 / 好看：** 产出要打开只点一次；按天分组，今天的在最上面。
- **成功标准：** 跑完一趟，能找到产出、能打开、失败不吓人。
- **明确不做什么：**
  - 不隐藏失败：文字 + 可展开明细都保留，只换表达方式。
  - 不把产出自动灌进 grounding 索引（materials ≠ 产出）。
  - 不迁移、不改名历史产出（旧文件留在原地，列表照样列出来）。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 表达层（失败用图标当警报）＋ 约定层（产出无时间维度）＋
  数据视图层（材料视图只认一个来源库）。
- **正确的长期默认路径：**
  - 失败与成功共用同一套「过程」语言，只是措辞不同；不引入新的警示色体系。
  - 产出目录 = `outputs/<日期>/`，日期是**结构**，不是文件名里的装饰。
  - 「我的材料」是**一个视图**，可以有多个来源；来源边界写在数据层，不写在 UI 里。
- **与引擎/各入口边界：** 约定文案在 `hermes-core::companion`（唯一身份源）与
  `hermes-channel::system_prompt`，改一处措辞两边同步；UI 与命令只在 GUI。
  server/Flutter 未见这份视图，不在本次范围（不假装做了）。
- **安全影响：** 新增的 `list_outputs` / `open_output` 只在 **workspace 内**解析路径
  （相对路径 + 规范化后必须落在 workspace 下），不复用导出目录豁免；
  只读与「用系统默认程序打开」，不新增写能力。
- **如何防复发：** 路径边界用纯函数 `resolve_workspace_file` 承载并加测试
  （含 `..` 逃逸、绝对路径、符号链接）；分组逻辑用纯函数
  `group_outputs_by_day` 承载并加测试。
- **为何这不是补丁：** 补丁是「把红叉换个颜色」「让模型自己记得加日期」；
  这里是**表达归位**（失败降为文字）＋**结构归位**（日期成为目录）＋
  **视图归位**（材料页容纳两类来源，且边界清楚）。

---

## 1. 方案（Plan）

- **目标：** 失败不吓人但看得见；产出按天归位；「我的材料」能看到并打开产出。
- **范围：**
  - **做：** 过程折叠的失败表达改文字 + 计数；约定文案改为 `outputs/<YYYY-MM-DD>/`；
    新增 `list_outputs` / `open_output`；「我的材料」加「我产出的」一节，按日期分组。
  - **不做：** 隐藏失败；自动把产出当材料喂检索；迁移或改名历史产出；
    改 server / Flutter 侧视图。
- **用户路径变化：**
  - 改前：失败 = 红叉 + 红字；产出平铺 `outputs/`；「我的材料」看不到产出。
  - 改后：失败 = 中性文字 + 「有 N 步没成」；新产出落 `outputs/<日期>/`；
    「我的材料 → 我产出的」按天列出、可打开。
- **技术要点：**
  - `crates/hermes-gui/ui/src/components/chat/MessageBubble.tsx`：过程标题与工具行。
  - `crates/hermes-gui/ui/src/i18n.ts`：新增「有 N 步没成」等键（中英各一）。
  - `crates/hermes-core/src/companion.rs`、`crates/hermes-channel/src/system_prompt.rs`：约定文案。
  - `crates/hermes-gui/src/commands/outputs.rs`（新）：`list_outputs` / `open_output`。
  - `crates/hermes-gui/ui/src/components/materials/MaterialsPanel.tsx`：新增一节。
- **风险与回滚：** 风险 = 旧产出平铺在 `outputs/` 根下，需要列表兼容两种形态
  （已在设计里覆盖；实测后改为「按文件自身时间」，见 §2 偏离 3）。回滚 = 还原改动。
- **方案确认：** [x] 已对照 P0/P1 · 2026-09-13 · 用户口头提出并确认方向

---

## 2. 实施（Implement）

- **实际改动摘要：**
  - **失败不喊**（`ui/src/components/chat/MessageBubble.tsx`）：过程折叠去掉 `XCircle`；
    非运行中且有过失败时，标题左侧不再放任何图标，改在标题后加中性灰文字
    「有 N 步没成」（新增键 `message.toolStepsFailed`）；工具行同样去掉 `XCircle`，
    失败状态字从红（`text-red-500`）降为中性灰（`text-app-fg-tertiary`）；
    「完成 / 运行中」两态不变。`XCircle` 已从该文件 import 中删净。
  - **约定文案改成带日期**：`hermes-core/src/companion.rs`（产品协议正文）、
    `hermes-channel/src/system_prompt.rs`（Dialogue 工作区段）、
    `hermes-tools/src/write.rs`（工具描述 + `path` 参数描述）、
    `hermes-core/src/paths.rs`（`WORKSPACE_OUTPUTS_DIR` 文档注释）。
  - **新命令** `crates/hermes-gui/src/commands/outputs.rs`：`list_outputs` / `open_output`。
    两个纯函数承载边界与顺序：`resolve_workspace_file`（拒绝绝对路径、`..`、
    符号链接逃逸；只认 workspace 内真实文件；**不复用**导出目录豁免）、
    `group_outputs`（日期降序、组内时间降序、读不出日期垫底）。
    `commands/source.rs` 的 `open_path` 改 `pub(crate)` 复用，不另写一份。
  - **注册**：两命令进 `main.rs` 的 `generate_handler!`；新增
    `crates/hermes-gui/tests/command_registration.rs` 审计「UI invoke 的命令必须注册」。
  - **面板**（`ui/src/components/materials/MaterialsPanel.tsx`）：「我产出的」一节加在
    「我带来的」之下（先材料后产出，与页头副标题一致）；按日期分组列出、可打开、
    空/载/错态齐全；搜索框同时过滤两节；产出**不进** grounding 索引。
- **关键路径/文件：** 见上。
- **偏离方案处（如实记）：**
  1. 约定文案并非「两处 / 唯一身份源」：实际落在 **3 个文件 4 处**（companion 的
     readonly 与完整版共用同一段字面量）。防漂移改为**测试锁措辞**：
     `companion::protocol_names_the_dated_outputs_default`、
     `write::spec_teaches_the_dated_outputs_default`、
     `system_prompt::dialogue_is_not_a_coding_playbook`（新增一行断言）。
  2. 分组函数实现名是 `group_outputs`（方案里写的是 `group_outputs_by_day`）。
  3. **实测推翻了方案里的「根下旧文件归『更早』组」。** 真实数据根 35 件产出
     （`outputs/` 33 + `output/` 2）**全是平铺**，一个日期文件夹都没有 —— 按原方案，
     今天刚产的 `财经资讯-2026-09-13.md` 也会被丢进「更早」，等于没按时间。
     改为：**有日期文件夹读文件夹，没有的读文件自身时间**（同一个问题：这份是哪天落下的）。
     读不出时间的才进兜底组，文案由「更早」改成「日期不明 / Date unknown」。
     实测结果：35 件 → 9 个日期分组，最新 `2026-09-13`（2 件），最老 `2026-07-24`（2 件），
     兜底组为空。
  4. 产出节放在材料节**下面**（方案 0b 写「下半节」，实现一致）。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 失败不喊 | 跑一个中间失败一次的任务 | 没有红叉；标题写「有 1 步没成」；点开见原文 | 代码线完成 · **待用户目视** | `XCircle` 已从 `MessageBubble.tsx` 删净 |
| 2 | 产出按天落盘 | 让搭子产出一份新东西 | 落在 `outputs/<今天>/` | 约定层完成（4 处文案 + 3 条锁测） · 真实落盘待用户跑一次 | 2026-09-13 起生效 |
| 3 | 材料页看得到产出 | 打开 知识 → 我的材料 | 「我产出的」按日期分组列出 | 数据层用**真实数据根**验证：35 件 → 9 组，最新 `2026-09-13` · UI 待重启目视 | §2 偏离 3 |
| 4 | 产出能打开 | 点一条产出 | 系统默认程序打开该文件 | 路径解析与 `open_path` 已验 · 点击待目视 | 复用 `source.rs::open_path` |
| 5 | 旧产出不丢 | 看 `outputs/` 根下的历史文件 | 仍在列表里 | ✅ 真实数据 35 件全在（`outputs/` 33 + `output/` 2） | 由「更早」改为按各自时间 |
| 6 | 路径不许逃逸 | `open_output ../../etc/passwd` | 拒绝 | ✅ `resolve_refuses_escapes_and_absolute_paths`（`..`、`a/../../b`、绝对路径、空、纯空白）；`resolve_refuses_symlinks_pointing_outside`（目录链接指向外部被拒，站内链接放行） | 纯函数测试 |
| 7 | 分组正确 | 日期目录 / 平铺 / 平行 legacy 混放 | 按日期降序，组内时间降序，读不出日期垫底 | ✅ `groups_are_newest_day_first_and_unfiled_last`、`newest_file_leads_its_day`、`day_folders_are_recognized_and_rubbish_is_not`、`flat_files_fall_back_to_their_own_timestamp`、`collects_dated_flat_and_legacy_files` | 纯函数测试 |
| 8 | 命令不漏注册 | UI 里每个 `invoke("…")` | 都在 `generate_handler!` 里 | ✅ `crates/hermes-gui/tests/command_registration.rs`：95 处调用 / 73 个命令名全覆盖；扫描器跳过字符串与注释，自身也有测试 | 新增防复发 |

- **自动化：** `cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
  `cargo test --workspace` · 前端 `npm run build`
- **手工：** GUI 里跑一趟并看「我的材料」。
- **测试结论：** 工程全绿 —— `fmt --check` 干净 · `clippy -D warnings` 无告警 ·
  `cargo test --workspace` **448 passed / 0 failed**（基线 437，本次 +11）·
  前端 `tsc && vite build` 通过，`ui/dist` 已重建。`target/debug/lebi-AI` 已重编。
  **用户可见面（红叉消失、「我产出的」分组、点击打开、真实落盘）未目视 —— 需重启 App 后由用户过一眼。**

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑（代码线） | 失败不再喊、产出按天可找可开；**用户可见面待目视** |
| 开箱即用未破坏 | ☑ | 不加依赖、不加数据库；产出节的空/载/错态齐全 |
| 本地优先未破坏 | ☑ | 只读本地 workspace；不联网 |
| 测试通过 | ☑ | 448 passed / 0 failed · clippy 无告警 · fmt 干净 · 前端构建通过 |
| 记录完整 | ☑ | 本文件 + `docs/records/README.md` 索引 |
| 产品+架构两视角齐全 | ☑ | 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 约定改在源头（提示词 4 处）+ 测试锁措辞；边界下沉到纯函数；不复用导出豁免 |
| 代码卫生：旧实现已清理 | ☑ | `XCircle` 从 `MessageBubble.tsx` 删净；`open_path` 复用而非复制；无新增死代码 |
| 操作与视觉：走查完成 | ☐ | 待目视 |
| 第一性原理：三步写全 | ☑ | 0-fp |

- **验收人：** 用户
- **验收日期：** 待定（工程侧 2026-09-13 自检完成）
- **结论：** ☐ 通过 · ☐ 驳回（原因：） —— **目视未做，不代签**
- **遗留项：**
  1. `OUTPUTS-RETRIEVAL`：产出不进 grounding 索引 —— 若用户希望「把昨天那份日报拿来」
     能被检索到，需要单独设计（可能要显式「把产出收为材料」的动作）。
  2. `OUTPUTS-OLD-FLAT`：`workspace/outputs/` 根下的历史文件与平行的
     `workspace/output/` 目录不做迁移，只做展示兼容。
  3. `SERVER-OUTPUTS-VIEW`：Flutter 侧未见此视图，本次未做。
