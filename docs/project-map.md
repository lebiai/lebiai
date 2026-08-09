# lebi-AI（乐彼AI）· 项目全景地图（梳理入口）

| 字段 | 内容 |
|------|------|
| **版本** | 2026-08-06 |
| **定位** | 产品/架构现状总览；**非** P0/P1 权威。冲突以根目录权威文档为准。 |
| **关联** | [`AGENTS.md`](../AGENTS.md)、[`PRODUCT_PRINCIPLES.md`](../PRODUCT_PRINCIPLES.md)、[`work-companion-solution.md`](./work-companion-solution.md)、[`records/`](./records/) |

---

## 1. 一句话

**乐彼AI（lebi-AI）= 本地工作搭子 AI。** Slogan：**越用越像你的手感。**  
一句话：**接得住想法，推得动事，必要时敢顶你——第二次更准。**  
四环：Do × Continuity × Care × Evolve。定义卡见 P0 v0.3。  
**不是**闲聊玩具、讨好机器、律师垂直、写代码专用、生活陪聊主叙事。  
蓝图：[`work-companion-solution.md`](./work-companion-solution.md)。

---

## 2. 用户主路径（当前默认）

```
clone / 下载二进制（或 Docker 镜像）
  → hermes init（或手写 ~/.lebi-ai/config.toml，0600）
  → 对话：ask / chat / run（或 GUI / 手机 / IM）
  → 会话结束 → full reflection 提炼 skill / memory / conflict 候选
  → 用户确认 / 拒绝 → 明文 Markdown 落盘
  → hermes distill 收敛近似记忆（可选 --apply / --llm-merge）
```

**产品决策（已确认）：** 进化候选**默认不自动写入**，必须用户确认；micro-reflection
只在轮次间异步轻量运行，绝不阻塞输入。

---

## 3. 架构分层

### 3.1 技能分类（正式分类 · 禁止「系统/用户 skill」二分）

| 分类 | 内容 | 可否删除 |
|------|------|----------|
| ① 引擎能力 | tools / turn / prompt / 权限 / 安全（`hermes-tools` 等，**不是** skill） | — |
| ② 内置技能 | memory-palace（生成）、skill-creator、find-skills（`include_str!`） | **否** |
| ③ 用户技能 | `~/.lebi-ai/skills/` 自建 / `skill_install` | 可 |
| ④ 项目技能 | `./.lebi-ai/skills/` 项目内共享 | 可 |

### 3.2 代码地图

| 路径 | 职责 |
|------|------|
| `crates/hermes-core` | 核心抽象：Session / LlmProvider / ToolHost / 上下文压缩；零 UI 依赖 |
| `crates/hermes-llm` | Anthropic + OpenAI-compatible provider（含 DeepSeek 兼容） |
| `crates/hermes-turn` | 回合引擎：工具循环、并行执行、权限（confirm） |
| `crates/hermes-tools` | 内置工具：read/write/edit/bash/git/think/todo/web/subagent/记忆/技能… |
| `crates/hermes-mcp` | MCP 客户端（rmcp）：stdio + Streamable HTTP |
| `crates/hermes-store` | JSONL 会话持久化、frontmatter 解析 |
| `crates/hermes-skills` | 技能域：解析、存储、匹配（token overlap + triggers，可选 embedding hybrid）、安装/删除、内置清单 |
| `crates/hermes-memory` | 记忆宫殿：分区、supersedes 链、effectiveness、distill 聚类 |
| `crates/hermes-reflect` | full + micro reflection、profile 编译、候选输出 |
| `crates/hermes-cli` | CLI 主入口：init/doctor/ask/chat/run/skills/memory/session/distill/mcp/serve/wechat/feishu/telegram… |
| `crates/hermes-gui` | 桌面 GUI（Tauri 2）：确认弹窗、记忆侧栏、技能 CRUD、设置 |
| `crates/hermes-server` | HTTP/WS 后端（Flutter 用）：bearer token 鉴权；路由覆盖 GUI 命令子集（WS 帧对应 chat） |
| `crates/hermes-channel` | IM 渠道共享驱动（`Channel` / `ServeCtx` / `serve_inbound`），CLI 与 GUI 共用 |
| `crates/hermes-weixin/feishu/telegram` | IM 渠道：仅协议差异（扫码/WS/长轮询），`Channel` 实现各自落位 |
| `clients/flutter` | 三端客户端（iOS/Android/macOS）：聊天、会话、管理面板、多模态/语音 |
| `~/.lebi-ai/` | config.toml / mcp.json / wechat.toml / feishu.toml / skills / memories / sessions / reflect-log.jsonl |
| `scripts/build-dmg.sh` | macOS DMG 打包 |
| `docs/records/` | 变更台账（强制） |

