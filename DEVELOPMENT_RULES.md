# 开发规则（P1 权威）

> **版本：** v0.1
> **更新日期：** 2026-08-06
> **前提：** 服从 [`PRODUCT_PRINCIPLES.md`](./PRODUCT_PRINCIPLES.md)。
> **入口：** 协作/Agent 先读 [`AGENTS.md`](./AGENTS.md)。

---

## 一、角色定位

```
用户（个人用户 / 开发者）—— 只关心「好不好用、快不快、变不变聪明」
产品经理视角       —— 路径、价值、成功标准、边界
架构师视角         —— 根因、默认路径、边界、可演进
开发者（你我）     —— 按正确设计实现 + 可验收交付
```

用户只做：拿二进制 → 配置 API Key → 对话 / 使用 → 确认进化候选。其余由我们完成。

**铁律：**

1. **一切站在用户角度** — 价值 = 更好用、更高效、更能进化
2. **修问题必须产品经理 + 架构师双视角** — 禁止修修补补（见 P0 第七条）
3. **代码高效、无冗余；改完即清旧** — 凡被替换的实现、注释、入口、文案必须同次清理（见 P0 第九条、§六）

---

## 一·附、修复问题的工作方式（强制）

**每一次修复 / 变更**，方案阶段必须显式写清：

| 视角 | 必答 |
|------|------|
| **产品经理** | 用户场景、路径变化、成功标准、明确不做什么 |
| **架构师** | 根因层级、正确默认路径、与引擎/各入口边界、如何防复发 |

### 禁止

- 只改 UI 遮错、只加 sleep/重试、只 if 特判某一环境
- 依赖「本机碰巧开着 Vite / 碰巧有某目录」
- 同一问题连续打补丁却不收敛设计

### 允许

- **最小正确实现**：根因清楚后，改动可以小，但是正确默认路径上的实现

方案记录（`docs/records/`）中若缺少上述两视角，**不得进入实施**。

---

## 二、用户思维（决策前必问）

| 环节 | 用户操作 | 我们的责任 |
|------|---------|-----------|
| 获取 | clone 或下载二进制 | 构建可复现、单文件可分发（Docker 镜像可选） |
| 配置 | 写 `~/.lebi-ai/config.toml`（`hermes init` 引导） | 默认配置可运行；密钥 0600 |
| 对话 | CLI / GUI / 手机 / 微信 / 飞书 / Telegram | 多入口同一体验、同一数据 |
| 进化 | 确认 / 拒绝 skill、memory、conflict 候选 | 候选可读、可审；落盘为明文 |
| 检索 | 查看 skills / memories / sessions | 命令清晰、结果可读 |

### 好用 / 高效标准（用户侧）

| 维度 | 达标含义 |
|------|----------|
| 好用 | 单一二进制、无数据库；不依赖技术背景也能在 GUI/手机端使用 |
| 高效 | 高频路径步骤最少；reflection 不阻塞；结果可直接用 |

### 禁止
- 让用户装数据库 / 消息队列 / 额外运行时（Rust 工具链只用于从源码构建）
- 为「技术优雅」牺牲步骤或清晰度
- 无用户价值的功能堆砌

---

## 三、文档位置（强制）

### 根目录 Markdown 白名单（仅 4 个）

```
PRODUCT_PRINCIPLES.md
DEVELOPMENT_RULES.md
AGENTS.md
README.md
```

- **禁止**在仓库根新增其他 `*.md`
- **所有其他说明文档**必须放在 [`docs/`](./docs/)，并由 [`docs/README.md`](./docs/README.md) 索引
- 变更台账：[`docs/records/`](./docs/records/)
- 例外：代码树内运行时资源（如 `crates/hermes-cli/src/skills/**/SKILL.md`）

违规处理：移入 `docs/` 或删除 + 写 `docs/records/` 记录。

---

## 四、变更流程（强制 · 不可跳过）

**每一次修改**必须完整走：

```
① 方案  →  ② 实施  →  ③ 测试  →  ④ 验收
```

并在 **`docs/records/`** 落盘。模板：[`docs/records/_TEMPLATE.md`](./docs/records/_TEMPLATE.md)。

