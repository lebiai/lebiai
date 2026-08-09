# 变更记录：问题清单 30 条全量处理（代码卫生 + 进化链路接线）

| 字段 | 内容 |
|------|------|
| **编号** | `20260807-codebase-hygiene-30-issues` |
| **日期** | 2026-08-07 |
| **状态** | **已验收**（工程全绿；Flutter 仅静态审查、GUI 目视待手测） |
| **负责人** | Codex Agent |
| **关联** | 全量代码学习问题清单 `docs/codebase-learning.md` §6（A1–9 / B10–20 / C21–30） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 所有 lebi-AI 用户（CLI / GUI / Flutter / IM 渠道）与后续协作者。
- **解决什么痛点：** 文档声称与实现不符（BM25、1:1 路由、反射日志）导致协作者与用户被误导；进化候选在部分表面断头（Flutter 静默、micro pending 丢失、deferred 只写不读）；高风险工具在无确认 UI 的 IM 面可被自动放行；品牌残留（Hermes）与死代码（NullToolHost、tb 遗留）拉低「越用越像你的手感」的信任。
- **用完后用户多得到什么：** 文档诚实可查；进化候选（记忆/技能）在 GUI、server、Flutter 全链路可审、可入队、可复盘（reflect-log 完整）；IM 面不再自动执行高风险工具；GUI/移动端补上 memory-palace 等 always-active 技能；运行更稳（OpenAI 重试、错误链保留、run_turn 有测试）。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（进化候选不再静默消失，UI 有提示）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径（对话/进化/技能）比改前更可靠

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在 GUI/手机/IM 与乐彼AI 共事后，后台产出记忆/技能候选；协作者按文档改代码。
- **路径变化：**
  - 改前：候选只在 CLI 交互与 micro auto-accept 落 reflect-log；GUI/server 审批不落日志；Flutter 收到 MicroReflection/SkillCandidateProposed 事件直接丢弃；micro pending 不入收件箱；`hermes ask` 全放行且无提示；IM 白名单含 skill_install/subagent 等高风险工具。
  - 改后：GUI/server 审批与冲突处置全部写 `reflect-log.jsonl`；server 与 GUI 的 micro pending 均入 `pending-review.json`（InboxSource::Micro）；Flutter 显示后台进化提示条；IM 白名单移除 4 个高风险工具；README 明示 `ask` 轻量模式差异；MCP 配置不再自动写技能（进化须批准）。
- **成功标准：** 任意入口产生的候选都能被记录、被审、被复盘；IM 面高风险工具不可达；用户可见品牌无 Hermes 残留。
- **明确不做什么：** 不新增 IM 确认 UI（白名单兜底即可）；不做 Flutter 候选审核页（提示条 + 桌面收件箱审核）；不收敛四套 zone 定义（§7 备忘，另立任务）。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 文档诚实（P3 层级）/ 进化链路状态机（inbox、deferred、reflect-log 三件套未对齐）/ 安全边界（无确认 UI 的表面靠白名单）/ 死代码（P0 第九条）。
- **正确的长期默认路径：**
  - reflect-log 是唯一审计链：所有 accept/reject/defer/merge/scope_split 一律 `log_append`；GUI、server、inbox 共享同一 `hermes_reflect::log`。
  - pending-review inbox 是所有「待人工审」候选的汇聚点：CLI（deferred 重评）、GUI micro、server micro 全部 `enqueue_from_reflection`；事件只是即时通知。
  - always-active 技能 = 会话级系统提示的一部分：`hermes-channel` 已实现，GUI/server 对齐同一语义（正文内联，索引只含 name+description）。
  - 无确认 UI 的表面（IM/ask）＝白名单 + 权限检查器兜底；白名单只收录手审过的低风险工具。
- **与引擎/各入口边界：** 引擎（hermes-reflect/memory/turn/llm）为唯一实现；GUI/server 只补事件与 UI 呈现；server 路由保持「GUI 子集 + WS 帧」定位（问题 2 已修文档）。
- **安全影响：** 移除 IM 白名单高风险工具 = 减小自动执行面；`hermes ask` 维持全放行但已明示；MCP 不再自动写用户技能目录。
- **如何防复发：** 新增入口必须走 `enqueue_from_reflection` + `log_append`；新增工具进白名单必须确认无确认 UI 表面可达；文档改动须过「诚实性」检查。
- **为何这不是补丁：** 每条都落在共享引擎/共享协议的默认路径上（inbox/log/skill 注入），并同步清理旧路径与死代码。

---