### 3.3 数据边界（用户必须分清）

| 数据 | 是什么 | 不是什么 |
|------|--------|----------|
| **会话** | JSONL 转录（`sessions/`） | 不是结构化知识 |
| **记忆** | 用户确认的持久事实/偏好（Markdown + frontmatter） | 不是未审流水账 |
| **技能** | 用户确认的可复用流程（Markdown + frontmatter） | 不是引擎工具替代品 |
| **reflect-log** | 接受/拒绝审计日志 | 不可作为已生效知识 |
| **MCP / Web 检索** | 用户主动接入的外部能力 | 内容未经本地核验 |

---

## 4. 用户可见能力（各入口）

| 入口 | 能力 | 成熟度（相对） |
|------|------|----------------|
| CLI | ask / chat / run / init / doctor / skills / memory / session / reflect-stats / distill | 高 |
| CLI 渠道 | wechat / feishu / telegram 长连对话、工具 🔧 摘要推送 | 高 |
| GUI（Tauri） | 对话、确认弹窗、记忆侧栏、技能 CRUD、设置、conflict UI | 高 |
| server + Flutter | REST/WS 聊天、会话、管理面板、图片输入、语音、后台推送（需凭证） | 中高（M0–M4） |

### 内置技能（②）

1. memory-palace（记忆宫殿协议，启动自动生成）
2. skill-creator（技能创作 + 子代理评测）
3. find-skills（从 skills.sh 生态查找/安装技能）

---

## 5. 近期已验收主线（台账）

| 台账 | 内容 | 状态 |
|------|------|------|
| `20260806-give-and-take-pushback` | **有来有回**（理解≠赞同 · 选项 · 你定） | **已实施** |
| `20260806-care-after-delivery` | **Care 交付后改进**（通用工作时刻，非垂直） | **已实施** |
| `20260806-csess-work-episode-loop` | **C-SESS 工作情节闭环**（种子/加权/再认出） | **已实施** |
| `20260806-work-companion-complete` | **工作与陪伴完整方案**（蓝图 + companion 协议 + 对话化） | **已实施** |
| `20260805-gui-ritual-system` | GUI 全站仪式感与视觉统一（A–E，不围着反思） | **已验收**（2026-08-06「仪式通过」） |
| `20260805-template-feature-removed` | **移除文档模板功能**（占位符方案废弃；唯一墓碑） | **已实施** |
| `20260806-doc-hygiene-dead-templates` | 删除已废弃模板设计文档与中间取消台账 | **已验收** |
| `20260803-authoritative-docs` | 权威文档体系（P0/P1/P2 + docs 索引 + 台账） | **已验收**（文档） |
| `20260803-pre-dev-review-rules` | 开发前全面审查 + 规则定稿（P1/AGENTS 增补、README/docker 修正、TODO 迁移、缺口表） | **已验收**（规则/文档） |
| `20260803-rule-accept-default-false` | RULE-ACCEPT：auto_accept 默认值对齐 P0 | **已验收** |
| `20260803-reflect-end-session-reflection` | REFLECT-END：CLI 会话结束 full reflection 接线 + 清死代码 | **待验收**（真机手测） |
| `20260803-reflect-end-manual-acceptance` | REFLECT-END 真机手测验收（会话结束自动提炼） | **待验收** |
| `20260803-telegram-offset-and-docs` | TELEGRAM：offset 持久化 + README 对齐 | **已验收**（实现）· 端到端待手测 |
| `20260803-fmt-check` | FMT-CHK：全仓 cargo fmt + CI 加 fmt 检查 | **已验收** |
| `20260803-token-secure-storage` | TOKEN-STORAGE：移动端 token 改用 flutter_secure_storage | **待验收**（需 Flutter 环境） |
| `20260803-gui-session-end-reflection` | G0：GUI 会话结束 full reflection + 候选确认 | **待验收**（真机手测） |
| `20260803-tb-case-data` | 创建 100 条结核病病案合成数据（2026 H1） | **已移除**（20260807，与本产品无关） |
| `20260803-clippy-fix-and-ci` | CLIPPY-1：clippy 修复 + CI 工作流 | **已验收** |
| 上游提交 | Flutter client + hermes-server（bearer-token auth） | 上游已合入 main |
| 上游提交 | `hermes distill` + `memory_distill` + `memory_save supersedes` | 上游已合入 main |

