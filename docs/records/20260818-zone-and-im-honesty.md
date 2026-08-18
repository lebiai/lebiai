# 变更记录：记忆分区单一真相 · IM 不再假装能记

| 字段 | 内容 |
|------|------|
| **编号** | `20260818-zone-and-im-honesty` |
| **日期** | 2026-08-18 |
| **状态** | **工程通过** · 待目视（微信记住一句话） |
| **型** | 工程 |
| **关联** | 全量审计「下一步」：P0 分区 + IM 提示 |

---

## 0-fp. 第一性原理

- **拒绝的类比：** 不是再加一层「兼容旧分区」的映射表当产品；不是把 IM 做成缩小版桌面。
- **拆出的真：**
  1. 一类事只能落在一格。格的名字必须全仓同一套，否则检索与蒸馏会错过。
  2. 磁盘上已有 `core` / `episode` 文件，读的时候必须当现行格，不能假装看不见。
  3. IM 没有 `memory_save` / `commitment_save`。系统提示里写这两个名字 = 教模型撒谎。
- **如何从真推出：** 分区只在 `companion::zones` 立法；读写都走 `normalize`。IM 用只读身份协议，不含 Evolve / 在办写工具。

## 0. 用户价值

同类偏好不再拆成两格；微信里说「记住」时，它会说明要回桌面，而不是谎称已写下。

## 0b. 产品经理

- **场景：** 桌面对话沉淀偏好；微信里随口说记住。
- **怎么走完：** 桌面照旧批准候选；微信被要求记住时，一句话说回电脑上的乐彼。
- **看起来：** 用户看不见分区名。失败时不说「已保存」。
- **不做什么：** 不扩 Flutter/IM 写路径；不改授权范围；不重写 slot 关键词表。

## 0c. 架构师

- **根因：** 四套 zone 词同时教模型；IM 先拼完整协议再补「不能写」。
- **默认路径：** `normalize` 读旧写新；bundled `memory-palace` 升 0.2 并启动覆盖；`PromptKind::Im` 只用只读协议。
- **为何不是补丁：** 删旧词，不是再加 if。

## 1. 方案

- **做：** 分区归一；IM 去 durable 写条款；palace 技能重装；清工具/反思/distill 旧词。
- **不做：** 授权跨入口；待审队列合并；bash SSRF；GUI 弹窗。

## 2. 实施

- `companion::zones::normalize`：`core`→preferences，`episode`/`project:*`→work。读写与检索都走它。
- 删 `palace.rs` 的 `ZONE_CORE` / `ZONE_EPISODE`。
- `memory-palace` 升 0.2.0：只教四格；没有 `memory_save` 工具时不准声称已记。启动覆盖旧副本。
- IM 系统提示改用 `companion_protocol_readonly()`（无 Evolve / `memory_save` / `commitment_save`）。回归测试按大小写匹配，不再假绿。
- distill 保护 preferences（含旧 `core`）。`MemoryFrontmatter::new` 写盘即归一。

**偏离方案处：** 无。顺手给 `always_active_skill_body_is_inlined` 补回漏掉的 `#[test]`。

## 3. 测试

| # | 用例 | 结果 |
|---|------|------|
| 1 | `zones::normalize` 别名 | 通过 |
| 2 | IM 提示不含 memory_save / commitment_save / Evolve | 通过 |
| 3 | palace 读 `preferences` 能命中磁盘上的 `core` | 通过 |
| 4 | 旧 palace 技能启动升级到 0.2.0 | 通过 |
| 5 | distill 保护 core/preferences 簇 | 通过 |
| 6 | clippy `-D warnings`（触及 crate） | 通过 |

- **自动化：** `cargo test -p hermes-core -p hermes-memory -p hermes-channel -p hermes-skills -p hermes-reflect -p hermes-tools --lib` 全绿。
- **手工：** 微信里说「记住我喜欢短句」应答回桌面，不得说已保存。需真机。

## 4. 验收

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | 是 | 一格一词；IM 不撒谎 |
| 开箱即用未破坏 | 是 | 旧文件只读时折叠，不要求搬家 |
| 本地优先未破坏 | 是 | |
| 测试通过 | 是 | 触及 crate 单测 + clippy |
| 记录完整 | 是 | |
| 产品+架构两视角齐全 | 是 | |
| 非修修补补 | 是 | 单一 normalize，不是再叠一套 |
| 代码卫生 | 是 | 删 ZONE_CORE / ZONE_EPISODE |
| 操作与视觉 | 工程齐 | IM 真机未点 |
| 第一性原理 | 是 | |

- **遗留项：** IM 真机一句（不代签）。授权范围、待审双队列、GUI 弹窗按审计下一刀。
