# 变更记录：第三批 · 结构（发布链 · 查重闸门 · 双队列合一 · 输入折叠 · 死代码）

| 字段 | 内容 |
|------|------|
| **编号** | `20260919-batch3-structure` |
| **日期** | 2026-09-19 |
| **状态** | **已验收**（2026-09-19 统一签收 · 依据与未验项见 [`20260919-acceptance-sweep`](./20260919-acceptance-sweep.md)）；原状态：待验收（代码 + 脚本已落地，全量门禁见 §3） |
| **负责人** | 主线（Codex） |
| **关联** | 复审 [`20260918-reaudit`](./20260918-reaudit.md) §七「第三批」；上一批 [`20260919-batch2-visible-fixes`](./20260919-batch2-visible-fixes.md) |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：**
  - 不是「审计清单逐条打勾」——打勾不等于客户机器上那件事真的会发生。
  - 不是「扫描器说没引用就删」——扫描器读不出「写在文档里的以后要做」。
  - 不是「本地跑得起来就算发布链没问题」——构建机的家目录不在客户机上。
- **拆出的真：**
  1. **发布链决定的是客户机器上的现实**，而它错了用户**看不见**：只会表现为「Intel Mac 装不上」「点了检查更新没反应」「文档导入静默失败」。所以发布链的每一条规则都要能**在没有用户在场时自己失败**（校验脚本 + 退出码），不能靠人记得。
  2. **只要密钥出现在 URL 里，它就会出现在反代/网关的 access log 里**。只有 WebSocket 握手这一处「无处放 header」是真实的例外；其余地方带着长期密钥的查询串都是把长期密钥写进日志。
  3. **「构建机上能跑」≠「客户机上能跑」**：`venv` 的 `bin/python` 是指向构建机 uv 缓存的绝对软链，`pyvenv.cfg` 记着同一个 home；打进安装包以后客户机上两个路径都不存在。
  4. **删除要分清「遗物」与「有据可查的以后」**：`embed` feature 与 `Persona::is_fallback()` 在文档里写着「以后要接 / 阶段 3 的 API」，它们是**未启用**，不是**无主**。
- **如何从真推出：** 发布链上「能自己失败的规则」→ 新增 tag/version 校验脚本，并去掉「先删后建」；auth 上「只留真实例外」→ `?token=` 收窄到 WS 握手；安装包上「不留构建机」→ 把解释器 vendor 进 sidecar 并用 `PYTHONPATH` 驱动；删除上「先查有没有书面意图」→ 逐条核文档后再动手。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 拿到安装包的客户（macOS / Windows）、以及发版的我们。
- **解决什么痛点：**
  - Intel Mac 的客户以前**根本收不到更新**（线上 `latest.json` 只有 `darwin-aarch64`）。
  - 安装包里的文档转换器（markitdown）绑着构建机绝对路径——**在客户机上必然失败**，且失败现场不像「缺个 Python」。
  - 长期 server token 可以走 `?token=` 打在任意 REST 上，被反代原样写进日志。
  - 内存里塞着两条并行的「待审队列」，同一条记忆进哪个队列看调用点心情。