| 阶段 | 必须产出 | 完成标志 |
|------|----------|----------|
| **方案** | 用户价值 + **产品经理路径/成功标准** + **架构师根因/默认路径** + 范围与风险 | 自检通过 P0 清单；两视角写全 |
| **实施** | 代码/配置/技能按**正确设计**落地（非补丁）；**同步删除旧路径与死代码** | 与方案一致或记录偏差；diff 可见「删旧」 |
| **测试** | 用户语言用例 + 自动化/手工结果 | 关键路径通过 |
| **验收** | 质量门槛全勾选 | 状态「已验收」；未通过=未完成 |

### 质量门槛（验收必须全部为是）

1. **用户价值：** 写得出用户因此更好用 / 更高效 / 更能进化
2. **开箱即用：** 未引入数据库 / 常驻中间件 / 额外运行时要求
3. **本地优先：** 未把用户数据默认送上非 AI 的第三方；密钥仍 0600
4. **测试通过：** 表中关键用例通过（或明确缩小范围并记录）；`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace` 必须全绿
5. **记录完整：** `docs/records/YYYYMMDD-*.md` 四阶段写全
6. **文档位置：** 未在根目录滥放 md
7. **非补丁：** 产品路径与架构根因已对齐；默认路径正确，不靠环境运气
8. **代码卫生（P0 第九条）：** 高效、无冗余；旧实现/死代码/注释尸块/过时入口已清理；默认路径单一

**未达门槛 = 不得视为完成。**

### 记录规则

- 路径：`docs/records/YYYYMMDD-slug.md`
- 索引：更新 `docs/records/README.md`
- Agent 收工前**必须**写好或更新台账

---

## 五、产品形态

```
lebi-AI（乐彼AI）= 共享引擎（crates/*）+ 多入口（CLI / GUI / Flutter / IM / server）
```

| 层 | 路径 | 说明 |
|----|------|------|
| 核心抽象 | `crates/hermes-core` | Session / LlmProvider / ToolHost / 上下文压缩；不依赖 UI 与传输 |
| 引擎能力 | `crates/hermes-llm`、`hermes-turn`、`hermes-tools`、`hermes-mcp` | 多 provider、工具循环与权限、内置工具、MCP 客户端 |
| 记忆与进化 | `crates/hermes-store`、`hermes-skills`、`hermes-memory`、`hermes-reflect` | 明文存储、技能域、记忆宫殿、reflection 管线与 distill |
| CLI | `crates/hermes-cli` | 主入口与全部子命令；内置 `skill-creator` / `find-skills` 元技能 |
| 桌面 GUI | `crates/hermes-gui`（Tauri 2） | **主交付面**；与 CLI 同引擎同数据；确认弹窗 / 记忆侧栏 / 技能 CRUD / 设置 / session-end reflection |
| GUI 启动脚本 | `scripts/run-gui.sh` | **打开 GUI 默认路径**：build `ui/dist` 后 `cargo run`；禁止默认依赖 :5173 |
| 移动客户端 | `clients/flutter` | 三端（iOS/Android/macOS）；后端 = `hermes-server` |
| HTTP/WS 服务 | `crates/hermes-server` | Flutter 后端；bearer token 鉴权；路由与 GUI commands 1:1 |
| IM 渠道 | `crates/hermes-weixin`、`hermes-feishu`、`hermes-telegram` | 共享渠道驱动层（`channel.rs`），仅协议差异 |
| 文档 | `docs/` | 非权威说明唯一区 |
| 台账 | `docs/records/` | 变更与验收 |

### 五·附、技能分类（执行细则 · 服从 P0 第八条）

改 skill / 工具 / 提示词前，**必须先归层**：

| 你想改的是… | 应落在 | 正确位置（示例） |
|-------------|--------|------------------|
| 读写文件、bash、git、web 搜索、确认策略、workspace 边界 | ① 引擎能力（不是 skill） | `hermes-tools` / `hermes-core` / 权限配置 |
| 记忆宫殿协议、造 skill、找 skill | ② 内置技能（bundled） | `crates/hermes-cli`（`memory-palace` 生成、`skill-creator` / `find-skills` 嵌入） |
| 用户自己的工作流 / 领域知识 | ③ 用户技能（User scope） | `~/.lebi-ai/skills/` |
| 项目内约定 / 团队共享 | ④ 项目技能（Project scope） | `./.lebi-ai/skills/` |

**禁止（执行）：**

