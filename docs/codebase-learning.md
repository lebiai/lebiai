# lebi-AI（乐彼AI）· 全量代码学习报告

| 字段 | 内容 |
|------|------|
| **日期** | 2026-08-07 |
| **性质** | 非权威文档；全量学习产出，供后续开发快速接入 |
| **权威** | 冲突以根目录 `PRODUCT_PRINCIPLES.md`（P0）/ `DEVELOPMENT_RULES.md`（P1）/ `AGENTS.md`（P2）/ `README.md`（P3）为准 |
| **方法** | 精读权威文档 + 6 个并行只读子代理逐 crate 深读 + 抽查核对 |
| **台账** | [`records/20260807-codebase-full-learning.md`](./records/20260807-codebase-full-learning.md) |

---

## 1. 产品与架构共识（30 秒版）

- **产品**：乐彼AI（lebi-AI）= 本地工作搭子 AI。Slogan：**越用越像你的手感**。四环：Do × Continuity × Care × Evolve。
- **身份协议唯一源码**：`crates/hermes-core/src/companion.rs`（`companion_protocol()` 在 :28，channel/GUI/server 全部复用）。
- **引擎**：Rust workspace，16 个 crate；单一引擎、多入口（CLI / GUI / Flutter+server / 微信 / 飞书 / Telegram），共享同一份 `~/.lebi-ai/` 明文数据。
- **数据**：会话 JSONL、记忆/技能 Markdown + YAML frontmatter、无数据库；`LEBI_DATA_DIR` 可覆盖；含密钥文件 0600。
- **进化**：full reflection（会话结束）+ micro-reflection（轮间异步、不阻塞）+ distill（聚类收敛）；候选须用户确认才落盘（默认 `auto_accept_memories=false`）。

## 2. 架构分层速记

| 层 | 路径 | 职责 |
|----|------|------|
| 核心抽象 | `crates/hermes-core` | Session / LlmProvider / ToolHost / 压缩 / companion 协议；零 UI/传输依赖 |
| 引擎能力 | `hermes-llm`、`hermes-turn`、`hermes-tools`、`hermes-mcp` | provider、回合引擎与权限、内置工具、MCP 客户端 |
| 记忆进化 | `hermes-store`、`hermes-skills`、`hermes-memory`、`hermes-reflect` | 明文存储、技能域、记忆宫殿、反射管线 |
| 共享层 | `hermes-channel` | **IM 与上下文唯一共享驱动**：`Channel` trait + `ServeCtx` + `serve_inbound` + `compose_system_prompt` + `ContextSources` |
| 桌面 | `hermes-gui`（Tauri 2，`ui/dist`，**无 devUrl**） | 主交付面；与 CLI 同引擎同数据，直连引擎不经过 server |
| 后端 | `hermes-server`（axum，bearer token） | Flutter 后端；自称「GUI commands 1:1」，实为子集 + WS 变体 |
| IM | `hermes-weixin/feishu/telegram` | 仅协议差异（扫码长轮询 / WS 长连接 / 长轮询） |
| 移动端 | `clients/flutter` | iOS/Android/macOS；调 server，token 存 Keychain/Keystore |
| 脚本 | `scripts/run-gui.sh`（默认开 GUI 路径）等 | 构建 / 打包 / markitdown sidecar |

## 3. 用户主路径（实现确认）

```
hermes init（引导 config.toml，0600）
  → 对话：chat / ask / run（GUI / 手机 / IM 同引擎）
  → 会话结束 full reflection（min_turns=3 门槛；/reflect 强制）
  → 候选确认/拒绝（CLI 逐条交互 / GUI inbox / server 路由）
  → 明文落盘 skills/ + memories/（supersedes 链收敛）
  → hermes distill --apply（可选，跨会话去重融合）
```

## 4. 各子系统精读摘要

### 4.1 hermes-core / hermes-llm / hermes-turn / hermes-mcp（引擎核心）