- **用完后用户多得到什么：**
  - 装包即用：苹果芯片与 Intel 都装得上、都能更新。
  - 文档导入（docx / pdf / xlsx）在客户机上**开箱可用**，不需要客户装 Python。
  - 长会话不再因为「一次请求把整篇旧工具输出再塞进去」而变慢、变贵。
  - 记忆不会因为「换了入口」而重复落两条。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（Python 已在包内）
  - [x] 步骤可感知、可预期（失败有中文原因，不是空白）
  - [x] 不增加无意义确认或噪音
  - [x] 主操作一眼能找；空/载/错态完整
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 客户下载 `lebi-AI_<版本>.dmg` → 拖进 Applications → 打开 → 设置里填 API Key → 回「资料员小文」上传一份 `会议纪要.docx` 让它整理。
- **怎么走完：** 打开 App（首次可能被 Gatekeeper 拦，见遗留项）→ 侧栏选工位 → 上传 docx → 转换器在**本机包内**跑出 Markdown → 进入会话；转换器缺失/失败时，界面给出明确中文原因，而不是静默不产出。
- **看起来怎么样：** 这一步没有新增界面；改动全部在「用户看不见但一定会撞上」的地方（更新、导入、发版）。因此**验收靠行为**：Intel 机器能更新、客户机 docx 能转、授权码输入后名册立刻变化。
- **好走 / 好看：** 没有新增步骤；反而少了一类失败（「导入没反应」）。
- **成功标准：** ① `v1.4.0` tag 与 `tauri.conf.json` 不一致时发版**直接失败**；② 线上 `latest.json` 同时有 `darwin-aarch64` 与 `darwin-x86_64`；③ 把 sidecar 拷到**另一个目录**仍能转换 docx。
- **明确不做什么：** 本次**不做** Apple 代码签名 / 公证（需要 Apple Developer 证书，我们没有）；**不做** Intel 可用的通用 sidecar（见 §4 遗留）。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  - 发布链：**脚本与 CI 配置**（写死 tag、先删后建、只出单架构、镜像里没有的 Python 却被打包）。
  - 安全：**中间件的判据太宽**（`?token=` 对全路由生效）+ **引进来却没挂载的 feature**（`tower-http/cors`）。
  - 存储：**同一个语义有两套实现**（`inbox.jsonl` 与 `deferred.jsonl`）。
  - 引擎：**只在输出侧折叠**，输入侧（历史 tool_use / text 块）还会把整篇旧内容再送一遍。
- **正确的长期默认路径：**
  - 发版：`tag == tauri.conf.json version` 是**硬门槛**；Release 只 upsert，绝不先删；macOS 产物是**通用包**，`latest.json` 同时声明两个 darwin 架构键。
  - 安装包：sidecar **自包含**（解释器在包内、只用相对路径/PYTHONPATH），构建机路径不得进入产物。
  - 认证：REST 只用 `Authorization: Bearer`；WS 优先一次性 `?ticket=`；长期 `?token=` **只**在 WS 握手被接受。
  - 队列：**一条队列**，原子写。
- **与引擎/各入口边界：** 只改 `hermes-server` 的认证判据、`hermes-*` 的清理与折叠、`scripts/` 与 CI；GUI / CLI / Flutter / IM 仍共享同一引擎与同一数据根，没有为某入口分叉。
- **安全影响：** `?token=` 不再对 REST 生效（长期密钥少一个泄漏面）；`cors` feature 删除（依赖更小）；TLS 仍是**反代**要求，server 非 loopback 时保留 warn 并在文档写明——本次**没有**引入 rustls（见遗留）。
- **如何防复发：** `scripts/check-release-tag.sh` 成为 CI 硬步骤；`write-latest-json.sh` 的 darwin 架构键变成显式参数并有注释说明 updater 的取值规则；`prepare-markitdown-bundle.sh` 自带「搬迁 + 校验」并会在产物里搜构建机路径；`store.rs` 的查重闸门落在 `put()` 里，任何调用点都绕不过。
- **为何这不是补丁：** 每一条都落在**唯一的默认路径**上（发版脚本、`put()`、唯一队列、wrapper），而不是在某个调用点加特判。

---

## 1. 方案（Plan）

- **目标：** 收掉复审 §七「第三批 · 结构」⑧–⑫，并顺带收 P1-8（安全）与 P1-12 的余项；用户另点的一项是把 `test/workspace/` 里上一代律师版产物清理掉。
- **范围：**
  - **做：** ⑨ 查重下沉 `FsMemoryStore::put`（P1-4）；⑩ 双队列合一 + inbox 原子写（P1-5）；⑪ 发布链（P1-7）；⑫ 输入侧折叠 + `web_fetch` 截断对齐（P1-1 / P1-14）；P1-2 桌面端注入编译档案；P1-8 的 `?token=` 与死 `cors`；P1-12 的死 `pub fn`；`test/workspace/` 旧产物搬走。
  - **不做：** Apple 签名/公证；Intel 可用的通用 Python sidecar；P1-13 项目专属归档区；`embed` feature（有书面意图，保留）；server 自带 TLS（仍走反代）。