完整索引：[`records/README.md`](./records/README.md)。

---

## 6. 缺口与建议优先级（重新开干时用）

### P0 · 工程与安全（建议下一主线）

| ID | 主题 | 说明 |
|----|------|------|
| **RULE-ACCEPT** | 配置默认值对齐 P0 | ✅ 已处理（`20260803-rule-accept-default-false`）：`auto_accept_memories` 默认 `false`（Default impl + init 模板 + 回归断言） |
| **REFLECT-END** | CLI 会话结束 full reflection 接线 | 实现 ✅（`20260803-reflect-end-session-reflection`）；真机手测 ⬜ **待验收** |
| **G0-GUI-REFLECT** | GUI 会话结束 full reflection | 实现 ✅（`20260803-gui-session-end-reflection`：离开会话统一出口 + min_turns + 审阅 modal）；真机手测 ⬜ **待验收** |
| **CLIPPY-1** | clippy `-D warnings` 修复 + CI | ✅ 已处理（`20260803-clippy-fix-and-ci`）：`sort_by_key(Reverse)` 修复；`.github/workflows/ci.yml` 落地（check/clippy/test）；实跑待 push |
| **FMT-CHK** | `cargo fmt --check` 未纳入 | ✅ 已处理（`20260803-fmt-check`）：全仓 `cargo fmt` 基线 + CI 加 `cargo fmt -- --check` 步骤 |
| **DOC-H** | 文档卫生 | ✅ 已处理（本审查内）：`TODO.md` 已迁入 `docs/flutter-progress.md`，根目录白名单合规 |
| **DOCK-CHK** | Docker 与 README 一致性 | ✅ 已处理（本审查内）：`docs/docker.md`「distroless」已修正为 debian-slim，与 Dockerfile 一致 |

### P1 · 体验与诚实

| ID | 主题 |
|----|------|
| README-HONESTY | ✅ 已处理（本审查内）：README「reflection at session end」改为「/reflect 手动 + roadmap」；架构图补 `hermes-server`/`hermes-telegram`；File Layout 补 `server.token`/`telegram.toml` |
| MOBILE-E2E | Flutter 多模态/语音/推送的真机端到端验证（需 APNs/FCM 凭证 + 设备） |
| WS-RELIAB | 微信长轮询断线重连与超时的压力验证 |
| TOKEN-STORAGE | 移动端 token 明文存 `shared_preferences` | ⬜ 已实施（`20260803-token-secure-storage`）：token 迁入 `flutter_secure_storage`（Keychain/Keystore）+ Android minSdk 23 + macOS entitlement；**待 Flutter 环境验收**（本机无 Flutter SDK） |

### P2 · 非主路径

| ID | 主题 |
|----|------|
| TELEGRAM | Telegram 渠道完善与 README 对齐 | ✅ 主体完成（`20260803-telegram-offset-and-docs`）：offset 持久化（`telegram-offset.txt`）+ README 章节；端到端重启验证待真机手测 |
| CLOUD-SYNC | 多设备数据同步（本期不做，本地优先） |

---

## 7. 协作规则（不重复全文）

1. 先读 **P0 → P1 → AGENTS**
2. 有用户影响 → `docs/records/YYYYMMDD-slug.md` 走完方案→实施→测试→验收
3. 改 skill/工具先归入 ①②③④
4. 禁止修修补补；改完清旧；server 与 GUI commands 保持 1:1

---

## 8. 推荐工作方式（「重新梳理」之后）

1. **对齐目标**：仍是「自我进化的本地 agent，单一引擎多入口」
2. **下一主线默认建议：CI + 文档卫生**（低成本、防回归）
3. 并行可做：Flutter 真机验证、微信渠道压测
4. 打包/发布冲刺放在核心手测通过之后

---

*本文件随重大阶段更新版本日期；细节以台账与代码为准。*