- **一条 `ToolHost` 抽象贯穿全链**：引擎只见 `ToolHost`；内置（`CompositeToolHost`）与 MCP（`McpToolHost`，`<server>__<tool>` 命名空间）同构；MCP 工具默认 `requires_confirmation: true`（外部副作用谨慎），内置工具默认 false。
- **三层权限 confirm**：配置 deny→allow（glob，支持 `bash:git *`、`mcp` 前缀）→ 产品默认（bash 开放 + 高危启发式：rm -rf / sudo / curl|sh / mkfs / fork bomb / shutdown 等）→ 未知工具 fail-safe 确认；安全调用并行、确认调用串行；`AlwaysAllow` 由前端记忆。
- **Anthropic 缓存**：JSON 层注入 3 个 `cache_control: ephemeral` 断点（最后 tool / system 数组化 / 最后消息末 block），核心类型零污染；system prompt 强制缓存稳定（时间戳注入最后一条 user 消息）；`deepseek.com` 自动关缓存。
- **Session 是 JSONL 事件流**：`SessionEvent`（meta/message/usage）逐行 append，崩溃可恢复；压缩用 CJK 感知 token 估算（CJK 1 字=1 token）+ LLM 摘要替换前缀，保留最近 N 对原文；thinking 默认不落盘。
- **回合引擎**：`run_turn`（turn/lib.rs:193）流式 → 工具分类 Phase 1 → 并行执行安全调用 → 串行确认调用（oneshot `ConfirmRequest`）→ 结果回喂 → `[lebi-AI Care]` nudge 会被 reflect 的 `is_internal_noise_text` 过滤，不污染记忆。
- **MCP**：stdio + Streamable HTTP 双传输；server 名禁含 `__`；连接失败 warn 跳过不拖垮会话。
- **数据隔离**：`~/.lebi-ai`（env 优先），首次运行把 `~/.small-rust-hermes` 纯 rename 迁移，绝不碰 `.lebi-law`；GUI/server 启动隔离律师版残留文件（quarantine）。

### 4.2 hermes-tools / hermes-skills（工具与技能）

- **内置工具**（`handles()` 白名单 + 条件装配，store/ctx 未注入则该类工具不暴露）：
  - 文件：`read/write/edit/glob/grep/bash/git`（git 只读白名单；bash 默认 120s，高危在 turn 层拦截）
  - 思考/计划：`think/todo_write/todo_list`（镜像 `TODOS.md`，per-session）
  - Web：`web_fetch/web_search`（三后端 Scraper/Tavily/BraveApi，进程级 TTL 缓存）
  - 记忆：`memory_search/memory_save/memory_delete/memory_distill/palace_zones/palace_read_zone/palace_recall`
  - 技能：`skill_list/skill_read/skill_read_file/skill_create/skill_install/skill_delete/propose_skill`
  - 子代理：`subagent`（`max_depth=1` 原子守卫 + 子 host 无 propose/subagent ctx → **结构性禁止递归**；CLI/agent/IM 已接，GUI 未接）
- **安全默认很硬**：文件/技能路径 canonicalize 前缀校验、技能名白名单正则、远程安装配额（50 文件 / 100KB / 5MB / 深度 6）+ 事务 staging→rename + **强制 `always_active=false`**。
- **技能生命周期三段式**：发现（索引只含 name+description，受 cap 截断）→ 激活（LLM 自觉调 `skill_read`）→ 执行（`skill_read_file` 按需拉附属文件）。`always_active` 唯一例外是内置 `memory-palace`。
- **匹配器不是 BM25**：v1 是 token 重叠 + effectiveness 因子，v2 hybrid 可选 embedding（0.4 token + 0.6 embed）；匹配结果只进 stats，**不再注入 prompt 正文**（激活 100% 靠 `skill_read`）。
- **内置技能嵌入**：memory-palace 为硬编码 Rust 字符串（`chat/mod.rs:614`，`always_active: true`）；skill-creator / find-skills 为 `include_str!` 多文件包；删除保护 `BUNDLED_SKILLS` 三件套在 `hermes-skills/src/install.rs`。

### 4.3 hermes-store / hermes-memory / hermes-reflect（存储与进化）

- **会话 JSONL**：第一行必须 Meta；`create_new` 防覆盖；append 后 sync_data；损坏行跳过。
- **记忆**：`<root>/memories/YYYY-MM-DD-mem_<12hex>.md`，frontmatter 含 id/created/source/confidence/pinned/tags/zone/supersedes/extra；`list_active` 一次哈希过滤 superseded id（旧文件留盘做审计）；effectiveness 记 `memory-stats.jsonl`。
- **进化管线**：
  - full reflection：`reflect()`（runner.rs:32，8192 tokens，t0.2）→ JSON 解析 + 截断修复 → `finalize_reflection_output`（丢空洞 episode，seed 自包含 episode）→ 逐候选确认 → `persist_skill/persist_memory` → 写 `reflect-log.jsonl`。
  - micro-reflection：explicit intent 关键词绕过 3 轮冷却（但受 `auto_accept_memories` 总开关约束）；`tokio::spawn` 后台不阻塞；技能候选**永不自动写**；写前 `check_near_duplicate` 0.55 阈值。
  - distill：TF-IDF + union-find 聚类（模型无关），survivor 按 effectiveness/长度选；core/pinned 保护只报告；`--llm-merge` 才用 LLM 合成正文。
