# lebi-AI（乐彼AI）· 全量代码学习报告（快照）

| 字段 | 内容 |
|------|------|
| **日期** | 2026-09-15 |
| **种类** | **H** 快照（非权威 · 会过期） |
| **权威** | 冲突只认 [`../../PRODUCT_PRINCIPLES.md`](../../PRODUCT_PRINCIPLES.md) **P0 v0.11**（P1 v0.6 / P2 `AGENTS.md` / P3 `README.md`）。本文不得当现行法。 |
| **方法** | 精读权威文档 + 6 个并行只读子代理逐 crate 深读 + 本人抽查关键断言 + `cargo check/test` 实测 |
| **基线** | 当前**工作树**（含未提交 WIP）。最后提交 `91cdcf7` v1.3.0（2026-08-18） |
| **台账** | [`../records/20260915-codebase-learning-refresh.md`](../records/20260915-codebase-learning-refresh.md) |
| **上一版** | `20260807` 版（已大面积过期）；本版重写，行号均针对 2026-09-15 工作树 |

> **先读这条：** 工作树里有 **~101 个未提交改动**（5387 增 / 1208 删），含整套未接线的新模块
> （personas、topics、scoped memory、outputs）。**只看 `git HEAD` 会得到错误的项目认知。**
>
> **并发提醒（2026-09-15 成稿时实测）：** 本报告写作期间，**另一会话正在同一工作树继续改 personas**
> （`crates/hermes-core/src/personas/` 从 4 个定义增至 10 个：新增 王海燕 / 小谢 / 小金 / 雨天 / 小雨 / 小文，
> 并改动 `companion.rs`、`hermes-channel/{context,companion_context}.rs`、`hermes-reflect/{episode,inbox}.rs`，
> 新台账 `docs/records/20260915-persona-roster.md`）。**行号与「未接线」判断会随该会话推进而变化，用前先复读文件。**

---

## 1. 实测状态（2026-09-15）

| 项 | 实测结果 |
|----|----------|
| `cargo check --workspace --all-targets` | **通过**（44.7s，无警告） |
| `cargo test --workspace` | **全绿**：39 个测试二进制 `test result: ok`，0 失败 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **通过**（1m11s，零警告） |
| `cargo fmt --all -- --check` | **通过**（无 diff） |
| 结论 | 当前 WIP 工作树**满足 P1 §六·附 B 的 lint/test 硬门槛** |
| 规模 | Rust 54,779 行 / 18 crate；GUI 前端 15,326 行 / 74 文件；Flutter 4,283 行 |
| 根目录 md | 正好 4 个权威文件 ✓ |

---

## 2. 产品与架构共识（30 秒版）

- **产品**：本地**工作搭子** AI。四环 Do × Continuity × Care × Evolve。不靠类比开工（P0 第零条）。
- **身份协议唯一源码**：`crates/hermes-core/src/companion.rs`。只读面用 `companion_protocol_readonly()`（`:88`），
  可落盘面用 `companion_protocol()`（`:193` = 只读段 + `PROTOCOL_DURABLE_WRITES` `:172`）；
  调用点 `hermes-channel/src/system_prompt.rs:62,92` 与 `hermes-channel/src/companion_context.rs:51`。
- **记忆分区词表唯一源码**：`companion.rs:16-23`（`preferences` / `standards` / `work` / `general`）。
- **单一引擎、多入口**：CLI、GUI、Flutter(server)、IM 共用 `hermes-core` 与 `~/.lebi-ai/`；surface 不 fork 引擎。
- **数据**：全明文。会话 JSONL（append + fsync，`hermes-store/src/session.rs:81-93`）、
  记忆/技能 Markdown + frontmatter；无数据库。含密钥文件 0600（`license.rs:248-250`、`hermes-llm/src/config.rs:495`）。

### 数据根布局（`crates/hermes-core/src/paths.rs`）

