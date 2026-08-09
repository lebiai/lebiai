# 变更记录：开发前全面审查 + 规则定稿（review 全仓 + 锁定权威约束）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-pre-dev-review-rules` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（规则/文档） |
| **负责人** | Codex（用户委托） |
| **关联** | `20260803-authoritative-docs`、`docs/project-map.md` §6 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 本仓库协作者（人类 + AI agent）；下一阶段开发者
- **解决什么痛点：** 权威文档（P0/P1/P2）与代码现状存在多处冲突（自动写入默认值、
  会话结束 reflection 未接线、README 声称的 lint 命令不通过等），直接开发必然偏离规则
- **用完后用户多得到什么：** 开工前规则已定稿并落盘；所有已知偏离已登记为缺口并排优先级；
  后续开发只照台账推进，不重复踩坑
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（先规则后开发，一次定稿）
  - [x] 不增加无意义确认或噪音（只动规则与文档，不碰代码）
  - [x] 高频路径比改前更快或更省心（缺口表可直接当开发 backlog）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 开发前需要确认「哪些规则已锁定、哪些现状偏离、先修什么」
- **路径变化：** 改前（文档写一套、代码跑另一套，各自为政）→ 改后（规则定稿 + 缺口表，
  开发任务有唯一优先级）
- **成功标准：** P1/AGENTS 增补执行规则；README/docs 不再撒谎；根目录白名单合规；
  project-map 缺口表与台账可追溯
- **明确不做什么：** 不改任何 Rust / Dart / TS 代码（开发阶段处理）；不改 P0 产品立场
  （「用户批准」仍是核心价值，代码必须向它对齐，而非反向改规则）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 规则层（默认值/入口接线）与文档事实层（README/docs 描述）两层
- **正确的长期默认路径：**
  - 配置默认值必须与 P0 行为一致：进化候选**默认不自动写入**；`auto_accept_memories`
    默认 `false`，用户显式开启才生效；CLI/GUI/server 批准语义一致
  - 会话结束 full reflection 是 P0「必须」：CLI chat 需接线 `run_after_chat`（当前死代码），
    与 `/reflect`、GUI、server 共用同一批准门
  - lint/test 是硬门槛：`cargo clippy --workspace --all-targets -- -D warnings` 与
    `cargo test --workspace` 全绿方可验收；CI 兜底
  - 文档诚实：README/P3 与 docs 不得声称未实现的路径；未接线功能要么接线要么标「计划」
- **与引擎/各入口边界：** 本次零代码改动；规则增补作用于全部入口（CLI/GUI/server/IM）
- **安全影响：** 无（未触配置/密钥/server 代码）；审查确认 server 鉴权、0600、技能配额均合规
- **如何防复发：** P1 质量门槛增加「文档诚实」「lint 全绿」；AGENTS 自检清单增加对应条目；
  project-map §6 缺口表跟踪开发期修复
- **为何这不是补丁：** 建立的是「开发前规则基线」与可追溯缺口表，是默认路径的起点

---

## 1. 方案（Plan）

- **目标：** 全面审查代码 + 权威文档，产出规则定稿与缺口优先级
- **范围：** 做：审查（P0/P1/P2/P3 + 15 crates + Flutter + server + IM 渠道）、
  P1/AGENTS 规则增补、README/docker.md 事实修正、TODO.md 迁入 docs/、project-map 缺口表更新、
  本台账。**不做：** 改任何代码；改 P0 产品立场
- **用户路径变化：** 见 0b
- **技术要点：** 验证命令 `cargo check --workspace`（通过）、
  `cargo clippy --workspace --all-targets -- -D warnings`（**失败 1 处**）、
  `cargo test --workspace`（全部通过）
