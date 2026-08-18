# 变更记录：在办 v1 实现

| 字段 | 内容 |
|------|------|
| **编号** | `20260814-zaiban-impl` |
| **日期** | 2026-08-14 |
| **状态** | **实施中** · 缺口已补 · 待目视走查 |
| **型** | 工程 |
| **关联** | [`20260814-zaiban-commitments`](./20260814-zaiban-commitments.md) · [`../spec/zaiban.md`](../spec/zaiban.md) |

---

## 0-fp. 第一性原理

- **拒绝的类比：** 见规格台账。实现不把在办塞进记忆 / `todo_write` / 进化收件箱。
- **拆出的真：** 缺第四类明文对象；对话与侧栏必须读同一份文件。
- **如何推出：** `hermes-commitments` 一份 `commitments.json` + 引擎工具 + GUI 侧栏 + 离开余债建议 + 系统提示按需注入标题。

## 0. 用户价值

对话里能记下、侧栏能建/勾/收下建议、下次问「今天干什么」能看见开着的债。

## 0b / 0c

服从 [`../spec/zaiban.md`](../spec/zaiban.md)。默认路径：`data_root()/commitments.json`，不依赖 Vite。

---

## 1. 方案

规格已冻结。本文是实现台账。

## 2. 实施

- 新 crate `hermes-commitments`：类型、近义门禁、原子写、离开扫描 `scan_residue`。
- 工具 `commitment_list` / `save` / `close` / `drop`（IM 白名单不含）。
- `companion_protocol` 增 Open work 小节；GUI 系统提示按需注入标题（≤7），挤/过期才加 nudge。
- GUI 侧栏「在办」块：空态、手建、近义问合并、待收下、点标题预填「开始做：」。
- 离开会话：与蒸馏独立，安静写「待收下」，7 天过期；事件 `hermes://zaiban-changed`。
- 对话 `commitment_*` 成功后发 `hermes://zaiban-changed` + `zaibanUpdated` 流事件：侧栏立刻刷新，对话里可点「已记下」。
- 侧栏展开：做完 / 丢掉 / 改标题 / 等谁 / 拆开 / 回源会话；开始做先问「完成算什么」。
- 对话顶一条稳定递回（待收下 / 近义并 / 过期 / 空会话开着的债），不靠模型碰巧开口。
- 离开后 LLM 保守扫语义近义对，写入 `semantic_pair`。
- `todo_write` 未改。Server/Flutter 无专用 UI。

**偏离：** 无。

## 3. 测试

| # | 用例 | 结果 |
|---|------|------|
| 1 | 近义 / 折叠 / 已完成不并 / 建议过期 | `hermes-commitments` 单测通过 |
| 2 | 问「今天干什么」才注入标题 | `companion_context` 通过 |
| 3 | 工具 list/handles 对齐 | `hermes-tools` 回归通过 |
| 4 | GUI/CLI/server 编译 | 通过 |
| 5 | `tsc --noEmit` | 通过 |

- clippy：本功能 crate 已清；workspace 另有既有告警（如 `url_safety` MSRV）未在本变更扩大。
- GUI 目视走查：待本机 `scripts/run-gui.sh`。

## 4. 验收

规格 §11 用户语言清单待目视。工程未勾「已验收」。

## 5. 附注

数据文件：`~/.lebi-ai/commitments.json`。