- **三端共享引擎，差异只在呈现层**：CLI 逐条交互、GUI 静默入 inbox（`pending-review.json`，MAX_ITEMS=100）、server 返回候选视图。

### 4.4 hermes-cli（主入口）

- **子命令**：`init / doctor / ask / run / chat / mcp / skills / memory / session / reflect-stats / distill / wechat / feishu / telegram / serve`（`main.rs:24-356`）。
- **chat 装配**：`compose_system_prompt`（channel，含 companion 协议，缓存稳定）→ `ContextSources::build_session_system`（palace 索引 > 编译 profile > pinned+relevant）→ 每轮 `build_turn_system`（无 palace/profile 时才注入相关记忆 + Care/Pushback nudge）→ `run_one_turn`（Ctrl-C 取消 + stdin 三态确认 `y/a/N`）。
- **`hermes ask` 是「无身份单轮」**：不加载 memory/skill store、无 companion 身份、确认自动放行一切（含高风险）——与 chat/run 显著不同，是最接近历史遗留的入口。

### 4.5 hermes-gui / hermes-server / hermes-channel（GUI / 后端 / 共享层）

- **GUI**：42 个 Tauri command（chat/session/memory/skills/mcp/onboarding/config/reflect/inbox/upload/wechat），全部直连引擎、**不经过 server**；`tauri.conf.json` 无 devUrl、`frontendDist: ./ui/dist`、`beforeBuildCommand: npm run build` → 防白屏真实落地。
- **确认闭环**：`ConfirmRequest` → 每连接确认桥 → oneshot `confirm_tokens` 表 → GUI `respond_confirm` / WS `confirm` 帧；`always_allowed_tools` 仅进程内有效。
- **server**：`/api/v1/*` REST + WS `/api/v1/chat`（帧 send/cancel/confirm）；token 来源 `--token → --token-file → HERMES_SERVER_TOKEN → ~/.lebi-ai/server.token`（自动生成 32B hex、0600），**任何情况非空，服务器永不裸奔**；恒定时间比较；默认绑定 `127.0.0.1:8765`；二进制内无 TLS（走反代）。
- **channel 是教科书式共享抽象**：`Channel` trait（关联类型 `Reply`：微信=入站原消息 / 飞书=open_id / TG=chat_id）+ `ServeCtx` + `handle_text_message` + `serve_inbound`（`CHAT_TOOL_WHITELIST` 15 个工具 = 无确认 UI 表面的安全边界，不含 bash）；系统提示 `compose_system_prompt` 缓存稳定。

### 4.6 hermes-weixin / feishu / telegram + Flutter + scripts（IM / 移动端 / 脚本）

- **微信**：iLink Bot 扫码（qrcode 轮询键 + 二维码 URL）+ 38s 长轮询；cursor 持久化去重；回包强制出站 schema（`client_id=wcb-{uuid}` 幂等、`context_token` 原样回显）；`service::serve` 一个循环服务 CLI/GUI 两 surface。
- **飞书**：WS 长连接（手写 protobuf 帧编解码、ping、ACK 200、随机抖动重连）；发消息走 HTTP + tenant token 缓存；仅 CLI 接线。
- **Telegram**：Bot token + 30s 长轮询；offset 持久化；仅 CLI 接线。
- **Flutter**：URL 在 SharedPreferences、token 只在 Keychain/Keystore；`hermesClientProvider` 监听变化自动重建 Dio/WS；WS 帧容错解析。已接：health/chat/sessions/skills/memories/config。**未接：reflect/mcp/uploads 路由与 MicroReflection/SkillCandidateProposed 事件**。
- **脚本**：`run-gui.sh`（npm build → cargo run，markitdown sidecar 三级解析）；`build-dmg.sh`（macOS DMG，强制 markitdown bundle 硬门槛）。

## 5. 关键数据流（一条消息走全链）