- **用户路径变化：**
  - Intel Mac：以前「装了 1.4.0 之后再也收不到更新」→ 现在与苹果芯片同一份通用包、同一个 `latest.json` 覆盖。
  - 客户机文档导入：以前「必然失败（找不到构建机的 Python）」→ 现在包内解释器直接可用。
  - 记忆收下：以前「同一句偏好能从两个入口各进一条」→ 现在 `put()` 一处拦近重复。
- **技术要点：** `crates/hermes-{core,memory,tools,reflect,channel,gui,server,cli}`；`scripts/{check-release-tag.sh,write-latest-json.sh,prepare-markitdown-bundle.sh}`；`.github/workflows/release.yml`；`docs/{dev/REMOTE_ACCESS.md,spec/projects.md}`；`crates/hermes-gui/resources/README.md`。
- **风险与回滚：**
  - 发布链改动**无法在本地端到端演练**（需要 CI + secrets）——回滚 = `git checkout` 那几个脚本/工作流。
  - sidecar 搬迁改变了包内布局（`venv/` → `python/` + `site-packages/`）——回滚 = 还原脚本并重跑 `prepare-markitdown-bundle.sh --force`。
  - `?token=` 收窄会拒绝「老客户端在 REST 上带 token」——Flutter 的回退只发生在 WS 上，已核对；服务器端测试同步改成断言 401。
- **方案确认：** [x] 已对照 P0 **v0.12** / P1 **v0.6** · 2026-09-19 · 用户「1 清理掉 / 2 开始第三批」

---

## 2. 实施（Implement）

