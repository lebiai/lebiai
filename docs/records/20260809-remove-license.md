# 变更记录：删除全部执照（LICENSE）内容

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-remove-license` |
| **日期** | 2026-08-09 |
| **状态** | **已验收** |
| **负责人** | Codex Agent |
| **关联** | 用户指示：删除掉所有执照的内容 |

---

## 0. 用户价值

- **谁用：** 打开项目页的普通用户 / 项目所有者。
- **解决什么痛点：** 产品介绍页面出现「许可证 / 商业授权 / PolyForm」等法律条款，与「站在用户角度」的产品介绍冲突；所有者决定移除全部执照相关内容。
- **用完后用户多得到什么：** README 与项目页无任何执照/许可条款，界面更纯粹。

---

## 0b. 产品经理视角

- **路径变化：** 删除 README「许可证」章节（含商业授权联系方式）、根目录 LICENSE 文件，以及文档中所有 LICENSE / PolyForm / 商业授权引用。
- **成功标准：** 用户可见文档无任何执照内容；Cargo 元数据可正常解析。
- **明确不做：** 不改历史台账（`docs/records/` 属历史事实记录）；不动技能元数据里的 `license: Option<String>` 功能字段（技能包自带的许可证元数据，非项目执照，删除会破坏技能解析/展示）。

---

## 0c. 架构师视角

- **根因：** 项目以 PolyForm 非商业许可开源，该许可信息散布于 LICENSE 文件、README、P0/P1/P2 文档与 Cargo 元数据。
- **正确默认路径：** 一处根因（所有权决定）+ 全量同步清理：删除 LICENSE 文件与 README 章节；删除根 `Cargo.toml` license 字段及全部 crate 的 `license.workspace = true`（避免失效引用）；清理 AGENTS / DEVELOPMENT_RULES / PRODUCT_PRINCIPLES / docs/README / docs/install / docs/project-map 中对 LICENSE 的引用；保留技能元数据 `license` 字段（功能，与项目执照无关）。
- **防复发：** 以后新增文档不得引入执照/许可条款；引入外部资源（技能包等）时其元数据 license 字段属功能字段，不视为执照内容。
- **为何这不是补丁：** 执照信息的多处分散引用一次性按单一事实源原则清理干净，无残留死链接。

---

## 1. 方案（Plan）

- **目标：** 移除全部执照相关内容（LICENSE 文件、README 章节、Cargo 字段、文档引用）。
- **范围：** 做：上述删除/清理。不做：历史台账回改、技能元数据 license 字段、第三方依赖 package-lock 中的元数据。
- **风险与回滚：** LICENSE 文件已移入废纸篓可还原；git 历史完整保留原文件。
- **方案确认：** [x] 已对照 P0/P1/P2/P3 · 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. 删除根目录 `LICENSE`（移入废纸篓）。
  2. `README.md`：删除「许可证」章节与商业授权联系方式。
  3. 根 `Cargo.toml`：删除 `license = "LicenseRef-PolyForm-Noncommercial-1.0.0"`；16 个 crate 的 `Cargo.toml`：删除 `license.workspace = true`。
  4. `AGENTS.md` / `DEVELOPMENT_RULES.md` / `PRODUCT_PRINCIPLES.md`（两处）/ `docs/README.md` / `docs/install.md` / `docs/project-map.md`：清理 LICENSE / PolyForm / 商业授权引用。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | 执照内容扫描 | 用户可见文档无 polyform / 商业授权 / 商业许可 残留 | ✅ rg 全库扫描通过（历史台账除外） |
| 2 | Cargo 元数据 | workspace 无失效 license 引用 | ✅ `cargo metadata --no-deps` OK |
| 3 | 技能功能 | `license: Option<String>` 字段保留 | ✅ 未改动 |
| 4 | 根目录 md | 仍为 4 份权威文档（无 LICENSE） | ✅ |

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 项目页无任何执照内容 |
| 文档诚实 | ✅ | 无失效引用/死链接 |
| 非修修补补 | ✅ | 单一事实源原则一次性清理 |
| 代码卫生 | ✅ | 无 license 元数据残留 |
| 记录完整 | ✅ | 本文档 + README 索引 |

- **验收人：** Codex Agent
- **验收日期：** 2026-08-09
- **结论：** ☑ 通过
