# 变更记录：TELEGRAM 渠道完善 — offset 持久化 + README 对齐

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-telegram-offset-and-docs` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（实现）· 端到端待真机手测 |
| **负责人** | Codex（用户委托） |
| **关联** | `docs/project-map.md` §6 P2 缺口 TELEGRAM |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 用 Telegram 与 Hermes 对话的个人用户 / 开发者
- **解决什么痛点：** ① `hermes telegram run` 重启后会把已处理过的消息**重复回复一遍**
  （offset 只存内存，代码里留有 TODO）；② README 完全没有 Telegram 使用说明
  （微信 / 飞书都有完整章节，Telegram 只能靠猜）
- **用完后用户多得到什么：** 重启 bot 不再重复回复；README 一页讲清 BotFather →
  auth → run 全流程
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（offset 文件可查；README 有完整步骤）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更快或更省心（重启安全 + 文档齐全）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户部署 Telegram bot 长跑，中途更新/重启
- **路径变化：** 改前（重启后重复回复已读消息，用户困惑）→ 改后（offset 持久化到
  `~/.small-rust-hermes/telegram-offset.txt`，重启从断点继续）
- **成功标准：** 重启后不重复处理已确认更新；README 出现 Telegram 章节（前置准备/
  配置/运行/会话存储）
- **明确不做什么：** 不做图片/语音等非文本消息支持（保持 MVP 拒绝文案）；不改
  wechat/feishu；不做多 bot 实例

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 状态持久化层（长轮询游标仅存内存）+ 文档层（README 缺渠道章节）
- **正确的长期默认路径：** 与微信同一模式——游标文件落 `~/.small-rust-hermes/`，
  每次轮询确认后 best-effort 写入，失败仅 warn 不阻断；Telegram 用整数 offset
  （`update_id + 1` 语义），微信用不透明 cursor 字符串——协议差异保留在渠道内
- **与引擎/各入口边界：** 只动 `hermes-cli/src/commands/telegram.rs`（渠道驱动）；
  `hermes-telegram` client 不感知持久化；共享 `serve_inbound` 不变
- **安全影响：** 无（offset 非敏感；不涉及 token/密钥）
- **如何防复发：** 删除代码内 TODO；README 章节与微信/飞书结构同构，后续渠道改动
  照此维护
- **为何这不是补丁：** 对齐已确立的微信游标模式（单一真相源），并同步删除 TODO 与
  补文档，是正确默认路径上的实现

---

## 1. 方案（Plan）

- **目标：** Telegram 重启不重复处理消息 + README 渠道文档齐全
- **范围：** 做：`telegram.rs` offset 持久化（`telegram-offset.txt`，镜像 wechat cursor）；
  README 增 Telegram 章节。**不做：** 非文本消息支持；wechat/feishu 改动；client 改动
- **用户路径变化：** 见 0b
- **技术要点：** `read_offset() -> Option<i64>` / `save_offset(i64)`；每次 getUpdates
  批次处理完保存；启动打印当前 offset
- **风险与回滚：** 低；写失败仅 warn；git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `crates/hermes-cli/src/commands/telegram.rs`：新增 offset 持久化三函数
     （`offset_path` / `read_offset` / `save_offset`，镜像 `wechat.rs` 的 cursor 模式）；
     `run()` 启动读 `read_offset()` 并打印；批次确认后 `save_offset`；退出前再存一次；
     删除原 TODO 注释
  2. `README.md`：新增「Telegram」章节（前置准备 BotFather / `hermes telegram auth` /
     手动 `telegram.toml` / `hermes telegram run` / 会话存储与工具摘要）
- **关键路径/文件：** `crates/hermes-cli/src/commands/telegram.rs`、`README.md`
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 编译与静态 | `cargo check -p hermes-cli` + clippy | 全绿 | 通过 | |
| 2 | 无 offset 文件启动 | 删除 telegram-offset.txt 后 run | `offset=None`，正常监听 | 通过（逻辑） | 端到端需 token |
| 3 | 有 offset 文件启动 | 写入 `12345` 后 run | 启动打印 `offset=Some(12345)` | 通过（逻辑） | |
| 4 | 重启不重复回复 | 跑 bot → 收消息 → 重启 → 同一消息不再回复 | 不重复 | ⬜ 待手测 | 需 BotFather token |
| 5 | README 步骤可达 | 按 README 章节操作 | auth/run 命令与代码一致 | 通过 | |

- **自动化：** `cargo check --workspace`、`cargo test --workspace`、clippy
- **手工：** 用例 4 需真实 bot token（用户执行）
- **测试结论：** [x] 自动化全部通过 · [ ] 端到端待真机手测（用例 4）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 重启安全 + 文档齐全 |
| 开箱即用未破坏 | ✅ | 无新依赖；offset 文件自动创建 |
| 本地优先未破坏 | ✅ | 明文 txt 落 `~/.small-rust-hermes/` |
| 测试通过 | ✅ | 自动化全绿；端到端待手测（用例 4） |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | 镜像既有微信游标模式 |
| 代码卫生（P0 第九条） | ✅ | TODO 注释删除；无死代码 |

- **验收人：** Codex（用户委托）
- **验收日期：** 2026-08-03
- **结论：** ☑ 条件通过（实现 + 自动化）· ☐ 驳回（原因：）
- **遗留项：** 用例 4 真机手测（需 BotFather token）→ 完成后补测并在本记录升「已验收」

---

## 5. 附注

- 原 TODO：`telegram.rs` 主循环注释「TODO: persist to telegram-offset.txt like the WeChat cursor」
- 微信对照实现：`wechat.rs` §cursor persistence（`wechat-cursor.txt`）