- **实际改动摘要：**

  **① 用户点的「清理掉」（`test/workspace/` 旧产物）**
  上一代律师版留在工作区根的东西**移走**（不是硬删，全部可回滚）到 `~/Documents/codeINDEx/test/_trash-20260919/`：`analysis_report.md`、两份 `会议纪要_*.docx`、`短视频运营手册.md`、`perm-test.txt`、3 个 `generate_*.py`、`output/`、`案例分析与知识保存/`、`caocao_analysis/`、`symptom-screening-workspace/`、`数据/`、`documents/`、`~/`、`.DS_Store`。
  保留用户真实资产：`.tmp_xw/`、`.tmp_xw2/`、`.upload_tmp/`、`TODOS.md`、`outputs/`、`uploads/`。

  **② P1-4 查重闸门下沉到 `put`**
  `FsMemoryStore` 带 `dedup_threshold`（默认 `DEFAULT_DEDUP_THRESHOLD`），`put()` 落盘前跑近重复判断，**除非** `supersedes` 非空或本次是「有意」写（`MemoryFrontmatter::intentional`，`#[serde(skip)]`，只属于这次调用，不落盘）。工具层删掉预检、改为把 `Conflict` 翻译成人话；反思入队与 micro-apply 遇到 `Conflict` 记为「已经知道」而不是失败；GUI 三个命令映射成 `memory_duplicate`，前端新增 `utils/errorText.ts` 统一翻人话（未知错误**不遮**）。

  **③ P1-5 双队列合一**
  `inbox` 成为唯一队列（`enqueue_candidate*` / `list_at` / `clear_at` / `prune_low_quality_at`，**原子写**：先写 `.json.tmp` 再 rename）；`deferred.rs` 整个删除，`MicroApplyConfig.queue_deferred` → `enqueue_pending`、`.inbox_only()` → `.caller_enqueues()`；CLI 会把旧 `deferred.jsonl` 并进唯一队列后删除。

  **④ P1-1 / P1-14 输入侧折叠 + 截断对齐**
  `ToolOutputFold` → **`RequestFold`**：`ToolResult.content`、`ToolUse.input`（只换叶子，JSON 仍合法）、`Text.text` 三种块都折；`recent_chars` 默认 8 000 → **20 000**（= `web_fetch::default_max_chars()`，两侧注释互相点名）；`session_recall` 的 `limit` 上限 20 → 200（否则折叠文案里「用 session_recall 翻旧账」是空头支票）。

  **⑤ P1-2 桌面端注入编译档案**
  `ContextSources` 加 `compiled_profile`，`build_turn_system` 三层顺序与 CLI 对齐（主题卡 → 编译档案 → 平索引）；GUI 与 server 每轮现读 `hermes_memory::load_profile()`。

  **⑥ P1-7 发布链**
  - `workflow_dispatch.inputs.tag` 删掉写死的 `default: "v1.2.0"`，改 `required: true`。
  - 新增硬步骤 **`scripts/check-release-tag.sh`**（比对 tag 与 `crates/hermes-gui/tauri.conf.json` 的 `version`，不一致直接失败并给出两种修法）。
  - 删掉 `gh release delete`（客户端 updater 打的是 `releases/latest/download/latest.json`，先删后建会让正在检查更新的人看到 404），改 upsert + 资产上传后单独 `gh release edit --notes-file` 刷新正文。
  - macOS job 加 `rustup target add x86_64-apple-darwin` + `TAURI_TARGET: universal-apple-darwin`（出通用包）。
  - `scripts/write-latest-json.sh`：默认同时输出 **`darwin-aarch64` 与 `darwin-x86_64`**，指向同一份通用 `.app.tar.gz`（updater 的键是 `${os}-${运行架构}`，源码里没有 `darwin-universal`；注释写明出处）。
  - **sidecar 自包含**：`prepare-markitdown-bundle.sh` 现在把基础解释器 `cp -R` 进 `python/`、把包从 venv 抬到 `site-packages/`、删掉 `venv/`（里面是构建机绝对软链、绝对 shebang 的脚本，以及一个我们从不调用的 `magika` console 脚本 —— markitdown 走的是 site-packages 里的 `import magika`，那部分原样保留），wrapper 改为 `PYTHONPATH=$ROOT/site-packages $ROOT/python/bin/python3.12 -m markitdown`，并在产物里搜构建机路径（`dist-info` 的 SBOM 元数据除外）。
  - 顺手修好：`release.yml` 第 60 行 `name:` 里有个裸冒号（`(universal: …)`）让 YAML **整份解析失败**——已改成逗号。
  
  **⑦ P1-8（安全，顺带）**
  `?token=` **只在 WebSocket 握手**被接受（`Upgrade: websocket` + `Connection: upgrade`）；普通 REST 一律 401，服务器端测试相应改成断言「即使 token 正确也 401」。删掉根 `Cargo.toml` 里从未挂载的 `tower-http` `cors` feature。`lib.rs` / `routes/mod.rs` / `docs/dev/REMOTE_ACCESS.md` 的措辞同步。TLS 仍要求反代（保留非 loopback 警告）。

  **⑧ P1-12 余项（死代码）**
  全仓扫描 845 个 `pub fn`，8 个零引用。逐条核**文档里的书面意图**后：**删** `apply_token_at`（被 `apply_token_at_with_key` 取代，GUI 走后者）、`ContentBlock::as_thinking`、`commitments::best_near`（连带 `NearHit` 结构与其 re-export）、`Session::push_assistant`、`access::reload_for_tests`（本身就是空函数，注释自认「OnceLock can't clear」）；**保留** `Persona::is_fallback()`（`20260914-personas.md` 写着「阶段 3 的 API，不删」）与 `embed` feature（`docs/explore/work-sources.md` 写着「v1 不上向量，保持关，以后再加」）。

- **关键路径/文件：**
  - `crates/hermes-memory/{src/store.rs,src/memory.rs,src/scoped.rs}`、`crates/hermes-tools/src/memory.rs`、`crates/hermes-reflect/src/{inbox.rs,micro_apply.rs,lib.rs}`（`deferred.rs` 已删）
  - `crates/hermes-core/src/{compaction.rs,license.rs,message.rs,session.rs}`、`crates/hermes-turn/src/lib.rs`、`crates/hermes-tools/src/session_recall.rs`
  - `crates/hermes-channel/src/{companion_context.rs,access.rs}`、`crates/hermes-gui/src/commands/{chat.rs,inbox.rs,reflect.rs,memory.rs,micro.rs}`、`crates/hermes-server/src/{auth.rs,lib.rs,routes/{mod.rs,chat.rs}}`
  - `scripts/{check-release-tag.sh,write-latest-json.sh,prepare-markitdown-bundle.sh}`、`.github/workflows/release.yml`、`crates/hermes-gui/ui/src/utils/errorText.ts`
  - `docs/{dev/REMOTE_ACCESS.md,spec/projects.md}`、`crates/hermes-gui/resources/README.md`、`.gitignore`（sidecar 的 `venv/` 改成新的 `site-packages/`，否则 304 MB 会被 git 收进来）
