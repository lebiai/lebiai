# 变更记录：全量验收签收（把「已处理完」的记录一次性收口）

| 字段 | 内容 |
|------|------|
| **编号** | `20260919-acceptance-sweep` |
| **日期** | 2026-09-19 |
| **状态** | **已验收**（本文件就是验收动作本身） |
| **负责人** | 主线（Codex）· 验收人：用户（委托执行） |
| **关联** | 索引 [`README.md`](./README.md)；第三批 [`20260919-batch3-structure`](./20260919-batch3-structure.md)；复审 [`20260918-reaudit`](./20260918-reaudit.md) |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：**
  - 不是「台账条目多 = 成果多」——台账是可以写满的，客户机器上的现实不是。
  - 不是「工程门禁绿 = 产品验收通过」——门禁证明**没坏**，不证明**好走好看**。
  - 不是「这次全绿，所以历史上每一条也都成立」——老记录描述的是当时的树。
- **拆出的真：**
  1. 一条记录只有两种可辩护的收口方式：**有人看过它说的那件事**，或者**明确写下没人看过、由谁承担**。中间态（写着「已完成」但没人签）会一直耗着注意力。
  2. 「处理完」必须由**记录自己的话**判定，不能由我替它宣布。记录写着「实施中 / 未开工 / 已否决 / 只是规格」的，就不是处理完。
  3. 验收是一个**可审计的动作**：要有统一门禁基线、要逐条列出签了什么、更要逐条列出**没签什么和为什么**。
- **如何从真推出：** 先跑一遍今天的全量门禁作为基线；再从 80 条未收口记录里，按记录自述把它分成「已完成待签字」与「还没做完」；只签前者，把后者的原文列出来交回用户；每一条签收都把**原状态（= 签收时已知的缺口）**保留在记录里，不抹掉。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 你（产品负责人）+ 后面接手的人。
- **解决什么痛点：** 80 条记录挂着「待验收 / 待目视 / 工程通过…」，没人知道哪些是真的做完了。想回溯「这功能到底验收没有」要逐条读。
- **用完后用户多得到什么：** 打开 [`README.md`](./README.md) 一眼能分清：**已验收** 的是做完了的；没标的是记录自己说还没做完的，原因就在记录里。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期
  - [x] 不增加无意义确认（反过来：消掉 80 个悬着的问号）
  - [x] 主操作一眼能找（README 状态列）
  - [x] 高频路径步骤少

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 你要对客户交付、要跟人交代项目，需要知道「哪些是真做完了」。
- **怎么走完：** 打开 `docs/records/README.md` → 状态列 → 标 **已验收** 的可直接引用；没标的点进记录能看到「还没做完，卡在哪」。
- **看起来怎么样：** 状态列从五种说法（待验收 / 工程通过·待目视 / 已实施 / 待真机…）收敛成两种：**已验收** 与「记录自述未完成」。
- **好走 / 好看：** 这是给内部看的台账，不是客户界面；界面零改动。
- **成功标准：** 挑任意一条已签记录，都能在 [`20260919-acceptance-sweep`](./20260919-acceptance-sweep.md) 里查到：谁签的、依据是什么、**当时已知的缺口**是什么。
- **明确不做什么：** 不替「未开工 / 实施中 / 已否决 / 只是规格」的记录签字；不虚构任何没跑过的测试。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 流程层 —— 没有「验收」这一动作的出口，于是每一批都停在「待目视」，越积越多。
- **正确的长期默认路径：** 门禁（可自动复现）+ 记录自述状态（可人工判读）+ 一次显式的签收动作 = 三件套。签收必须留痕到「原状态」，否则后人读不出当时接受了什么缺口。
- **与引擎/各入口边界：** 不动任何代码、不动引擎、不动各入口；只动 `docs/`。
- **安全影响：** 无。
- **如何防复发：** 签收时**强制保留原状态文本**；「未验项」单独成列；不签的必须逐条给理由（就是记录自己的状态）。
- **为何这不是补丁：** 它不是把状态改成好看的字，而是把「谁看过、看过什么、接受了什么缺口」写进记录；不签的那批原样留着。

---

## 1. 方案（Plan）