- 把 ① 引擎能力写成可卸载 SKILL.md 冒充「内置技能」
- 用「系统 skill vs 用户 skill」二分糊掉 bundled / user / project 的所有权
- 删除 / 覆盖内置技能名（`memory-palace` / `skill-creator` / `find-skills`）——启动即重装
- 远程技能默认 `always_active=true`（安装时强制关闭）

**Progressive Disclosure（强制）：**

- 索引只含 name + description（+ triggers）
- 正文仅在触发时经 `skill_read` 读取；配套 `scripts/` / `references/` / `assets/` 按需读取
- 安装配额：≤50 文件 / 单文件 ≤100 KB / 总量 ≤5 MB；路径深度 ≤6；校验 `..`、绝对路径、保留名
- 安装为事务写入：临时目录组装 → 原子 rename，失败不留半成品

---

## 六、工程规范

1. **单一二进制**：无数据库、无消息队列；构建自包含（Docker 镜像可选）
2. **明文本地存储**：会话 JSONL；技能 / 记忆 Markdown + YAML frontmatter；密钥文件 0600
3. **多入口同契约**：`hermes-server` 路由与 `hermes-gui` commands 1:1；DTO 不得各自漂移
4. **server 安全**：默认 `127.0.0.1` + bearer token 必填；公网必须反代 TLS（见 `docs/REMOTE_ACCESS.md`）
5. **默认 UI 语言 en-US**（`config.toml` 的 `ui.language`，支持 `zh-CN`）
6. **特别危险才确认**：工作区内常态读写 / 普通 shell / memory_save 默认放行；破坏性 shell、远程 skill_install、删记忆/技能、subagent、未知 MCP 等须 confirm 并说明原因；会话级 "Always Allow" 仅存于进程内；workspace 硬边界始终生效
7. **上下文纪律**：skill 正文不注入系统提示（发现 → 激活 → 执行）；pinned 记忆才常载
8. **reflection 纪律**：候选默认**不自动写入**；用户确认才落盘；distill 先 dry-run 后 `--apply`；**配置默认值必须与 P0 一致**（`auto_accept_memories` 默认 `false`，仅当用户显式开启才自动写入，且入口间批准语义一致）
9. **知识库收敛**：distill 聚类近似记忆；supersedes 链取代旧记忆；effectiveness 可追踪
10. **代码卫生**（服从 P0 第九条）——见下节

### 六·附、代码卫生：高效 · 无冗余 · 改完即清旧（强制）

> 写代码与改代码同一标准。Agent / 人类均适用。

#### 高效

- 热路径避免重复读盘、重复 JSON 解析、无意义全量拷贝
- 禁止用 sleep/盲重试掩盖竞态；先修契约与状态机
- 列表 / 流式 UI：不在无关重渲染里做重计算

#### 无冗余

- **单一真相源：** 同一业务规则只在一处定义（工具、权限、文案、路径约定）
- 禁止「复制一个模块改两行」长期并存；提取共享或删掉旧的
- 新抽象仅在第二次真实重复时引入，禁止为想象中的扩展提前叠层

#### 改完即清旧（凡旧必清）

同一次 PR / 同一次 `docs/records` 实施阶段内，必须处理：

| 清理对象 | 示例 |
|----------|------|
| 旧实现 | 被替换的 fn、分支、feature flag 死枝 |
| 旧入口 | 菜单 / 路由 / Tauri command 注册、CLI 子命令、Agent 工具名 |
| 旧调用 | 权限 allow 列表、system prompt 纪律、前端 invoke 名 |
| 死导出 | 未使用的 `pub`、组件、样式、i18n key |
| 注释尸块 | 整段注释掉的代码；「以前是…现在改成…」而无信息量的注释 |
| 过时说明 | 仍描述已删除路径的注释、docs 片段（权威文档随 P0 升版） |

**允许暂时双轨的唯一条件：** 台账写明删除期限与负责人；到期未删 = 变更未完成。默认**不允许**双轨。

#### 实施自检（收工前）

1. 本次默认路径是否只有一条？
2. `rg` / 全局搜旧名是否还有业务引用？
3. 是否留下注释掉的旧代码？有则删。
4. diff 是否同时包含删除？只有新增没有删除 → 怀疑堆叠。

---

### 六·附 B、规则一致性（开发前锁定 · 2026-08-03）

> 来自开发前全面审查（`docs/records/20260803-pre-dev-review-rules.md`）。三条基线规则：

