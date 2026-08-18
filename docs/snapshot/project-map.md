# lebi-AI（乐彼AI）· 项目全景地图

| 字段 | 内容 |
|------|------|
| **版本** | 2026-08-18 |
| **种类** | **H** 快照（非权威） |
| **定位** | 架构与入口对照。冲突以 [`../../PRODUCT_PRINCIPLES.md`](../../PRODUCT_PRINCIPLES.md) **P0 v0.11** 为准。过期条目不得覆盖 P0。 |
| **关联** | [`../../AGENTS.md`](../../AGENTS.md)、[`../spec/zaiban-work.md`](../spec/zaiban-work.md) v2.0、[`../records/20260817-product-debt.md`](../records/20260817-product-debt.md) |

---

## 1. 一句话

**乐彼AI（lebi-AI）= 本地工作搭子 AI。** Slogan：**越用越像你的手感。**  
四环：Do × Continuity × Care × Evolve。定义卡见 **P0 v0.11**。  
用户默认路径 = **桌面 GUI**；CLI = 引擎装配入口；手机 / IM 已接线、非默认交付。

---

## 2. 用户主路径

```
下载桌面 GUI
  → 试用期内配置 API Key
  → 对话
  → 跨次还欠的交差进「在办」（必有期限）
  → 会话结束 → full reflection 提炼 skill / memory / conflict 候选
  → 用户确认 / 拒绝 → 明文 Markdown 落盘
  → 自设星期可蒸「回顾」（周报底稿）
  → hermes distill 收敛近似记忆（可选 --apply / --llm-merge）
```

进化候选**默认不自动写入**。micro-reflection 轮间异步，不阻塞输入。

---

## 3. 代码地图（18 crate + Flutter）

| 路径 | 职责 |
|------|------|
| `crates/hermes-core` | Session / LlmProvider / ToolHost / 压缩 / 授权 / 搭子协议 |
| `crates/hermes-llm` | Anthropic + OpenAI 兼容（默认 DeepSeek） |
| `crates/hermes-turn` | 回合引擎：工具循环、权限 confirm |
| `crates/hermes-tools` | 内置工具（文件 / shell / web / 记忆 / 技能 / 在办…） |
| `crates/hermes-mcp` | MCP 客户端 |
| `crates/hermes-store` | JSONL 会话、frontmatter |
| `crates/hermes-skills` | 技能域 + 内置三件套 |
| `crates/hermes-memory` | 记忆宫殿、supersedes、distill |
| `crates/hermes-reflect` | full + micro reflection、收件箱 |
| `crates/hermes-commitments` | **在办** `commitments.json` + **回顾** `reviews/` |
| `crates/hermes-sources` | 工作材料 Word/PDF（桌面） |
| `crates/hermes-cli` | 引擎装配 / 调试入口 |
| `crates/hermes-gui` | 桌面 GUI（Tauri 2，`ui/dist`）· 用户默认路径 |
| `crates/hermes-server` | Flutter 后端：GUI **子集** + WS（非 1:1） |
| `crates/hermes-channel` | IM 共享驱动 + GUI/server prompt 装配 |
| `crates/hermes-weixin/feishu/telegram` | IM 协议差异 |
| `clients/flutter` | 三端；已接线、非默认交付 |
| `~/.lebi-ai/` | 明文数据根（`LEBI_DATA_DIR` 可覆盖） |

### 四层对象不要混

| 层 | 落点 | 寿命 |
|----|------|------|
| 当次拆步 | `todo_write` | 本会话 |
| 在办 | `commitments.json` | 跨会话，必有期限 |
| 回顾 | `reviews/*.md` | 按区间一份 |
| 手感规则 | `memories/` | 长期，一类事一条 |

---

## 4. 能力矩阵

| 能力 | CLI | GUI | Server/Flutter | IM |
|------|-----|-----|----------------|----|
| 对话 + 流式 | ✅ | ✅ | ✅ WS + ticket | ✅ |
| 全量工具 | chat 白名单 / run 全量 | ✅ | ✅ 同引擎 | ❌ 只读白名单 |
| 工具确认 UI | ✅ | ✅ | ✅ | ❌ fail-closed |
| 记忆 / 技能管理 | CLI | ✅ | REST 部分；Flutter 有面板 | ❌ |
| 反思 / 收件箱 | ✅ | ✅ | REST + Flutter Evolve 已调 | ❌ |
| **在办 / 回顾** | 工具层有 | ✅ 抽屉（v2 工程收口） | ❌ 规格禁止（server 不写） | ❌ 规格禁止 |
| 微信连接 | CLI | ✅ 扫码只看 | ❌ | 本体 |
| Onboarding | `init` | ✅ | ❌ | — |
| 授权门禁 | — | ✅ 试用 3 天 | 无独立 UI | — |
| 发送方 allowlist | — | — | — | ✅ 强制 |

在办 **仅桌面 GUI** 是已拍板（规格 §1.15），不是漏做。Flutter Evolve 已调 inbox。授权只锁桌面。

---

## 5. 今天的线（2026-08-17）

| 台账 | 内容 | 状态 |
|------|------|------|
| `20260817-product-debt` | 收口产品债：冻结 v2 · Cue/空态/已蒸过 · 刷新本图 | 工程收口 · 待目视 |
| `20260817-zaiban-work-unify` | 无期限不成债 · 周报底稿 · 改删常驻 | 工程通过 · 待目视 |
| `20260817-review-ledger` | 回顾账留在页上 · 目录/读账拆开 | 工程通过 · 待目视 |

8/14 一批「工程通过 · 待产品确认」（第一性原理重建、记忆蒸馏、打开/搜索诚实、Office 真排版等）**代码已在树上**。代理人不代签目视。完整索引：[`../records/README.md`](../records/README.md)。

---

## 6. 仍开放（诚实）

| ID | 主题 | 说明 |
|----|------|------|
| **ZAIBAN-EYE** | 在办 v2 用户走查 | 记下要日子、过了换期、改删不用悬停、回顾不见 py |
| **AUG14-EYE** | 8/14 工程项目视 | 不代签。清单在 records 索引 |
| **REFLECT-HAND** | 会话结束蒸馏真机 | 实现已久，手测仍开放 |
| **LICENSE-SCOPE** | 授权只锁 GUI | 已拍。CLI / server / IM 不验签 |
| **MOBILE-E2E** | 语音 / 推送真机 | 需 APNs/FCM |
| **CLOUD-SYNC** | 多设备同步 | 本期不做 |

---

## 7. 协作

1. 先读 **P0 v0.11 → P1 v0.6 → AGENTS**
2. 有用户影响 → `docs/records/YYYYMMDD-slug.md`
3. 改 skill/工具先归入 ①引擎 / ②bundled / ③user / ④project
4. server ≠ GUI 1:1；问是否落在本表能力矩阵

*过期以 P0 与代码为准。*
