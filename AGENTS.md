# Agent / 开发者必读（项目共识入口）

> **本文件是仓库内所有 AI 助手与人类协作者的第一入口。**
> 做任何功能、重构、文档、打包决策前，先读完本节，再读权威文档。

---

## 唯一产品目标

**乐彼AI（lebi-AI）= 本地工作搭子 AI。**  
Slogan：**越用越像你的手感。**  
一句话：**接得住你的想法，推得动你的事，必要时敢顶你——第二次更准。**

- 完整定义卡：[`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md) 文首（**P0 v0.11**）
- **用户默认路径：** 下载桌面 GUI → 试用期内配置 API Key → **对话**（禁用「聊天」作主词）→ 共事 / 谋划 → 批准进化候选
- **不是**闲聊玩具、讨好机器、律师垂直、写代码专用产品
- **不是**「生活陪伴 / 情感陪聊」主叙事；主场在**工作**（谋划·表达·决策·复盘·推进）
- 关系用词：中文**搭子**，英文 **work companion**。禁止搭档 / 下属 / 工作伴侣
- 四环 Do × Continuity × Care × Evolve；爱上瞬间 ①～⑤ 见 P0 定义卡
- 进化是手段；对外主词是**越用越像你的手感 / 第二次更准**
- 授权是产品事实：试用 3 天 / 过期锁主能力 / 本机验签

**第一性原理（P0 第零条）。** 不准靠类比。方案必须写全：拒绝了什么类比 → 拆出的真 → 如何从真推出。写不出 → 不准开工。  
**一切站在用户角度。** 功能答不出「强化爱上①～⑤哪一条」→ 不做。

**每一次修复 / 改动必须同时用产品经理 + 架构师视角。**
**产品经理对用户操作与视觉负全责**（P0 第十一条）：方案必须写清怎么走、长什么样；用户可见面难看或难走 = 未完成。
**禁止修修补补**（只堵症状、堆特判、依赖环境碰巧可用）。
**代码必须高效、无冗余；更新/修改时必须同步清理旧实现、死代码、失效注释与过时入口——凡旧必清。**
细节：[`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md) 第七条 / **第九条**、[`DEVELOPMENT_RULES.md`](./DEVELOPMENT_RULES.md)「修复问题的工作方式」与 §六·附「代码卫生」。

### 技能分类（改 skill / 工具前必读）

**禁止**用「系统 skill vs 用户 skill」做设计分类。正式分类（P0 第八条）：

| 分类 | 是什么 | 位置 | 可否删除 |
|------|--------|------|----------|
| ① 引擎能力 | tools / prompt / 安全 — **不是** SKILL.md | `crates/hermes-core`、`hermes-tools` 等 | —（引擎契约） |
| ② 内置技能 | memory-palace、skill-creator、find-skills | `crates/hermes-skills/bundled/`，启动自动安装 | **否** |
| ③ 用户技能 | 用户自建 / 安装 | `~/.lebi-ai/skills/` | 可 |
| ④ 项目技能 | 项目内共享 | `./.lebi-ai/skills/` | 可 |

**Progressive Disclosure：** 索引只含 name + description；正文 `skill_read` 按需加载；
远程安装强制 `always_active=false`。
细节：[`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md) 第八条、[`DEVELOPMENT_RULES.md`](./DEVELOPMENT_RULES.md) §五·附。

若与目标冲突：**以产品目标为准。**

---

## 文档摆放铁律

### 仓库根目录只允许这 4 个 Markdown

| 文件 | 层级 |
|------|------|
| `PRODUCT_PRINCIPLES.md` | P0 |
| `DEVELOPMENT_RULES.md` | P1 |
| `AGENTS.md` | P2（本文件） |
| `README.md` | P3 |

- **禁止**在根目录新增任何其他 `*.md`
- **所有其他说明文档**必须放在 [`docs/`](./docs/)（见 [`docs/README.md`](./docs/README.md)）
- 变更台账：[`docs/records/`](./docs/records/)
- 非文档例外：代码内嵌运行时 `SKILL.md` 等资源

发现根目录出现多余 md → **移入 `docs/` 或删除**，并写台账。

---

## 变更铁律（每次必须）

```
方案（产品+架构） → 实施（正确设计） → 测试 → 验收 → 写入 docs/records/
```

1. **开工前：** 复制 [`docs/records/_TEMPLATE.md`](./docs/records/_TEMPLATE.md) 为 `docs/records/YYYYMMDD-slug.md`
   - 写清 **第零条三步**（拒绝的类比 / 拆出的真 / 如何推出）
   - 写清 **用户价值**
   - **产品经理：** 场景、用户怎么走完、看起来怎么样、空/载/错态、是否好看好走、成功标准、不做什么
   - **架构师：** 根因、正确默认路径、边界、如何按操作与视觉规格落地
   - 若只能写出类比、或「临时 if / 遮错 / 碰运气」、或写不出界面怎么走、长什么样 → **停手重做方案**
2. **实施中：** 按正确默认路径落地；禁止叠补丁；**同步删除旧路径/死代码/过时注释**；偏差写进记录
3. **实施后：** 用户语言测试 + 必要自动化
4. **收工前：** 质量门槛（含「非补丁」「代码卫生」）勾选；更新 [`docs/records/README.md`](./docs/records/README.md)

**无台账 / 无两视角方案 / 未验收 = 任务未完成。**

细则：[`DEVELOPMENT_RULES.md`](./DEVELOPMENT_RULES.md) §变更流程。

---

## 文档权威层级

冲突时序号小的覆盖大的：

| 优先级 | 文件 |
|--------|------|
| **P0** | [`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md) |
| **P1** | [`DEVELOPMENT_RULES.md`](./DEVELOPMENT_RULES.md) |
| **P2** | 本文件 `AGENTS.md` |
| **P3** | [`README.md`](./README.md) |
| **其他文档** | 仅允许在 [`docs/`](./docs/)，必须分级并服从 P0 |