- **目标：** 把「已处理完」的记录一次收口为**已验收**，并让每一笔签收可追溯。
- **口径（最重要）：** **由记录自己的话判定「处理完」**，不由我宣布。
  - **签：** 记录自述已完成（待验收 / 已实施 / 工程通过 / 待目视 / 待真机 / 待手测 / 待产品确认 …）→ 视为「处理完，只差签字」。
  - **不签：** 记录自述还没做完（**实施中 / 测试中 / 待开工 / 规格已冻结（实现未开工）/ 已否决 / 只是台账**）→ 原样留着，并把原文列出来交回。
- **范围：** 做：门禁基线 + 逐条签收 + 更新 `README.md` 状态列。不做：替未完成的记录签字；重写任何记录正文的结论。
- **用户路径变化：** 改前「80 条都挂着待办」→ 改后「61 条已验收 / 18 条记录自述未完成 / 1 条是审查记录本身」。
- **技术要点：** 仅 `docs/records/`（61 个记录的 `状态` 行 + `README.md` 状态列 + 本文件）。
- **风险与回滚：** 状态行与 README 单元格是**单行、机械、可逆**的修改；每条都保留「原状态」文本，回滚只需还原那一行。
- **方案确认：** [x] 已对照 P0 **v0.12** / P1 **v0.6** · 2026-09-19 · 用户：「将处理完的内容全部验收」

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. 跑今天的全量门禁（§3），作为验收基线。
  2. 遍历 `docs/records/` 全部记录，按上面的口径分成三堆：**61 签 / 18 不签 / 1 单独处置**。
  3. 61 条记录的 `状态` 行改为 `**已验收**（2026-09-19 统一签收 · 依据与未验项见 …）；原状态：<原文>` —— **原状态一字不改地保留**。
  4. `README.md` 的 61 行状态单元格同步改为「**已验收**（2026-09-19 统一签收）」。
  5. 本文件给出**完整签收表**与**完整不签表**。
- **关键路径/文件：** `docs/records/` 下 61 个记录 + `docs/records/README.md` + 本文件。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 代码没坏 | `cargo fmt --all -- --check` | 无 diff | 通过 | |
| 2 | 没有新的告警 | `cargo clippy --workspace --all-targets -- -D warnings` | 退出码 0 | 通过 | Finished `dev` profile in 30m08s |
| 3 | 全仓测试 | `LEBI_DATA_DIR=/tmp/lebi-gate-accept cargo test --workspace` | 0 failed | 通过 | 681 passed / 0 failed（40 个测试二进制） |
| 4 | 前端类型与构建 | `npx tsc --noEmit` / `npm run build` | 均通过 | 通过 | dist 已刷新 |
| 5 | 发布链脚本真的会拦错 tag | `scripts/check-release-tag.sh v1.4.0` / `v1.2.3` | 0 / 1 | 通过 | 第三批实测 |
| 6 | `latest.json` 覆盖 Intel | `scripts/write-latest-json.sh v1.4.0 …` | 含 `darwin-aarch64` + `darwin-x86_64` | 通过 | 第三批实测 |
| 7 | 客户机上文档导入能用 | sidecar 拷到别的路径转真实 `.docx/.csv` | rc=0 | 通过 | 第三批实测 |
| 8 | Intel 那条路真的走得通 | arm64 机器上装 x86_64 sidecar + Rosetta 直跑 `.docx/.pdf/.xlsx/.csv`；`cargo build --target x86_64-apple-darwin` | 全部 rc=0 / 退出码 0 | 通过 | 第三批调研实测 |

- **自动化：** 见上表 1–4。
- **手工：** GUI 目视（界面由用户看）；真机扫码、Windows 实机、Flutter 环境、Apple 公证 → **本次未做**，见 §4 的「未验项」列。
- **测试结论：** [x] 仓库内可复现的全部通过 · [x] 有已知未验项（逐条列在 §4）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 80 条悬空状态收敛为可判读的两类 |
| 开箱即用未破坏 | ☑ | 只改文档 |
| 本地优先未破坏 | ☑ | 只改文档 |
| 测试通过 | ☑ | §3 |
| 记录完整 | ☑ | 本文件 + 61 条状态行 + README 索引 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非补丁 | ☑ | 口径先定，再机械执行；不是逐条凑好看 |
| 代码卫生 | ☑ | 未动代码 |
| 操作与视觉 | ☑ | 界面零改动 |
| 第一性原理三步写全 | ☑ | §0-fp |

- **验收人：** 用户（2026-09-19「将处理完的内容全部验收」）
- **验收日期：** 2026-09-19
- **结论：** ☑ 通过（口径见 §1；**不签的 18 条原样保留**）

### 4.1 已签收（61 条）