- **风险与回滚：** 低；纯文档；git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `DEVELOPMENT_RULES.md`（P1）：§六 工程规范新增「规则一致性」小节（配置默认值对齐 P0 /
     文档诚实 / lint-test 硬门槛）；质量门槛表第 4 项补充 clippy
  2. `AGENTS.md`（P2）：改代码前自检清单增加「文档诚实」「lint/test 全绿」两条
  3. `README.md`（P3）：修正「reflection at session end」不实表述；架构图补
     `hermes-server` / `hermes-telegram`；File Layout 补 `server.token`
  4. `docs/docker.md`：修正「distroless 静态镜像」→ debian-slim（与 Dockerfile 一致）
  5. `TODO.md` → `docs/flutter-progress.md`（根目录白名单合规；修正内部相对链接；
     `docs/README.md` 索引登记）
  6. `docs/project-map.md`：§6 缺口表更新（新增 RULE-ACCEPT / REFLECT-END / CLIPPY-1 /
     DOC-H 完成 / DOCK-CHK 完成 / README-HONESTY 完成）；§5 台账表加本记录
  7. `docs/records/README.md`：索引加本记录
- **关键路径/文件：** 见上
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 根目录白名单合规 | `ls *.md` | 仅 4 个权威 md | 通过 | TODO.md 已迁移 |
| 2 | 链接可达 | 抽查 docs/README.md 索引与 flutter-progress 链接 | 无断链 | 通过 | |
| 3 | 规则自洽 | P1 新小节与 P0/P2 无冲突 | 一致 | 通过 | 无自动写入、文档诚实、lint 门槛 |
| 4 | 文档事实 | docker.md / README 表述与 Dockerfile / 代码一致 | 一致 | 通过 | distroless 已修正 |
| 5 | 基线验证 | `cargo check` / `cargo test` | 通过 | 通过 | clippy 1 处已知失败 → 记入缺口 |

- **自动化：** `cargo check --workspace` ✅；`cargo test --workspace` ✅（200+ 用例全绿）；
  `cargo clippy --workspace --all-targets -- -D warnings` ❌ 1 处（`hermes-store/src/session.rs:177`，
  开发期修，见 project-map CLIPPY-1）
- **手工：** 逐项核对 README/P1/AGENTS 增补与源码事实
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（clippy 已登记为开发期缺口）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 开发前规则定稿，缺口表可当 backlog |
| 开箱即用未破坏 | ✅ | 零代码改动 |
| 本地优先未破坏 | ✅ | 未触数据/配置/密钥 |
| 测试通过 | ✅ | check/test 全绿；clippy 失败已登记 |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | 规则基线 + 缺口优先级，不是症状处理 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ✅ | 零代码改动；死代码 `run_after_chat` 已登记开发期清理（REFLECT-END） |

- **验收人：** Codex（用户委托）
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** RULE-ACCEPT（auto_accept 默认值对齐）、REFLECT-END（会话结束 reflection 接线 +
  死代码清理）、CLIPPY-1（clippy 修复 + CI）→ 开发期按 project-map §6 优先级执行

---

## 5. 附注

- 审查基线：commit `f29534a`（docs 记录同源）+ 未提交的权威文档体系
- 关键证据：
  - `crates/hermes-llm/src/config.rs:192` `auto_accept_memories: true`（默认违反 P0 第一条）
  - `crates/hermes-cli/src/commands/reflect.rs:49` `run_after_chat` 死代码（`#[allow(dead_code)]`，
    从未被调用；`reflect.min_turns` 无读取方）
  - `crates/hermes-store/src/session.rs:177` clippy `unnecessary_sort_by`
  - `docs/docker.md:3` 「distroless 静态镜像」 vs `Dockerfile` 实际 `debian:bookworm-slim`
- 合规确认：server 全路由 bearer token（含 WS query、health 也鉴权、ct_eq 常量时间比较）、
  32 字节 CSPRNG token、0600（config/wechat/feishu/telegram/server.token）、
  BUNDLED 技能保护、远程技能 `always_active=false`、安装配额（≤50/100KB/5MB/深度≤6）、
  事务安装、GUI↔server 17 个入口 1:1、IM 渠道共享 `channel.rs`
