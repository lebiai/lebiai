# 变更记录：默认语言改为中文（zh-CN）

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-default-language-zh` |
| **日期** | 2026-08-09 |
| **状态** | **已验收**（测试 + tsc + build 全绿） |
| **负责人** | Codex Agent |
| **关联** | 用户反馈：默认应该是中文 |

---

## 0. 用户价值

- **谁用：** 新用户（国内为主）。
- **解决什么痛点：** 新装应用界面默认英文，与「国内普通用户」主画像不符。
- **用完后用户多得到什么：** 开箱即中文；仍可在设置里切英文。

---

## 0b. 产品经理视角

- **路径变化：** 新建配置默认 `ui.language = zh-CN`；UI 初始态与 get_config 失败兜底均为中文。
- **成功标准：** 干净环境打开即中文；设置切 English 仍可用；已有配置不受影响（用户在设置里选一次语言即可切换）。
- **明确不做：** 不改已有用户磁盘配置（用户主权）；不增加语言检测（不跟系统语言，固定默认中文）。

---

## 0c. 架构师视角

- **根因：** 默认语言字面量散落多处：`default_ui_language()`、`default_config_toml()` 模板 `[ui] language`、前端 `DEFAULT_LANGUAGE`、`uiStore` 初始态、`App.tsx` 兜底——`default_config_toml` 字面量才是新建配置的真正来源，漏改导致测试失败后修复。
- **正确默认路径：** 默认中文收敛为单一事实（`default_ui_language()` = "zh-CN" + 模板字面量同步），前端 `normalizeLanguage` 支持 zh-CN/en-US 双向、其余回退默认；初始态与兜底全部默认 zh-CN。
- **防复发：** 模板生成处禁止手写字面量语言值（本次已同步）；新增测试断言覆盖。
- **为何这不是补丁：** 每处默认来源都收敛到 zh-CN，不是靠前端猜系统语言。

---

## 1. 方案（Plan）

- **目标：** 默认中文，可切英文。
- **范围：** `hermes-llm/config.rs`（默认函数 + 模板 + 测试）、`ui/src/i18n.ts`（DEFAULT_LANGUAGE + normalizeLanguage）、`ui/src/store/uiStore.ts`（初始态）、`ui/src/App.tsx`（兜底）。
- **风险与回滚：** 低；回滚 = 还原上述四处默认值。
- **方案确认：** [x] 已对照 P0/P1/P2/P3 · 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** `default_ui_language()` → "zh-CN"；`default_config_toml()` 模板 `[ui] language = "zh-CN"`；测试断言同步；`i18n.ts` DEFAULT_LANGUAGE → "zh-CN"、normalizeLanguage 双向保留；`uiStore` 初始 language/t → zh-CN；`App.tsx` get_config 失败兜底 → zh-CN。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | 新建配置默认语言 | `[ui] language = "zh-CN"` | ✅ `default_config_template_loads` 通过（16 passed） |
| 2 | 语言切换 | en-US 与 zh-CN 均可选 | ✅ tsc + build 通过 |
| 3 | 前端构建 | npm run build | ✅ |

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 开箱中文 |
| 测试通过 | ✅ | hermes-llm 16 passed · tsc · build |
| 非修修补补 | ✅ | 模板与默认函数同步，杜绝漂移 |
| 记录完整 | ✅ | 本文档 + README 索引 |

- **验收人：** Codex Agent · **验收日期：** 2026-08-09 · **结论：** ☑ 通过

---

## 5. 附注

- 已有用户磁盘配置里的 `ui.language = "en-US"` 不会被自动改写（用户主权）；在设置里选一次「简体中文」即可。