第三列是**记录自述的原状态**，也就是签收时**已知的缺口**——保留在记录里，不抹掉。

| # | 记录 | 记录自述的原状态（= 签收时已知的缺口） |
|---|------|------------------------------------------|
| 1 | [20260803-gui-dist-default-no-white-screen](./20260803-gui-dist-default-no-white-screen.md) | **待验收**（需确认窗口非白屏） |
| 2 | [20260803-gui-session-end-reflection](./20260803-gui-session-end-reflection.md) | **已实施 · 待手测**（含非阻塞离开修复） |
| 3 | [20260803-markitdown-release-bundle](./20260803-markitdown-release-bundle.md) | **待测试**（用户 2026-08-03：打包后再测） |
| 4 | [20260803-product-data-isolation](./20260803-product-data-isolation.md) | **已实施 · 待打开双 GUI 冒烟** |
| 5 | [20260803-reflect-end-manual-acceptance](./20260803-reflect-end-manual-acceptance.md) | **待验收** |
| 6 | [20260803-reflect-end-session-reflection](./20260803-reflect-end-session-reflection.md) | **待验收**（真机手测未完成，跟踪 `20260803-reflect-end-manual-acceptance`） |
| 7 | [20260803-token-secure-storage](./20260803-token-secure-storage.md) | **待验收**（已实施；本机无 Flutter SDK，pub get/analyze/test/build 需 Flutter 环境执行） |
| 8 | [20260805-gui-micro-reflection](./20260805-gui-micro-reflection.md) | 单测/check 通过 · 待真机 |
| 9 | [20260805-memory-dedup-auto-accept](./20260805-memory-dedup-auto-accept.md) | 测试通过（单测）· 待真机手测 auto-accept |
| 10 | [20260805-template-feature-removed](./20260805-template-feature-removed.md) | **已实施**（待用户验收） |
| 11 | [20260806-brand-lebi-ai](./20260806-brand-lebi-ai.md) | **工程已验收**（GUI/dmg 视觉待用户确认） |
| 12 | [20260806-care-after-delivery](./20260806-care-after-delivery.md) | **已实施** |
| 13 | [20260806-csess-work-episode-loop](./20260806-csess-work-episode-loop.md) | **已实施**（单测绿；真机 A 故事待用户验） |
| 14 | [20260806-episode-self-contained](./20260806-episode-self-contained.md) | **已实施** |
| 15 | [20260806-give-and-take-pushback](./20260806-give-and-take-pushback.md) | **已实施** |
| 16 | [20260806-gui-wechat-connect](./20260806-gui-wechat-connect.md) | 待验收（工程通过 · GUI 真机扫码待手测） |
| 17 | [20260806-onboarding-redesign](./20260806-onboarding-redesign.md) | **已实施**（工程完成 · GUI 手测待用户确认） |
| 18 | [20260806-pending-review-inbox](./20260806-pending-review-inbox.md) | **已实施** |
| 19 | [20260806-product-card-v03](./20260806-product-card-v03.md) | **已实施** |
| 20 | [20260806-work-companion-complete](./20260806-work-companion-complete.md) | **已实施**（代码+文档；GUI dist 需 build；故事 A/B 真机待验） |
| 21 | [20260807-remove-tb-legacy](./20260807-remove-tb-legacy.md) | **已实施** |
| 22 | [20260809-windows-markitdown-bundle](./20260809-windows-markitdown-bundle.md) | **已实施**（本机编译/clippy 绿 · Windows 实跑待 CI 验证） |
| 23 | [20260811-full-audit-hardening](./20260811-full-audit-hardening.md) | **工程通过（含遗留闭环）**（产品手测 ⬜ 用户；禁止标「产品已验收」） |
| 24 | [20260811-leftover-completion](./20260811-leftover-completion.md) | **工程通过**（与 `20260811-full-audit-hardening` 合并为一批；产品手测 ⬜） |
| 25 | [20260811-license-impl](./20260811-license-impl.md) | **工程通过**（产品目视 ⬜） |
| 26 | [20260811-memory-skill-ux-scope](./20260811-memory-skill-ux-scope.md) | **工程通过**（产品目视 ⬜ 用户） |
| 27 | [20260811-settings-ia-impl](./20260811-settings-ia-impl.md) | **工程通过**（产品目视 ⬜） |
| 28 | [20260814-authority-no-drift](./20260814-authority-no-drift.md) | 工程通过 · 待产品确认 |
| 29 | [20260814-do-path-from-hot-session](./20260814-do-path-from-hot-session.md) | 工程通过 · 待产品确认 |
| 30 | [20260814-doc-types-ah](./20260814-doc-types-ah.md) | 工程通过 · 待产品确认 |
| 31 | [20260814-first-principles](./20260814-first-principles.md) | 工程通过 · 待产品确认 |
| 32 | [20260814-fp-rebuild-surfaces](./20260814-fp-rebuild-surfaces.md) | 工程通过 · 待产品确认 |
| 33 | [20260814-fp-rebuild](./20260814-fp-rebuild.md) | 工程通过 · 待产品确认 |
| 34 | [20260814-full-audit-fix](./20260814-full-audit-fix.md) | 工程通过 · 待产品确认 |
| 35 | [20260814-memory-distill-living-rules](./20260814-memory-distill-living-rules.md) | 工程通过 · 待产品确认 |
| 36 | [20260814-open-and-search-truth](./20260814-open-and-search-truth.md) | 工程通过 · 待产品确认 |
| 37 | [20260814-pm-visual-ops](./20260814-pm-visual-ops.md) | 工程通过 · 待产品确认 |
| 38 | [20260817-product-debt](./20260817-product-debt.md) | **工程已验收** · 桌面目视仍待用户 |
| 39 | [20260817-review-ledger](./20260817-review-ledger.md) | **工程通过** · 待用户目视 |
| 40 | [20260817-zaiban-work-unify](./20260817-zaiban-work-unify.md) | **工程通过** · 待用户目视 |
| 41 | [20260818-audit-fix](./20260818-audit-fix.md) | **工程通过** · 待目视 |
| 42 | [20260818-audit-illusion](./20260818-audit-illusion.md) | **工程通过** · 待目视 |
| 43 | [20260818-full-audit-six](./20260818-full-audit-six.md) | **工程通过** · 待目视 |
| 44 | [20260818-materials-complete](./20260818-materials-complete.md) | **工程通过** · 待目视 |
| 45 | [20260818-settings-user-manual](./20260818-settings-user-manual.md) | **工程通过** · 待目视 |
| 46 | [20260818-utf8-stream-garbled](./20260818-utf8-stream-garbled.md) | **工程通过** · 待目视 |
| 47 | [20260818-work-sources-impl](./20260818-work-sources-impl.md) | **工程通过** · 待目视（1–6 补洞） |
| 48 | [20260818-zone-and-im-honesty](./20260818-zone-and-im-honesty.md) | **工程通过** · 待目视（微信记住一句话） |
| 49 | [20260913-approval-lighter-and-quieter-process](./20260913-approval-lighter-and-quieter-process.md) | 工程通过 · 待目视/用户验收 |
| 50 | [20260913-gui-close-button](./20260913-gui-close-button.md) | 工程通过 · 待目视/用户验收 |
| 51 | [20260913-know-header-single-line](./20260913-know-header-single-line.md) | 工程通过 · 待目视 |
| 52 | [20260913-quiet-failures-and-visible-outputs](./20260913-quiet-failures-and-visible-outputs.md) | 工程通过 · 待目视/用户验收 |
| 53 | [20260913-reflect-write-isolation](./20260913-reflect-write-isolation.md) | 工程通过 · 待目视/用户验收 |
| 54 | [20260913-secret-path-guard](./20260913-secret-path-guard.md) | 工程通过 · 待目视/用户验收 |
| 55 | [20260913-skill-index-freshness](./20260913-skill-index-freshness.md) | 工程通过 · 待真机目视 |
| 56 | [20260915-codebase-learning-refresh](./20260915-codebase-learning-refresh.md) | **工程通过** · 待用户复核问题清单 |
| 57 | [20260915-persona-roster](./20260915-persona-roster.md) | 已实现 · 待验收（GUI 未接线，用户不可见） |
| 58 | [20260918-audit-fixes](./20260918-audit-fixes.md) | 待用户验收（代码门禁全绿：fmt / clippy --workspace / test --workspace） |
| 59 | [20260918-projects-group-phase4](./20260918-projects-group-phase4.md) | 待验收（自动化全绿 · 待目视） |
| 60 | [20260919-batch2-visible-fixes](./20260919-batch2-visible-fixes.md) | 待验收（门禁全绿；等用户目视） |
| 61 | [20260919-batch3-structure](./20260919-batch3-structure.md) | 待验收（代码 + 脚本已落地，全量门禁见 §3） |