## 1. 方案（Plan）

- **目标：** 按 A→B→C 顺序处理 30 条；工程门槛 `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `npm run build` 全绿。
- **范围：** 做：文档诚实化、进化链路接线、安全白名单、死代码/品牌清理、测试补强、OpenAI 重试、Usage 字段、MCP 错误链、ACL 调查。不做：Flutter 审核页、zone 收敛、Tauri capability 新增（见 C30）。
- **技术要点：** 触及 hermes-core/turn/llm/reflect/memory/skills/mcp/channel/weixin/cli/gui/server/flutter；无新增运行时依赖（hermes-server 补 `chrono` workspace 依赖）。
- **风险与回滚：** Flutter 无法本地编译验证（静态审查 + 结构最小改动）；C30 保持现状有源码级依据。
- **方案确认：** [x] 已对照 P0/P1（含第七条/第九条）· 2026-08-07

---

## 2. 实施（Implement）

### A 组 · 文档/注释诚实（9 条全完成）

| # | 处理 |
|---|------|
| A1 | `docs/project-map.md:58`、`README.md:307`：BM25 虚构 → 「token overlap + triggers，可选 embedding hybrid」 |
| A2 | `crates/hermes-server/src/routes/mod.rs` 头注释 + project-map：1:1 → 「GUI 子集 + WS 帧」 |
| A3 | `docs/flutter-progress.md`：M3 reflect/mcp 标注「server 已实现、Flutter 未接线」（事件曾静默丢弃） |
| A4 | `server/lib.rs` 过时注释更新为已有 REST 端点 |
| A5 | `cli/commands/context.rs` 注释诚实化（GUI/server 各持副本） |
| A6 | `cli/commands/agent.rs` 头部声明移除「end-of-session reflection」（实际无） |
| A7 | `cli/commands/reflect_stats.rs` 失效路径 → `hermes-reflect/src/prompt.rs` |
| A8 | `skills/stats.rs` 头注释更新（used = 有实质输出） |
| A9 | `skill-creator/SKILL.md` 路径 → `~/.lebi-ai` |

### B 组 · 未接线 / 半成品（9/11 完成，2 项以最小方案闭环）

| # | 处理 |
|---|------|
| B10 | `deferred.jsonl` 真正可重评：`cli/commands/reflect.rs` Defer 写队列；新增 `review_deferred()` 在 `run_with_min_turns` 开头用同一审批门重评估，重新 defer 回写，stdin 关闭保留文件 |
| B11 | zone-summary 死路径全删：`memory/palace.rs`（zone_cache_dir/load/save_zone_summary/sanitize_zone_filename）、`memory/lib.rs` 导出、`tools/palace.rs` 缓存分支、`reflect/compile.rs`（compile_zone_summary + COMPILE_ZONE_SYSTEM）+ 相应死测试 |
| B12 | `decay_factor` 接线进 `memory/relevance.rs::search_memories_with_effectiveness` |
| B13 | GUI/server 的 accept/reject/conflict 全部写 `reflect-log.jsonl`（Accept/Reject/Merge/ScopeSplit 对应动作；GUI inbox accept/reject 也写日志）；`handle_conflict` 硬编码 `"general"` zone bug 修复（GUI/server 读请求 zone，GUI 前端传 `candidate.zone`） |
| B14 | GUI/server `context.rs::build_turn_system` 注入 always_active 技能全文（与 `hermes-channel` 语义一致），并清理 `#[allow(dead_code)]` 的 `tools` 死字段（三处调用点同步） |
| B15 | GUI micro pending 入 inbox（`InboxSource::Micro`）；server 同路径对齐（见 B16） |
| B16 | **Flutter 事件断头闭环**：server micro pending 也 `enqueue_from_reflection`；Dart 新增 `MicroReflection` 模型 + `SkillCandidateProposed/MicroReflection` 提示条（`ChatState.notice` + `_NoticeBanner`）；`ToolExecStart` 原已接线 |
| B17 | GUI 前端补 `toolExecStart`：`types/index.ts` 联合类型 + `chatStore.ts` 更新 summary + `MessageBubble` 工具行显示摘要（`tsc --noEmit` 通过） |
| B18 | `README.md` Usage + `ask.rs` 文档注释：明示无身份/无记忆技能/确认全放行，建议 `chat` 处理高危 |
| B19 | `CHAT_TOOL_WHITELIST` 移除 `skill_install / skill_delete / memory_delete / subagent`（无确认 UI 表面不可达）；注释更新 |
| B20 | `mcp/config.rs` 移除 `officecli` 自动检测/自动写技能副作用（`load_default` 只读配置；死 helper 全删） |