```
LEBI_DATA_DIR:14  →  HERMES_DATA_DIR(兼容):18  →  指针文件 data-dir.txt:38  →  ~/.lebi-ai:22
  sessions/                 JSONL 会话（含 sessions/{wechat,feishu,telegram}/<user>/）
  memories/  profile.md     memory-stats.jsonl
  skills/                   user scope（项目 scope = ./.lebi-ai/skills）
  commitments.json          「在办」
  reviews/                  「回顾」{stamp_from_to.md, index.json, prefs.json}
  topics.json / topics-<owner>.json、pending-review.json、reflect-log.jsonl、deferred.jsonl
  sources/                  「我的材料」catalog.json + <id>/{original.<ext>, body.md}
  workspace/outputs/<YYYY-MM-DD>/   产出物
  config.toml  license.json  server.token  mcp.json  channel-allowlist.toml
```

迁移 `maybe_migrate_data_root()`（`paths.rs:165`）只被 `hermes-gui/src/main.rs:14` 与
`hermes-cli/src/main.rs:283` 调用——**server 不迁移**（见 §5.C5）。人物定义**不在数据根**，编译进二进制（`hermes-core/src/persona.rs:55-63`）。

---

## 3. 分层速记

| 层 | crate | 关键点 |
|----|-------|--------|
| 核心抽象 | `hermes-core` | Session / LlmProvider / ToolHost / 压缩 / 授权 / persona。零 UI 与传输依赖 |
| 引擎能力 | `hermes-llm`、`hermes-turn`、`hermes-mcp` | provider（Anthropic 三段 cache 断点 `anthropic.rs:235`；OpenAI 兼容 `openai.rs:82`）、回合循环与权限、MCP（`server__tool`，`host.rs:84`，强制 confirm `:93`） |
| 工具 | `hermes-tools` | 注册表 `lib.rs:54`（基础 8）+ 动态追加；白名单 `handles()` `lib.rs:169`；测试强制「宣布的必须可调用」`lib.rs:385` |
| 记忆进化 | `hermes-store`、`hermes-skills`、`hermes-memory`、`hermes-reflect` | 明文存储、技能域、记忆宫殿 + 主题卡、reflection 管线 |
| 在办/回顾 | `hermes-commitments` | `commitments.json`（**无期限不成债** `store.rs:288-290`）+ `reviews/` |
| 共享面 | `hermes-channel` | `Channel` trait `channel.rs:61`、`ServeCtx`:78、`serve_inbound`:221、`IM_TOOL_WHITELIST`:38、allowlist `access.rs` |
| 桌面（默认路径） | `hermes-gui` | Tauri 2；`frontendDist=./ui/dist`（**无 devUrl**，`tauri.conf.json:8`）；直连引擎不经 server |
| 后端 | `hermes-server` | axum + bearer token；GUI 子集 + WS（`routes/mod.rs:29-104`）；ticket 60s `tickets.rs:15` |
| IM | `hermes-weixin/feishu/telegram` | 仅协议差异（长轮询 / WS / 长轮询） |
| 手机 | `clients/flutter` | 4 页；token 存 Keychain/Keystore（`connection_providers.dart:37-57`） |
| 材料 | `hermes-sources` | 仅 GUI 写入；倒排全内存；CJK 二元分词 `tokenize.rs:6-25` |

---

## 4. 各子系统要点（新人视角）

### 4.1 引擎核心
- **权限三层**：配置 deny→allow（glob）→ 产品默认（`hermes-turn/src/permissions.rs:40`）→ 未知 fail-safe。
  `is_absolute_risk`（`turn/danger.rs:25`）让 `skill_install`、高危 bash、任何 MCP 工具**无法被 allow 跳过**；
  高危 bash 清单在 `danger.rs:153`（rm -rf / sudo / mkfs / curl|sh / fork bomb …）。