禁止另起第二套产品说明；禁止把探索方案当现行目标；禁止 Docker/远程优先写成默认主路径。
方向变更：**先升 P0**，再同步本文与 P1。禁止只改 `docs/spec/work-companion-solution.md` 当第二宪法。
新说明必须先选种类 B–H（P0 第六条），放入对应目录。选不出种类就不写。

---

## 架构速记

```
crates/hermes-core     → 核心抽象（Session / LlmProvider / ToolHost / 压缩）
crates/hermes-llm/turn/tools/mcp → 引擎能力（provider、工具循环、内置工具、MCP）
crates/hermes-store/skills/memory/reflect → 明文存储 + 进化管线
crates/hermes-cli      → 引擎装配 / 调试入口（全量子命令）
crates/hermes-gui      → 桌面 GUI（Tauri 2，同引擎同数据）· **用户默认路径**
crates/hermes-server   → Flutter 后端（REST/WS，bearer token；GUI 子集 + WS 对话帧，非 1:1）
clients/flutter        → 移动客户端（iOS/Android/macOS）
crates/hermes-weixin/feishu/telegram → IM 渠道（共享 channel.rs 驱动）
scripts/run-gui.sh     → **打开 GUI 的默认命令**（先 build ui/dist 再启动）
~/.lebi-ai/         → **本产品**用户数据（与律师版 `~/.lebi-law` 隔离）
LEBI_DATA_DIR           → 可选，覆盖数据根
docs/                  → 非权威文档唯一位置
docs/records/          → 变更与验收台账
```

### 打开桌面 GUI（默认路径 · 防白屏）

**正确默认（协作 / Agent 必须用）：**

```bash
# 仓库根
scripts/run-gui.sh              # npm build ui/dist → cargo run -p hermes-gui
# 或等价：
cd crates/hermes-gui/ui && npm install && npm run build && cd - && cargo run -p hermes-gui
```

| 规则 | 说明 |
|------|------|
| **界面从 `ui/dist` 加载** | `tauri.conf.json` **不得**把 `devUrl: http://localhost:5173` 设为默认；依赖碰巧开着的 Vite → 白屏 |
| **改 UI 源码后** | 必须再 `npm run build`（或再跑 `scripts/run-gui.sh`），否则仍是旧 dist |
| **禁止** | 只 `cargo run -p hermes-gui` 却指望 5173；未 build 前端就当「能开」 |
| **热更新（可选）** | 仅本地临时需要 HMR 时再开 Vite + 临时 devUrl；**不得**写进默认配置或用户路径 |

细节：[`docs/dev/gui-run.md`](./docs/dev/gui-run.md)。

---

## 改代码前自检

0. **第一性原理：** 拒绝了什么类比？拆出的真是什么？做法如何从真推出？写不出 → 停手
1. 用户要额外装数据库 / 中间件吗？
2. 要打开终端才能用吗？（CLI 场景除外；GUI/手机应免终端）
3. 对用户是否**更好用、更好看、更高效、更能进化**（Do/Continuity/Care/Evolve 至少强化一环）？
4. 数据是否仍本地明文（除用户 AI API 与主动触发的 Web/MCP）？
5. 是否共享引擎而非为某入口 fork 平行逻辑？各入口身份是否仍是 **companion 协议**（`hermes-core::companion`）？
6. server 是否仍默认本机 + token？公网是否 TLS？密钥是否 0600？
7. 是否已建/更新 `docs/records/` 并准备验收？
8. 新增 md 是否先选 B–H、放入对应目录、且未另立产品目标？方向变更是否**先升 P0**？
9. **产品经理视角**是否写清操作与视觉（先看见什么、点什么、空/载/错态、是否好看好走）？
10. **架构师视角**是否写清（根因/默认路径）？是否按操作与视觉规格实现？
11. 若动 skill/工具/提示词：是否已归入 **①②③④**？是否误用「系统/用户 skill」二分？
12. 技能是否遵循 Progressive Disclosure？远程技能是否强制 `always_active=false`？
13. **代码是否高效、无冗余？** 旧实现 / 死代码 / 注释尸块 / 过时 i18n 与 invoke 是否已清理？
14. **文档是否诚实？** README / P3 与 docs 是否声称未实现的路径与入口？（未接线 → 标「计划中」或接线）
15. **lint / test 是否全绿？** `cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace` 是否通过？
16. **若动 GUI：** 默认是否仍走 `ui/dist`？是否未把 `devUrl`/5173 写回默认？打开方式是否 `scripts/run-gui.sh` 或「先 npm build 再 cargo run」？
17. **文案：** 中文主词是否为「对话」而非「聊天」？关系是否仍是「搭子」而非「搭档/工作伴侣」？是否无锚假装记得、无交付强迫打磨？
18. **权威：** 是否未另立品类/口号/主路径？P1/本文文首是否对齐 P0 **v0.11**？权威正文是否只写现行规则？新 docs 是否已标明种类 B–H？

任一「否」→ 停手。

---

## 与各入口关系

**用户默认路径是桌面 GUI。** CLI 是引擎装配 / 调试入口。
Flutter（经 `hermes-server`）/ 微信 / 飞书 / Telegram 是同等引擎的表面：已接线、**非默认交付**。
共享同一引擎与同一份数据，**不是**平行产品。禁止某一入口独占引擎逻辑。
产品身份协议的**唯一源码**：`crates/hermes-core/src/companion.rs`（必须与 P0 定义卡同词）。

*对齐 PRODUCT_PRINCIPLES **v0.11** / DEVELOPMENT_RULES **v0.6** · 2026-08-14*