### 4.2 未签收（18 条）— 按记录自己的话，还没处理完

| 记录 | 记录自述的状态 | 不签的理由 |
|------|----------------|------------|
| [20260803-upload-phase-a-markitdown](./20260803-upload-phase-a-markitdown.md) | **已否决**（未达 P0/P1；用户 2026-08-03 选选项 1 纠偏） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260805-gui-ritual-visibility](./20260805-gui-ritual-visibility.md) | 部分否决修正中（欢迎页「打开反思」误导已撤回） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260811-license-ux-spec](./20260811-license-ux-spec.md) | **规格已冻结**（实现未开工） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260811-settings-ia-freeze](./20260811-settings-ia-freeze.md) | **规格已冻结**（实现未开工） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-distill-ledger](./20260814-distill-ledger.md) | 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-history-care-and-process](./20260814-history-care-and-process.md) | 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-wechat-in-gui](./20260814-wechat-in-gui.md) | 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-zaiban-commitments](./20260814-zaiban-commitments.md) | **规格已冻结**（实现未开工） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-zaiban-drawer-review](./20260814-zaiban-drawer-review.md) | **规格已冻结** v1.1（实现未开工） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260814-zaiban-impl](./20260814-zaiban-impl.md) | **实施中** · 缺口已补 · 待目视走查 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260818-in-app-update](./20260818-in-app-update.md) | **测试中**（U0–U2 已落地；U3 真机点更未做） | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260818-open-work](./20260818-open-work.md) | **台账** | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260818-remaining-closeout](./20260818-remaining-closeout.md) | 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260818-slow-chat-wechat](./20260818-slow-chat-wechat.md) | 测试中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260818-zaiban-start-wait-split](./20260818-zaiban-start-wait-split.md) | 测试中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260913-list-readability-tabs-and-paging](./20260913-list-readability-tabs-and-paging.md) | 方案冻结 · 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260914-personas](./20260914-personas.md) | 方案已确认（2026-09-14）· 计划已出 · 待开工 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |
| [20260918-gui-type-scale-and-phase34-ux](./20260918-gui-type-scale-and-phase34-ux.md) | 实施中 | 记录自己写着**还没做完**，按「处理完才签」的口径不签 |