- **并行/串行**：安全调用并发、需确认的串行（`hermes-turn/src/lib.rs:473` vs `:520`）；无确认通道 fail-closed。
- **ToolHost 四种实现**：`BuiltinToolHost`（`hermes-tools/src/lib.rs:193`）、`CompositeToolHost`（`:349`）、
  `PersonaToolHost`（`persona_scope.rs:49`）、`McpToolHost`。`ToolSpec` 只有 `requires_confirmation`，没有 read_only/parallel 字段（`core/provider.rs:32`）。
- **压缩**：CJK 感知估算（`core/compaction.rs:20`），触发 `should_compact`:73，压缩点 `turn/agent.rs:224`。
- **LLM**：Anthropic 缓存断点、`deepseek.com` 自动关缓存；重试 429/5xx ×3（`retry.rs:11`）；流式半个汉字用 `Utf8Carry`（`utf8.rs:8`）。

### 4.2 工具与技能
- 需确认的工具只有 7 个：`memory_delete`、`skill_create/install/delete`、`propose_skill`、`subagent`、`commitment_drop`。
  **bash 默认放行**，靠形态判定。
- 写盘位置：`write` 默认 `workspace/outputs/<日>/`（`write.rs:36`）；`todo_write` 覆盖 `workspace/TODOS.md`；
  skill 落 `~/.lebi-ai/skills/`；`web_fetch/web_search` 只写进程内 256 条 TTL 缓存，不落盘。
- 防秘密外泄：唯一名单 `safety.rs:362`（产品密钥）× 数据根 × OS 凭据（`.ssh/.gnupg/.aws/.kube/.netrc`），
  前置于 read/write/open（`:14,219,245`），并喂给 seatbelt（`bash_sandbox.rs:130`）；
  bash 字符串闸 `bash_secret_read_blocked:287` 可被相对路径绕过，只剩 seatbelt 兜底且仅 macOS。
- 技能：frontmatter `name/description/triggers/version/license/always_active/extra`（`skill.rs:19-45`）；
  索引只含 name+description 且限量（`hermes-channel/src/context.rs:122-137`）= Progressive Disclosure 已落地；
  远程安装强制 `always_active=false`（`install.rs:258,356`）；索引保鲜靠 `list_or_cached` + 每轮 `refresh_skills`（`gui/state.rs:362`）。

### 4.3 记忆与进化（产品的「第二次更准」）
- **四层对象不要混**：当次拆步 `todo`（进程内 + `TODOS.md`）／跨次**在办** `commitments.json`／
  **回顾** `reviews/*.md`／长期**手感规则** `memories/`。
- **slot vs topic**：slot = 「怎么干活」（7 类，`slot.rs:12-43`），topic = 「讲什么」（`topics.rs`）；**两个轴**，
  历史 bug 正是拿 slot 当 topic（见 `20260914-memory-topic-cards.md`）。
- **归属**：唯一判定 `resolve_owner`（`hermes-memory/src/scoped.rs:41`）；偏好/标准 → 全局；`builtin` 角色 → 全局（`persona.rs:40`）。
- **候选必确认**：默认 `auto_accept_memories=false`（`hermes-llm/src/config.rs:198`，测试断言 `:684`）；
  入队 `inbox.rs:95`（指纹去重 + 上限 100），批准 `accept_memory_item:378`，拒绝先记 `reflect-log.jsonl` 再删。
- 去噪 `is_internal_noise_text`（`episode.rs:37`）；micro 触发 = 意图词表或每 3 轮（`micro.rs:15-58`）。
- distill：TF-IDF 余弦 + union-find，阈值 0.55（`memory/distill.rs:33,76`），pinned/preferences 受保护（`:155`）。

### 4.4 桌面 GUI（用户默认路径）
- 面板只有 3 个：`chat / know / settings`（`App.tsx:221-230`、`Sidebar.tsx:32-36`）；
  「在办 / 回顾」是抽屉（`ChatView.tsx:259,421`）；「它记得的 / 做法 / 材料」是 Know 内 tab（`KnowPanel.tsx:42-46`）。