- **偏离方案处：** ① 顺手修了 `release.yml` 的 YAML 语法错（否则整条发布链根本不会触发）；② 原计划要把 `write-latest-json.sh` 按「通用包」处理，核实 updater 源码后发现它**没有** `darwin-universal` 键，改为双键指向同一份产物；③ P1-13 未实施，改为**先把文档说真**（`docs/spec/projects.md` 的 §4.3 从「落地」改为「部分」，指明项目专属归档区未做）。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 发版时 tag 写错要当场失败 | `bash scripts/check-release-tag.sh v1.4.0` / `… v1.2.3` | 前者 0、后者 1 并打印两种修法 | 通过 | 实测输出见 §5 |
| 2 | 发布工作流本身是合法 YAML | `python3 -c "import yaml; yaml.safe_load(...)"` | 解析成功 | 通过 | 修好前报 `line 60, column 35` |
| 3 | Intel 用户也能拿到更新 | `write-latest-json.sh v1.4.0 <dir> out.json` | `platforms` 同时含 `darwin-aarch64` 与 `darwin-x86_64`，两者 url/signature 相同 | 通过 | 用假产物跑通 |
| 4 | 换台机器也能转文档 | 把 `markitdown-sidecar/` 拷到 `/tmp/another-mac/Resources/…` 再转一份真实 `.docx` 与 `.csv` | 两个都 rc=0 且有内容 | 通过 | 真实 `会议纪要_XX项目周例会.docx` |
| 5 | 产物里不留构建机路径 | 在 sidecar 里 grep `/Users/`、`/home/`（排除 `dist-info`）+ 找绝对软链 + 找 `pyvenv.cfg` | 三样都空 | 通过 | `cryptography` 的 SBOM JSON 属元数据，已排除 |
| 6 | REST 上带长期 token 必须被拒 | `GET /api/v1/health?token=<正确 token>` | 401（旧行为是 200） | 通过 | `crates/hermes-server/tests/auth.rs` 改为断言两种 token 都 401 |
| 7 | WS 握手仍能带 token | 裸 TCP 发 `GET /api/v1/chat?token=<正确 token>` + Upgrade 头 | 101 | 通过 | 既有测试保留 |
| 8 | 近重复记忆不再落第二条 | `cargo test -p hermes-memory` | `put` 自己就拒绝；带 `supersedes` 时即使正文重复也写 | 通过 | 新增 2 条测试 |
| 9 | 反思队列只剩一条 | `cargo test -p hermes-reflect` | 新测试确认待审落在注入的根里；旧 `deferred` 相关测试与模块已删 | 通过 | |