### C 组 · 死代码 / 品牌 / 小瑕疵（21–26 全完成；27–30 工程健壮性）

| # | 处理 |
|---|------|
| C21 | 删除 `NullToolHost`、`is_dangerous_tool`、两个 `cyan()` |
| C22 | User-Agent → `lebi-ai/{version}`（anthropic.rs / openai.rs） |
| C23 | Flutter 用户可见品牌 → lebi-AI / 乐彼AI（chat_screen / chat_drawer） |
| C24 | `go_router` 依赖删除；`weixin-bot-api.md` 悬空引用删除；`send_typing` 接线进微信 serve 循环 |
| C25 | `docker-compose.yml` `${LEBI_HOME:-$HOME/.lebi-ai}`；`docs/docker.md` musl → glibc buildx |
| C26 | tb 遗留三件套删除 + 墓碑 `20260807-remove-tb-legacy.md` |
| C27 | `memory/profile.rs` 假测试 → 走真实 `save_profile/load_profile`（`LEBI_DATA_DIR` + Mutex 串行）；`hermes-turn` 新增 4 个 `run_turn` 测试（纯文本 / 工具回喂 / 确认放行 / 确认拒绝，stub provider + echo host） |
| C28 | 共享 `hermes-llm/src/retry.rs`（is_retriable_status/backoff_delay/parse_retry_after）；OpenAI `complete`/`stream` 接入重试退避；图片降级注释诚实化；流式 `finalise` 补 `BlockStop{index}` 对齐 Anthropic |
| C29 | `TurnEvent::Usage` 增 `cache_read_tokens/cache_creation_tokens`（从 `cumulative_usage` 填充）；MCP `call` 错误改 `error_chain()` 保留根因链（含单测） |
| C30 | Tauri ACL 源码级调查（tauri 2.11 `webview/mod.rs`）：本地 origin + 无 app ACL manifest 时自定义命令**不受限**；一旦新增 capability 反而会 gate 全部 42 个命令。**保持现状**（有意为之），如实记录 |

## 3. 测试（Test）

| # | 用例 | 结果 | 备注 |
|---|------|------|------|
| 1 | `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 全仓 0 警告（含新增代码） |
| 2 | `cargo test --workspace` | 通过 | 全部 crate 绿（含新增 turn 4 例、retry 2 例、profile 3 例、error_chain 2 例、always_active 注入 1 例） |
| 3 | `cd crates/hermes-gui/ui && npm run build`（+ `npx tsc --noEmit`） | 通过 | chunk 体积警告为存量，非本次引入 |
| 4 | `cargo check` 相关 crate | 通过 | core/turn/llm/reflect/memory/skills/mcp/channel/weixin/cli/gui/server |
| 5 | Flutter 静态审查 | 通过（无法运行 analyze） | 无 flutter 工具；改动最小化且结构对齐既有模式 |
| 6 | 手工 GUI 目视（工具摘要 / 技能注入 / 进化提示） | 待用户手测 | 记录为遗留项 |

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 进化候选全链路可审可复盘；IM 面安全收敛 |
| 开箱即用未破坏 | ✅ | 无新增运行时；GUI 仍走 `ui/dist` |
| 本地优先未破坏 | ✅ | 数据仍本地明文 `~/.lebi-ai` |
| 测试通过 | ✅ | clippy -D warnings / cargo test / npm build 全绿 |
| 记录完整 | ✅ | 本台账 + 墓碑（tb 遗留）+ 索引更新 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补 | ✅ | 全部落共享引擎/共享协议默认路径 |
| 代码卫生（P0 第九条） | ✅ | 同步清理死字段/死函数/死测试/失效注释/过时依赖 |

- **验收人：** Codex Agent（工程）；GUI 目视 + Flutter 真机待用户
- **验收日期：** 2026-08-07
- **结论：** ✅ 工程通过 · 遗留：GUI 目视手测、Flutter 真机手测、`docs/codebase-learning.md` §6 已更新标记

## 5. 附注

- `docs/codebase-learning.md` §6 已为已解决条目加 ✅ 标记并保留未做项（zone 收敛）说明。
- 关键依据：Tauri 2.11.1 `src/webview/mod.rs`「Check ACL on plugin commands, when the app defined its ACL manifest, or when the request comes from a non-local (remote) origin」——本地 origin + 无 app ACL = 自定义命令放行，符合「GUI 薄壳 + 引擎直连」设计。