```
入口（CLI chat / GUI send_message / IM handle_text_message / WS send）
  → 时间头注入（inject_time_header，不落盘）
  → compose_system_prompt（companion 协议 + workspace + 工具策略 + 记忆宫殿）
  → ContextSources.build_turn_system（palace/profile/相关记忆/nudge）
  → hermes_turn::run_turn（流式 → ToolUse → 权限分类 → 并行安全执行 + 串行确认执行）
  → 结果回喂（care_after_tools_nudge 可被 reflect 过滤）
  → 新消息落 JSONL（SessionWriter，逐行 sync）
  → 轮后：micro-reflection（后台 spawn）+ effectiveness 统计 + propose_skill 队列
  → 会话结束：full reflection → 候选确认/拒绝 → skills/ + memories/ 明文落盘
```

## 6. 发现的问题清单（按「先文档后代码」顺序）

> ✅ = 已于 2026-08-07 处理完毕（台账：`docs/records/20260807-codebase-hygiene-30-issues.md`）。
> 未标 ✅ 的条目为**有意保留**的决策或后续任务（见文末说明）。

### A. 文档/注释不诚实（最优先）
1. ✅ **「BM25」是虚构**：`docs/project-map.md` §3.2 与 `README.md:305` 写「BM25 + triggers」，实际是 token 重叠（+可选 embedding hybrid）。
2. ✅ **「server 路由 1:1 GUI commands」不成立**：`crates/hermes-server/src/routes/mod.rs:9` 与 project-map 声称 1:1，实际缺 17 个 GUI 命令（regenerate/truncate/onboarding/session-end-reflect/inbox 4 个/check_markitdown/微信 6 个）。
3. ✅ **`docs/flutter-progress.md:22` 声称 M3「reflect/mcp REST 完成」**：Flutter 实际无任何调用，进化候选链路在移动端断头。
4. ✅ **`server/lib.rs:8-14` 过时注释**：说 management endpoints「later milestones」——早已实现。
5. ✅ **`cli/commands/context.rs` 注释不诚实**：声称 GUI 也走 hermes-channel，实际 GUI/server 各复制了一份 ContextSources（三份平行实现）。
6. ✅ **`agent.rs:5` 声称含 end-of-session reflection**：实际无任何 reflection 调用。
7. ✅ **`reflect_stats.rs:24` 指向不存在的 `built-in/reflect/SKILL.md`**（反射 prompt 实际在 `hermes-reflect/src/prompt.rs`）。
8. ✅ **`stats.rs:1-3` 头注释过时**：说 used=「正文回显检测」，实际是「有实质输出即记 Used」。
9. ✅ **`skill-creator/SKILL.md` 正文写 `~/.small-rust-hermes/`**：数据根已迁移 `~/.lebi-ai`。

### B. 未接线 / 半成品（进化链路与功能缺口）
10. ✅ **`deferred.jsonl` 只写不读**：「下个会话再评估」未实现；CLI micro pending 只打印数量无消费端。
11. ✅ **`save_zone_summary` / `compile_zone_summary` 无调用者**：zone-summary 缓存只读不写，`palace_read_zone` 永远命中不到缓存。
12. ✅ **`decay_factor`（记忆衰减）未接线**：只有单测引用，实际排序只用 `factor()`。
13. ✅ **GUI/server 的 accept/reject 不写 `reflect-log.jsonl`**：审计链中断（只有 CLI 与 micro auto-accept 写）。
14. ✅ **GUI/server 不注入 `always_active` 技能正文**：memory-palace 只在 CLI/IM 生效，GUI/Flutter 面缺失。
15. ✅ **`InboxSource::Micro` 从未入队**（GUI micro pending 走事件而非 inbox）。
16. ✅ **Flutter 静默吞掉 MicroReflection / SkillCandidateProposed 事件**。
17. ✅ **GUI 前端忽略 `toolExecStart` 事件**（TS 无该 case，工具执行摘要对 GUI 用户不可见）。
18. ✅ **`hermes ask` 确认全放行**（含 skill_install/bash 高风险），README 未提示差异。
19. ✅ **IM 渠道 `confirm_tx: None` 自动放行**：`CHAT_TOOL_WHITELIST` 含 skill_install/skill_delete/subagent/memory_delete，无确认 UI 的表面靠白名单兜底（边界明确但需知晓）。
20. ✅ **`hermes-mcp` 配置加载副作用**：发现 `officecli` 二进制时自动写 `~/.lebi-ai/skills/officecli/SKILL.md`（未经用户确认，与「进化须批准」有张力）。