- 命令 76 条（`main.rs:30-107`）；`tests/command_registration.rs:172` 断言「前端每个 invoke 都已注册」（本次反向也扫过，无孤儿）。
- 事件：流式 `Channel<ChatStreamEvent>`（`events.rs:5` → `chatStore.ts:162`）；在办变更 `hermes://zaiban-changed`；收件箱 `hermes:inbox-changed`。
- 最小窗口能力：UI 用 `.destroy(` ⇒ 必须授 `core:window:allow-destroy`，由 `tests/window_capabilities.rs:16` 守（`:131` 证明 `core:window:default` 不含它）。
- 合规亮点（P0 禁止项有护栏而非违反）：`companion.rs:217 speech_honesty_clause`（不准假装导出/虚构按钮）；
  `remembered.ts:27` 只在 `memory_save` **真成功**后庆祝；会话结束默认只入收件箱、不弹窗。

### 4.5 CLI / server / IM
- CLI 全量子命令在 `hermes-cli/src/main.rs`（`init:27`、`ask:33`、`run:45`、`chat:53`（支持 `--persona`）、
  `personas:70`、`topics:117`、`distill:98`、`serve:142`、`wechat/feishu/telegram` …）。CLI = 引擎装配/调试入口。
- server token：`--token`→`--token-file`→`HERMES_SERVER_TOKEN`→`~/.lebi-ai/server.token`（自动生成 32B、0600，`auth.rs:39-106`），日志只打指纹。
- IM 三道闸：allowlist（`channel.rs:233`，空 = 全拒）→ 工具白名单（`:38`）→ 无确认 fail-closed。
  会话落 `sessions/{channel}/{user}/…`，与桌面同一 JSONL 格式。

### 4.6 周边
- 材料：拖入 → `import_document`（`gui/commands/upload.rs:123`）→ 转 md → `ingest_auto_keep` → 下一轮检索注入 `[Materials]`；
  只认 pdf/doc/docx，上限 200 份 / 2M 字符；markitdown sidecar 由 `scripts/prepare-markitdown-bundle.sh` 打进包。
- 打开 GUI 唯一正确姿势：`scripts/run-gui.sh`（先 build `ui/dist`）。`build.rs:26-49` 在 dist 缺失时自动 npm build。
- 打包：`build-dmg.sh`（app,dmg + `.sig`）、`build-exe.ps1`（NSIS）；更新 `tauri.conf.json:44-52` + `write-latest-json.sh`。

---

## 5. 已核实的问题清单（按严重度）

> 标注：**[亲验]** = 本次本人直接核对；**[子代理]** = 子代理报告并附证据位置，未二次复验。

### A · 安全 / 授权（最高）

1. **[亲验] 签名私钥（seed）在仓库里，且与出厂验签公钥匹配。**
   `crates/hermes-core/src/license.rs:543-547` 的 `DEV_SEED`，被测试 `public_key_matches_seed`（`:619-623`）断言等于
   `PUBLIC_KEY_BYTES`（`:34-37` = 现网 dmg/exe 的验签公钥）；签名函数 `sign_token_with_seed` 是 **pub**（`:151`）。
   文件头 `:6` 写着「Private key never ships in the client」，`docs/spec/license-ux.md:260` 也写「发版前务必轮换」。
   → 拿到本仓库（`origin git@github.com:lebiai/lebiai.git`）的人可**自签永久授权码**，签发体系对已发布版本失效。
   **需产品/商业决策（轮换密钥 + seed 移出源码），不是随手能改的补丁。**
   > **已修复（2026-09-18）：** 密钥对已轮换，私钥移出仓库、只留签发机；全仓扫描守卫测试防复发。
   > 见 [`../records/20260918-license-hardening.md`](../records/20260918-license-hardening.md)。
   > 本条只描述 2026-09-15 那天的树。