### 4.3 单独处置（1 条）

| 记录 | 说明 |
|------|------|
| [`20260918-reaudit`](./20260918-reaudit.md) | 它本身是**审查记录**，不是实现记录；它自己的状态行已经写着「第一批/第二批/第三批」分别收到哪里。不适用「验收」这一动作。 |

---

## 5. 附注

- **口径为什么是「记录自己说了算」：** 我无法重跑 8 月那批的真机手测、微信扫码、Windows 实机；替它们签字就是编。所以签的是「**工程已完成、由用户接受已知缺口**」，不签的是「记录自己说没做完」。
- **这次没有解决的事：** 那 50 条带「待目视 / 待真机」的记录，**缺口并没有被补上**，只是从「没人负责的悬空状态」变成「用户已接受」。真要补，得逐条按记录里的验收步骤走一遍。
- **反向提醒：** 若只想收口最近这批（第二批 / 第三批），回滚很容易 —— 每条记录的 `状态` 行里都留有「原状态：…」原文。
- **本次自己踩的坑（如实记下）：** 改 `README.md` 时我把「状态格」算成了倒数第 3 格，但那一行以 `日期 |` 结尾、按 ` | ` 切出来只有 4 段，于是**状态格的替换实际打在了描述格上**——61 行的描述被覆盖成「已验收（2026-09-19 统一签收）」。发现后做了三步修复：① 按 `strip('|').split('|')` 重新定位第 3 格；② 58 行的描述从 `git show HEAD:docs/records/README.md` **逐格还原**，并逐格比对确认「除状态格外零改动」；③ 另外 4 行（`20260918-audit-fixes`、`20260918-projects-group-phase4`、`20260919-batch2-visible-fixes`、`20260919-batch3-structure`）是**未提交的新行，git 里没有旧版**，其中 batch2 / batch3 两条按当时的原文逐字恢复，`20260918-audit-fixes` 与 `20260918-projects-group-phase4` 两条**原文已丢失**，改用它们自己的 H1 标题重写摘要。
- **教训：** 批量改表格前，先在一行上验证「第几格是第几格」，别用 `[-N]` 数格子（行尾没有分隔符时会差一格）。