### C. 死代码 / 品牌残留 / 小瑕疵
21. ✅ `NullToolHost`（core/tool_host.rs:35）全仓零调用；`is_dangerous_tool`（turn/lib.rs:49）零调用；`style.rs:80 cyan()` 零调用。
22. ✅ User-Agent 仍是 `small-rust-hermes/{version}`（anthropic.rs:80、openai.rs:59）。
23. ✅ Flutter 用户可见层仍显示 Hermes（`chat_screen.dart:199,316`、`chat_drawer.dart:43`），品牌记录要求 `rg -i hermes` 用户可见层归零。
24. ✅ `pubspec.yaml` 的 `go_router` 死依赖；`weixin-bot-api.md` 悬空引用；微信 `send_typing` 无调用方。
25. ✅ `docker-compose.yml:11` `${LEBI_HOME:-~/.lebi-ai}` 的 `~` 不会在 compose 内展开（建议实测）；`docs/docker.md:97` musl 建议过时（Dockerfile 是 glibc）。
26. ✅ `gen_tb_cases.py` + `docs/tb-case-data.md` + `data/tb_cases_2026_h1.csv` 与本产品无关（遗留）。
27. ✅ `hermes-turn` 测试缺口：`run_turn`/`run_agent` 无测试（只有 permissions 14 个 + danger 8 个）；`profile.rs` 单测是「假测试」（未走被测函数）。
28. ✅ `OpenAiProvider` 无重试/退避、流式缺 `BlockStop`、图片降级占位符（TODO 自标）。
29. ✅ `TurnEvent::Usage` 丢 cache_read/cache_creation 字段；MCP 错误类型在 `call` 被压平。
30. ✅ Tauri ACL 存疑：crate 无 `capabilities/` 目录，`gen/schemas/capabilities.json` 为空 `{}`，应确认默认放行是否符合预期。

> **C30 决策**：经源码核实（tauri 2.11 `webview/mod.rs`），本地 origin + 无 app ACL manifest 时自定义命令不受 ACL 限制；新增 capability 反而会 gate 全部自定义命令。保持现状为有意为之，未加 capability 文件。
> **未做项（后续任务）**：§7 四套 zone 定义收敛（统一到 `companion.rs`）不在本次范围。

## 7. 分区体系现状（疑点备忘）

四套 zone 定义并存：companion 常量（preferences/standards/work/general）、`palace.rs`（core/work/episode/general）、内置 memory-palace 正文（core/work/project:<name>/episode/general）、reflection prompt（preferences/standards/work）。`ZONE_EPISODE` 无使用方；relevance 中 `"episode"`/`"work"` 是字面量硬编码。后续若收敛分区应统一到 companion.rs。

## 8. 最值得记住的 10 件事

1. **改上下文改 `hermes-channel`，不是 CLI**：`ContextSources` / `compose_system_prompt` 是唯一事实源；CLI 的 `commands/context.rs` 只是 re-export。
2. **system prompt 必须字节稳定**（Anthropic prompt caching）：时间戳挪进用户消息；动态内容走每轮 `build_turn_system`。
3. **确认门禁分层清晰**：PermissionChecker（deny→allow→Prompt）→ danger.rs（bash 高危 + skill_install 恒确认）→ 前端三态确认；无确认 UI 的表面靠 `CHAT_TOOL_WHITELIST`。
4. **技能激活 100% 靠 `skill_read`**：匹配器只喂统计；正文永远不内联（有回归测试钉死）。
5. **进化候选默认不自动写入**：`auto_accept_memories=false` 有回归断言；supersedes 是知识收敛唯一机制，旧记录留盘审计。
6. **GUI 是「Tauri 薄壳 + 引擎直连」**：42 个 command 直接调 hermes-*，不经过 server；与 CLI 完全同数据。
7. **server 永不裸奔**：token 任何情况非空，默认 127.0.0.1，恒定时间比较，无内嵌 TLS。
8. **IM 渠道差异只剩「收到什么、回给谁」**：`Reply` 关联类型编码三渠道差异；`serve_inbound` 全共享。
9. **子代理递归是结构性禁止**：子 host 无 propose/subagent ctx + `max_depth=1` 原子守卫 + allow_tools 白名单。
10. **防白屏是真实落地**：无 devUrl、`frontendDist: ./ui/dist`、`build.rs` 兜底、`scripts/run-gui.sh` 强制 rebuild。

---

*本文为学习快照（2026-08-07），随代码演进可能过时；改动后请同步更新本报告或归档为历史版本。*