- **自动化：** 全量门禁（`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`LEBI_DATA_DIR=/tmp/lebi-gate-b3 cargo test --workspace`、`npx tsc --noEmit`、`npm run build`）结果见 §5；`cargo check --workspace --all-targets` 已绿。
- **手工：** GUI 目视 + 「重启 GUI」由用户在下一轮做（本轮结束前已重启一次供查看）。发布链的**端到端**演练必须在 CI 上做（见 §4 遗留）。
- **测试结论：** [x] 全部通过（仓库内可跑的） · [x] 有已知问题（见 §4）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | Intel 能更新；客户机文档导入可用；记忆不再两处落 |
| 开箱即用未破坏 | ☑ | 不新增运行时；Python 在包内 |
| 本地优先未破坏 | ☑ | 数据仍在 `~/.lebi-ai/` 明文；无新增网络出口 |
| 测试通过 | ☑ | 见 §3 / §5 |
| 记录完整 | ☑ | 本文件 + 复审状态行 + `README.md` 索引 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补（默认路径正确） | ☑ | 闸门落在 `put()` / 唯一队列 / 发版脚本 / wrapper |
| 代码卫生（P0 第九条） | ☑ | `deferred.rs` 删除、死 `pub fn` 删除、死 `cors` feature 删除、过时注释与文档同步 |
| 操作与视觉（P0 第十一条） | ☑ | 本轮无新增界面；错误文案改人话（`errorText.ts`） |
| 第一性原理三步写全（P0 第零条） | ☑ | §0-fp |

- **验收人：** 待用户
- **验收日期：** —
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  1. **Apple 代码签名 / 公证未做** —— 需要 Apple Developer 账号与证书；现状是客户首次打开会被 Gatekeeper 拦（右键→打开可绕过）。文档已在 `build-dmg.sh` 头注释写明。
  2. **通用包里的 sidecar 仍是单架构（arm64）** —— 通用 `.dmg` 在 Intel Mac 上 App 能跑，但**文档导入**会失败（包内 Python 是 arm64）。**2026-09-19 已把出路查清**（下面全部是实测，不是推测）：

     | 事实 | 证据 |
     |------|------|
     | 线上 `latest.json` 至今只有 `darwin-aarch64` | `releases/latest/download/latest.json`，Intel 客户压根收不到更新 |
     | `macos-13` runner 已下线；Intel 标签是 `macos-15-intel` / `macos-26-intel`（4C/14GB） | GitHub 托管 runner 文档 |
     | **没有通用 Python** | python-build-standalone 最新 tag 只有 `aarch64-apple-darwin` / `x86_64-apple-darwin`，`universal2` 命中 0 |
     | **x86_64 sidecar 能在一台 arm64 机器上装出来** | 下载 x86_64 的 `install_only` 解释器 + `pip install --platform macosx_11_0_x86_64 --python-version 3.12 --only-binary=:all: markitdown[docx,pdf,xlsx]==0.1.6` → 全部解析出 x86_64 wheel（85 个 `.so` 里 10 个是 universal2，其余 x86_64） |
     | **而且真能跑** | 在装了 Rosetta 的本机直跑：`docx` 1681 字、`pdf` 223 字、`xlsx` 2168 字、`csv` 全部 rc=0，内容正确 |
     | **Rust 侧也能交叉编到 x86_64** | `cargo build -p hermes-gui --target x86_64-apple-darwin` → 退出码 0，产物是真 x86_64 Mach-O（冷启动 debug 52m35s） |
     | 体积 | sidecar 303 MB → 压缩 **98 MB** |

     **结论与推荐：出两个按架构分开的包**（`…_apple-silicon.dmg` / `…_intel.dmg`，各自带自己的 sidecar，`latest.json` 两条各自指自己的 tarball + 签名）。
     理由：① 客户只下 ~125 MB，而「一个通用包塞两套 sidecar」要 ~225 MB —— 国内下 GitHub 这是真实痛点；② CI 分钟数与今天的通用构建**基本持平**（universal 本来就把两个架构都编了一遍），不需要额外的 Intel runner；③ 与 updater 的 `${os}-${arch}` 模型天然对齐。
     代价与对策：客户要在下载页**选对包**（文件名写成 `apple-silicon` / `intel`，安装文档给「苹果菜单 → 关于本机 → 芯片」三步判断，并在 Release 正文里写清）。附加待办：两个 `lebi-AI.app.tar.gz` 同名，上传前必须改名成带架构的名字，否则 Release 资产会互相覆盖。
  3. **P1-13 项目专属归档区未落地** —— 规格 §4.3 要求 `财富早知道/<日期>/` + 项目标签，现在仍是全局 `outputs/<日期>/`；本轮只把文档说真（`docs/spec/projects.md` §11 改为「部分」）。要动「写盘默认路径」，需要先想清楚「归档但不做密室」。
  4. **server 无自带 TLS** —— 仍要求反代；非 loopback 只 warn。要不要收成「非 loopback 必须显式 `--allow-cleartext`」需产品表态（公司服务器场景是真实存在的）。
  5. **`embed` feature 仍无人启用** —— 有书面依据保留（「以后再加」），不是遗物；启用时必须在**过滤后的集合**上做（见 `20260914-personas.md`）。
  6. **sidecar 体积 304 MB（每个架构各一份）** —— `magika` + `onnxruntime` 看着像可以剪，但**剪不得**：markitdown 在 `_markitdown.py:15` 就 `import magika`，magika 又要 onnxruntime。省体积只能靠换实现（见遗留 7），不是删目录。
  7. **文档转换这一层的长期包袱** —— 一个 300 MB 的 Python 运行时、按架构各一份、还要当嵌套二进制做签名，全都来自 markitdown。长期正解是用原生 Rust 做转换（`docx` 读 zip+XML、`xlsx` 用 calamine、`pdf` 用 lopdf/pdf-extract），能把包体、架构问题、Rosetta/签名麻烦一起消掉；属于独立项目，不是这一批。

---

## 5. 附注

- **`check-release-tag.sh` 实测：**
  ```
  $ bash scripts/check-release-tag.sh v1.4.0
  tag v1.4.0 matches tauri.conf.json version 1.4.0
  $ bash scripts/check-release-tag.sh v1.2.3     # 退出码 1
  error: release tag does not match the app version. …
  ```
- **`latest.json` 实测（假产物）：** `darwin-aarch64` / `darwin-x86_64` / `windows-x86_64` 三键，前两者 url 与 signature 完全相同。
- **sidecar 搬迁实测：** 拷贝到 `/tmp/another-mac/Resources/markitdown-sidecar` 后 `markitdown 会议纪要_XX项目周例会.docx` rc=0，正文正确；`.csv` 同样 rc=0。
- **一次自己发现并修掉的事故：** `prepare-markitdown-bundle.sh --force` 会先删再建 sidecar 目录，把 tracked 的 `.keep` 删了；同时包内布局从 `venv/` 改成 `python/` + `site-packages/`，而 `.gitignore` 只忽略了 `venv/` —— 304 MB 产物会变成待提交文件。两处都已在 §2 修正（`.keep` 按 HEAD 原样恢复，`.gitignore` 换成 `site-packages/`），`git status` 对 `resources/` 现在只剩这条 README 改动。
- **一次被自己抓到的验证错误（要记住）：** 我在判断「`venv/bin/magika` 能不能删」时跑过 `rg -n magika markitdown/` 得到 0 命中，据此在台账里写了「markitdown 从不 import magika」。**这个结论是错的**，因为 `rg` 默认遵守 `.gitignore`，而 sidecar 整棵树正是被忽略的 —— 它一声不响地把目录跳过了。加 `--no-ignore` 重跑：`_markitdown.py:15 import magika`、`:121 magika.Magika()`、`:724 identify_stream()`。结论修正：删 `venv/bin` 仍然对（我们不调用 CLI），但**理由**不是「markitdown 用不到 magika」，而是「它用的是 site-packages 里的 Python 包」。教训：在被忽略的目录里做搜索，必须 `--no-ignore`。
- **`test/_trash-20260919/`**：`test/workspace/` 根旧产物全部移到这里，**未硬删**，确认无用后可整目录删除。
- **全量门禁（2026-09-19 实跑，全绿）：**
  ```
  cargo fmt --all -- --check                              → FMT OK
  cargo clippy --workspace --all-targets -- -D warnings   → Finished (0 warnings)
  LEBI_DATA_DIR=/tmp/lebi-gate-b3 cargo test --workspace   → TEST_EXIT=0，681 passed / 0 failed
  cd crates/hermes-gui/ui && npx tsc --noEmit              → TSC OK
  cd crates/hermes-gui/ui && npm run build                 → ✓ built（dist 已刷新）
  ```
  （`/tmp/lebi-gate-b3` 是隔离数据根，真实数据根 `test/` 未被测试写入。）
- **GUI 已重启**（`LEBI_DATA_DIR=…/test scripts/run-gui.sh`，pid 见当次汇报），供用户目视。