2. **[亲验] 授权只卡两个点**：`hermes-gui/src/commands/chat.rs:92`、`review.rs:103`（`can_use_main`）。
   过期后 CLI / Flutter(server) / IM 全功能，GUI 的反思、记忆、微信连接也未设门。
   「授权只锁桌面」是已拍板（`project-map.md` §6），但**桌面内部**哪些该锁没写清。
   > **已修复（2026-09-18）：** 门禁收进 provider 装配层（`hermes-core::LicenseGatedProvider`
   > 由 `hermes-llm::Config::build_active_provider` 套上），四个入口一起覆盖。见同上台账。
3. **[子代理] server 无 TLS，且仍接受 legacy `?token=`**（`hermes-server/src/auth.rs:129-133`）；
   `data-dir/reset`、`config PUT` 等破坏性写面同一 token 即可（`routes/config.rs:333,144`）。默认 loopback + 仅 warn（`lib.rs:46-51`）。
   > **已收窄（2026-09-19）：** `?token=` 现在**只在 WebSocket 握手**被接受（浏览器没法给 WS 设 header）；
   > 普通 REST 带 `?token=` 一律 401。**TLS 仍未内置**，公网必须反代。见
   > [`../records/20260919-batch3-structure.md`](../records/20260919-batch3-structure.md)。

### B · 未接线 / 文档不实（诚实性）

1. **[亲验] personas 只到引擎，默认路径拿不到。**
   `hermes-gui/src/commands/chat.rs:307` 与 `hermes-server/src/routes/chat.rs:240` 硬编码 `persona: None`；
   `SessionMeta.persona` 只有 CLI 写；`ui/src` 里没有任何工位/人物界面。
   即 `docs/spec/personas.md` 的**阶段 1（引擎）已落地、阶段 3（GUI 形状）未做**。
2. **[亲验] 台账状态落后于代码**：`docs/records/20260914-personas.md:7` 仍写「计划已出 · 待开工」，
   而工作树已有 `persona.rs`、`personas/*.md`、`scoped.rs`、`topics.rs`、`persona_scope.rs`、`gui/commands/outputs.rs`。
3. **[亲验] `docs/snapshot/project-map.md:76` 的「chat 白名单 / run 全量」与代码不符**：两者都是全量工具 + 交互确认，
   差异只在 chat 额外套 `PersonaToolHost`（`hermes-cli/src/commands/chat/mod.rs:139-144`）。该图 2026-08-17 后未更新。
4. **[亲验] 15 份 docs 仍写 P0 v0.9**：`docs/dev/{REMOTE_ACCESS,docker,gui-run,license-test,mobile-extras,web-search}.md`、
   `docs/guide/{api-key-guide,channel-allowlist}.md`、`docs/spec/{license-ux,settings-ia,gui-ritual-motion,work-companion-solution}.md` 等。
5. **[亲验] 台账索引有洞**：`docs/records/README.md` 没有 `docs/records/20260807-remove-tb-legacy.md` 的索引行
   （只在 `:143` 被顺带提及）。注：该文件 `:27` 的 `20260803-fix-server-auth.md` 是**命名示例**，不是坏链。

### C · 引擎侧不一致 / 平行实现

1. **[子代理] 两套 prompt 装配并存**：`hermes-channel/src/context.rs`（CLI/IM，`compose_system_prompt`）
   vs `hermes-channel/src/companion_context.rs`（GUI/server，`build_turn_system`）；人物块只在后者。
   `ServeCtx`（`channel.rs:78-97`）没有在办/材料字段 → **IM 永远看不到在办与材料**。两处各有顺序断言测试，短期安全，长期双源。
2. **[子代理] `url_safety` 两份且已分叉**：`hermes-tools/src/url_safety.rs` 放行 `198.18/15`（Clash/Surge fake-ip），
   `hermes-skills/src/url_safety.rs` 不放行。