1. **配置默认值与权威行为一致** — 任何「自动写」「默认开」的配置项，其默认值必须符合 P0
   （进化候选默认不自动写入；危险操作默认需确认）。发现默认值偏离 → 改默认值，不改 P0。
2. **文档诚实** — README / P3 与 `docs/` 不得声称未实现的路径与入口；未接线的功能要么
   接线，要么明确标「计划中 / roadmap」，验收时逐条核对。禁止「文档先吹、代码后补」。
3. **lint / test 硬门槛** — 验收前 `cargo clippy --workspace --all-targets -- -D warnings`
   与 `cargo test --workspace` 必须全绿；CI（`.github/workflows`）补上后由 CI 兜底，
   本地命令与 CI 行为一致。

### 六·附 C、桌面 GUI 默认加载路径（防白屏 · 强制）

> 对齐律师版实践与 P0「禁止环境运气依赖」。冲突时以本节 + P0 为准。

| 必须 | 禁止 |
|------|------|
| 默认从 `crates/hermes-gui/ui/dist` 加载界面 | 默认 `devUrl: http://localhost:5173` |
| 打开 GUI：`scripts/run-gui.sh`，或先 `npm run build` 再 `cargo run -p hermes-gui` | 只 `cargo run -p hermes-gui` 却依赖碰巧开着的 Vite |
| `tauri.conf.json` 的 `beforeBuildCommand` 构建前端（打包路径） | 把 HMR/5173 写成 README 主路径 |
| 改 `ui/src` 后重建 dist 再验收 | 改了前端却只重启 Rust 二进制当「已生效」 |

**根因备忘：** Tauri 若配置了 `devUrl`，**debug** 的 `cargo run -p hermes-gui` 会去连 Vite；5173 未开 → **整页白屏**。  
正确设计：默认不设 `devUrl`，始终 `frontendDist`；热更新仅可选、临时、写在 `docs/gui-run.md`。


## 七、共识防漂移

- 禁止平行「第二产品愿景」
- 禁止 Docker / 远程优先叙事冲本地单二进制主路径
- 禁止根目录堆文档
- 改共识：升 P0 版本 → 同步 P1/AGENTS → 检查 README → **写 docs/records**

---

## 八、工作方式

1. 用户视角 → 2. 产品视角 → 3. 方案 → 4. 实施 → 5. 测试 → 6. 验收落盘

分歧：用户体验 > 技术优雅；本地数据 > 云端默认；进化可审 > 静默累积。

---

## 九、避坑清单

| # | 问题 | 教训 |
|---|------|------|
| 1 | 让用户装数据库 / 消息队列 | 单一二进制，明文文件即可 |
| 2 | 原则放深层目录 | 放仓库根（仅权威） |
| 3 | 把开发者当唯一用户 | 用户是个人用户 + 开发者，GUI/手机入口要可用 |
| 4 | 各入口复制引擎逻辑 | 共享 `hermes-core`；server 与 GUI commands 1:1 |
| 5 | README 旧叙事 | 只认 P0/P1 |
| 6 | 改完不测不验收 | 无 docs/records = 未完成 |
| 7 | 根目录堆 md | 一律进 docs/ |
| 8 | server 裸奔（无 token / 明文公网） | 默认本机 + token；公网必须反代 TLS |
| 9 | 「系统 skill / 用户 skill」二分 | 用 bundled / user / project 三分类；引擎能力不是 skill |
| 10 | 远程技能默认注入系统提示 | 安装强制 `always_active=false` |
| 11 | 新旧实现双轨 + 注释尸块 | 改完即清旧（P0 第九条）；禁止堆叠 |
| 12 | reflection 静默自动写入 | 候选必须用户确认后才落盘 |
| 13 | GUI 白屏：debug 连 5173 / 未 build dist | 默认 `ui/dist`；`scripts/run-gui.sh`；禁止默认 devUrl |

---

## 十、目录约定

```
/
├── PRODUCT_PRINCIPLES.md     ← 根：仅权威
├── DEVELOPMENT_RULES.md
├── AGENTS.md
├── README.md
├── docs/                     ← 其他所有说明文档
│   ├── README.md
│   └── records/              ← 变更台账
├── crates/                   ← 共享引擎 + 各入口
├── clients/flutter/          ← 移动客户端
└── scripts/
```
