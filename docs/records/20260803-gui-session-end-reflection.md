# 变更记录：G0 — GUI 会话结束自动 full reflection + 候选确认

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-session-end-reflection` |
| **日期** | 2026-08-03 |
| **状态** | **已实施 · 待手测**（含非阻塞离开修复） |
| **负责人** | Grok（用户委托） |
| **关联** | P0 第一条自我进化；CLI `run_with_min_turns`；用户确认「GUI 必须 session-end reflection」 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 桌面 GUI 用户（主交付面）
- **解决什么痛点：** 进化是产品核心承诺，但 GUI 只能手动点 Reflect；用户正常「新建/切换会话」时不会提炼技能与记忆，CLI 有、主交付没有
- **用完后用户多得到什么：** 聊够轮次后离开会话 → 自动提炼 → 在窗内确认候选 → 知识库可进化，无需记着去 Reflect 面板
- **好用性自检：**
  - [x] 不需要终端 / 数据库
  - [x] 轮次不足静默跳过，少打扰
  - [x] 失败不挡离开与会话保存
  - [x] 与手动 Reflect 同一批准门

---

## 0b. 产品经理视角（必填）

- **场景：** 用户在 GUI 聊完一段后点「新建聊天」或切换历史会话
- **路径变化：** 改前（直接离开，进化断）→ 改后（达 `min_turns` 则自动提炼并审阅，再离开）
- **成功标准：** 见验收表 §3（7 条）
- **明确不做什么：** 不改 CLI；本期不做 server/Flutter 对称；不做持久 deferred 队列一期；不打包

---

## 0c. 架构师视角（必填）

- **根因层级：** 入口接线缺失（GUI 有 reflect API + UI，无「离开会话」统一出口）
- **正确的长期默认路径：** 所有离开当前会话动作 → 同一 `leaveCurrentSession` 编排 → `run_session_end_reflection`（门禁 + reflect）→ 有候选则审阅 UI → 再执行原离开动作；与 CLI quit-driven 语义一致（`min_turns`、失败不阻塞）
- **与引擎/各入口边界：** 反射仍走 `hermes_reflect::reflect`；批准仍走既有 accept commands；server 跟随属后续
- **安全影响：** 候选默认不自动写入（auto_accept 仅既有 memory 规则）
- **如何防复发：** 新建/切换/删除当前会话/关窗均走统一出口；README 与实现同真
- **为何这不是补丁：** 补的是主交付面的产品默认路径，不是遮错

---

## 1. 方案（Plan）

- **目标：** GUI session-end full reflection
- **范围：** `hermes-gui` Rust commands + React UI；文档诚实
- **技术要点：**
  1. `run_session_end_reflection`：读 `config.reflect.min_turns`，计数 user 文本轮，不足 `Skipped`；否则 `Completed{reflection}`
  2. 前端 `runAfterSessionEnd(action)`：流式中拒绝离开；否则 invoke → 空/跳过直接 action；有候选 modal 审阅后 Continue
  3. Sidebar new/load/delete 与窗口 CloseRequested 接线
  4. 审阅 UI 与 ReflectPanel 复用组件，禁止复制第二套批准逻辑
- **风险与回滚：** 关窗若权限不足则 best-effort；反射失败放行离开
- **方案确认：** [x] 2026-08-03 / 用户确认 G0=是

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. Rust：`run_session_end_reflection`（`min_turns` 门禁 + `Skipped`/`Completed`）；`run_reflection` 仍无门禁（手动面板）；user 文本轮计数单测
  2. 前端：`runAfterSessionEnd` 统一出口；新建/切换/删除当前会话/关窗接线
  3. 有候选 → `SessionEndModal` + 共享 `ReflectionReview`（与 Reflect 面板同批准逻辑）
  4. 无候选 / 跳过 / 失败 → 直接执行离开；流式中禁止离开
  5. README / i18n / project-map / records 索引同步
- **关键路径/文件：**
  - `crates/hermes-gui/src/commands/reflect.rs`、`main.rs`
  - `crates/hermes-gui/ui/src/store/chatStore.ts`、`App.tsx`、`Sidebar.tsx`
  - `components/reflect/{ReflectionReview,SessionEndModal,ReflectPanel}.tsx`
  - `types/index.ts`、`i18n.ts`、`README.md`
- **偏离方案处：** server/Flutter 未对称（按方案不做）；关窗不阻塞等 LLM
- **性能修复（同议题）：** 离开会话改为**先离开、后台提炼**（不再卡在「正在提炼…」）；session-end 用 `reflect_quick`（max_tokens 3072）；有候选再弹审阅；右下角轻量进度可关闭

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 够轮次离开会提炼 | ≥min_turns 后 New Chat | 出现提炼/审阅 | | |
| 2 | 短会话不打扰 | 1 轮后切换 | 静默离开 | | |
| 3 | 无候选直接走 | 够轮次但无候选 | 不挡离开 | | |
| 4 | 接受写入 | 接受 memory | 磁盘有记忆 | | |
| 5 | 拒绝不写 | 拒绝 | 不落盘 | | |
| 6 | 失败可离开 | mock/断网 reflect | 会话仍在，可离开 | | |
| 7 | 手动 Reflect 仍可用 | Reflect 面板 Run | 同前 | | |

- **自动化：** `cargo test -p hermes-gui count_user_text_turns` ✅；`npm run build`（tsc+vite）✅
- **手工：** GUI 真机手测上表 1–7（待用户/本地跑 Hermes GUI）
- **测试结论：** [x] 自动化通过 · [ ] 手工待验收

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☐ | |
| 开箱即用未破坏 | ☐ | |
| 本地优先未破坏 | ☐ | |
| 测试通过 | ☐ | |
| 记录完整 | ☐ | |
| 产品+架构两视角齐全 | ☐ | |
| 非修修补补 | ☐ | |
| 代码卫生 | ☐ | |

- **验收人：**
- **验收日期：**
- **结论：** ☐ 通过 · ☐ 驳回
- **遗留项：**

---

## 5. 附注

server/Flutter 对称接线跟踪：发版前 P1。