3. **[子代理] 双待审队列**：`deferred.jsonl`（CLI）与 `pending-review.json`（GUI/server）并行，靠 `MicroApplyConfig::queue_deferred` 切换（`micro_apply.rs:30-31,53`）。
   > **已修复（2026-09-19）：** 只剩一条队列 `pending-review.json`（原子写）；`deferred.rs` 已删，
   > CLI 会把旧 `deferred.jsonl` 并进来后删掉。见 [`../records/20260919-batch3-structure.md`](../records/20260919-batch3-structure.md)。
4. **[子代理] embedding 是死代码**：`hermes-memory` 的 `embed` feature 无任何 crate 启用（`Cargo.toml:23-26`），`embed.rs` 不可达（`scoped.rs:226-230` 自认）。
   > **复核（2026-09-19）：** 说法成立，但**不是遗物** —— `docs/explore/work-sources.md` 写着
   > 「v1 不上向量，保持关，以后再加」。**有意保留**，见 [`../records/20260919-batch3-structure.md`](../records/20260919-batch3-structure.md)。
5. **[子代理] 数据根迁移只在 GUI/CLI 调用** → 老 `.small-rust-hermes` 用户走 Flutter 后端会看到空的 `~/.lebi-ai`。
6. **[子代理] 启动即改用户文件**：`hermes-core/src/workspace_hygiene.rs:12-27,45-85` 按文件名子串（含「判决」「民初」「法条」）
   把文件搬进 `_quarantine_lawyer/`，不可逆；GUI `state.rs:225` / server `state.rs:97` 启动执行。值得产品复核。
7. **[子代理] `standards` 不受 distill 保护**（`distill.rs:155` 只看 `pinned || is_preferences`）。

### D · 入口差异（与能力矩阵相关）

1. **[子代理] server 会写记忆**：轮末跑 micro-reflection，用户把 `reflect.auto_accept_memories` 置 true 时自动落盘（`routes/chat.rs:541-558`）。
   默认 false 合规（[亲验] `hermes-llm/src/config.rs:198`），但「server 不写」的直觉不成立。
2. **[子代理] server 认不出渠道会话**：`routes/sessions.rs:127-153` 把 `wechat/feishu/telegram` 会话当普通会话列出
   （GUI 有 `channel` / `read_only` 标记，`gui/commands/session.rs:124-160`）→ 手机端可改/删 IM 记录且不分组。
3. **[子代理] IM 只读只有一道防线**：构建期过滤工具清单，`ToolHost` 仍挂着 `memory_save`/`skill_create`（`channel.rs:36-37` 自认）。
4. **[子代理] IM 协议债**：飞书无 `message_id` 去重（`feishu.rs:181,218`）；TG 用 `chat_id` 做 key → 群聊共享一条历史、无 @ 判定（`telegram.rs:171-182`）；
   微信无去重、无群/会话 id（`weixin/types.rs:84-104`）。
5. **[子代理] Flutter 半成品**：`saveSkill:166`、`createMemory:202`、`countInbox:111` 零调用；
   默认 `http://localhost:8765`，Android 无 `usesCleartextTraffic`、iOS 无 ATS 例外 → 真机大概率连不上（**未实机验证**）。

### E · 卫生 / 冗余（P0 第九条）

1. **[子代理] i18n 约 80 个死键**（`ui/src/i18n.ts`）：`nav.skills`、`propose.*`(16)、`reflect.*`(22)、`inbox.*`(14)、`settings.tabAccount/…`。
2. **[子代理] `#[allow(dead_code)]` 罩住整个 `AppState`**（`gui/src/state.rs:55`）与 `ActiveSession`（`:178`）、`error.rs:4`。
3. **[子代理] micro 反思不刷新收件箱**：`chatStore.ts:771-806` 不派发 `hermes:inbox-changed` → 角标滞后到下次切面板。
4. **[子代理] 单文件 temp 名不唯一**（`hermes-memory/src/topics.rs:123`）→ 同 owner 并发写可能互覆。

### F · 交付链

1. **[子代理] 更新只覆盖 Apple Silicon**：`write-latest-json.sh:64-73` 只写 `darwin-aarch64` / `windows-x86_64`，
   而 `build-dmg.sh:62-68` 支持 universal → Intel Mac 点「更新」拿不到包。
2. **[子代理] `release.yml:135-139` 先删后建 Release**，中途失败留下空 Release（历史已发生一次）。
3. **[子代理] CI 不覆盖真实打包**：`ci.yml:34-44` 只有 fmt/check/clippy/test，无 `tauri build`、无 flutter analyze/test。

---

## 6. 能力矩阵（2026-09-15）

| 能力 | CLI | GUI（默认） | Server/Flutter | IM |
|------|-----|-------------|----------------|----|
| 对话 + 流式 | ✅ | ✅ | ✅ WS + ticket | ✅ |
| 工具面 | 全量（chat 另套 PersonaToolHost） | 全量 | 全量子集（无 commitment/source/subagent） | 只读白名单 10 个 |
| 工具确认 | ✅ | ✅ | ✅ | ❌ fail-closed |
| 记忆 / 技能 | ✅ | ✅ | REST + Flutter 面板 | ❌ |
| 反思 / 收件箱 | ✅ | ✅ | ✅ | ❌ |
| 在办 / 回顾 | 工具层有 | ✅ 抽屉 | ❌（`open_work: &[]`） | ❌ |
| **人物 / 工位** | ✅（`--persona`、`hermes personas`） | **❌ 未接线** | ❌ | ❌ |
| 我的材料 | ❌ | ✅ 上传 + 检索 | ❌（无 `/sources`） | ❌ |
| 授权门禁 | ❌ | ✅ 只卡对话/回顾 | ❌ | ❌ |

---

## 7. 新人必知（改代码前）

1. **先读 P0 v0.11 → P1 v0.6 → AGENTS**；有用户/架构影响必须写 `docs/records/YYYYMMDD-slug.md`。
2. **打开 GUI 只用 `scripts/run-gui.sh`**；改 `ui/src` 后必须 rebuild，`tauri.conf.json` 不得写回 `devUrl`/5173。
3. **默认路径差异**：引擎改了不等于用户看得见（personas 就是活教材）；桌面才是默认交付。
4. 提示词只改 `companion.rs`；别在各 surface 抄第二份身份协议。
5. 记忆归属只走 `resolve_owner`；zone 名不得自造（`companion::zones`）。
6. 加工具必须同时改「声明」与 `handles()`（`hermes-tools/src/lib.rs:385` 的测试会红）。
7. 加 `invoke` 命令必须同时改 `commands/mod.rs` 与 `main.rs` 注册表（`tests/command_registration.rs` 守）。
8. 新列表页用 `Pager`（`Pager.tsx:11`，PAGE_SIZE=10）。
9. 碰文件的测试要用 `with_data_dir` + 全局锁，否则会写真实 `~/.lebi-ai`。
10. 工作树 = WIP；切分支/清理前先确认那些 untracked 模块（personas/topics/outputs）不是你要的。

---

## 8. 本文可信度

- **亲验**：§1 全部（`cargo check`/`cargo test` 实跑）、§2 数据根与协议位置、§4.4 命令注册与能力、§5.A1/A2、§5.B1–B5。
- **子代理报告未二次复验**：§5 中标 [子代理] 的条目（已附 文件:行号，请按行号复核后再据此决策）。
- 行号针对 **2026-09-15 工作树**；`git HEAD`（`91cdcf7`）版本缺 personas/topics/outputs 整层，行号会偏移。
- 工作树在成稿期间**仍在被另一会话改动**（见文首并发提醒），personas 相关行号请在动手前复读。
- 本快照为只读产出，**未修改任何产品代码**；问题清单不等于已处理。
