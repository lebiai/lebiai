# 变更记录：品牌定名 乐彼AI / lebi-AI（含图标与数据目录）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-brand-lebi-ai` |
| **日期** | 2026-08-06 |
| **状态** | **工程已验收**（GUI/dmg 视觉待用户确认） |
| **负责人** | Codex（用户拍板品牌名） |
| **关联** | 会话「品牌问题梳理」；`20260806-onboarding-redesign`（引导页同步）；`20260803-product-data-isolation`（数据隔离语义）；后续 `20260807-dock-name-lebi-ai`（Dock 悬浮名品牌化 · 二进制名 lebi-AI） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 拿到 dmg/exe 的终端客户（无技术背景）+ 开发者
- **解决什么痛点：** 应用还叫 Hermes、窗口标题是 Hermes、图标是 Tauri 默认橙标——客户第一眼不知道「这是什么、是不是为我而来」，品牌无辨识度
- **用完后用户多得到什么：** 桌面上出现一个「乐彼AI（lebi-AI）」图标，窗口、引导页、欢迎语、打包产物品牌一致；第一眼即知产品名
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（改名不动用户数据）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更省心（数据目录迁移一次自动完成）

---

## 0b. 产品经理视角

- **场景：** 用户安装/双击 app → 看到图标、窗口标题、首次引导页、侧栏品牌
- **路径变化：** 改前：Hermes + Tauri 默认图标 + 数据 `~/.small-rust-hermes`；改后：乐彼AI（中文态）/ lebi-AI（英文态）+ 自研图标（蓝→紫渐变 + L 标 + 星芒）+ 数据 `~/.lebi-ai`
- **成功标准：**
  1. dmg / 窗口 / 侧栏 / 引导页 / 欢迎语均显示 lebi-AI 或 乐彼AI（按语言态）
  2. 图标不再是 Tauri 默认图，macOS Dock 与 dmg 中可见新图标
  3. 首次启动旧数据自动迁移到 `~/.lebi-ai`，API Key 与会话不丢
  4. 通用版与律师版（`~/.lebi-law`）仍然隔离，绝不合并
- **明确不做什么：** 本轮不改技术层标识（crate 名 `hermes-*`、二进制名 `hermes`、事件名 `hermes://`、`HERMES_LOG` 等），它们是内部契约，客户不可见；后续可单独立项。不重做产品口号（保留「越用越像你的手感」）。

---

## 0c. 架构师视角

- **根因层级：** 品牌 = 配置 + 文案 + 资源（tauri.conf / i18n / icons / 打包脚本）；数据目录 = 常量 + 首次迁移
- **正确的长期默认路径：**
  - 用户可见层一律从 i18n（`app.brand`）取品牌，不硬编码
  - `paths.rs` 单一真相源：`DEFAULT_DATA_DIRNAME = ".lebi-ai"`、`ENV_DATA_DIR = "LEBI_DATA_DIR"`；`maybe_migrate_data_root()`：目标不存在且旧目录存在 → 整目录 rename（原子、不删源以外的任何数据）；律师版目录 `~/.lebi-law` 绝不触碰
  - 图标母版 1024×1024 PNG 一份，`tauri icon` 生成 GUI 全套，`icons_launcher` 生成 Flutter 全套（两个前端同一身份）
- **与引擎/各入口边界：** 只改 surface 层（gui/flutter/cli 文案与资源）+ core 的 data root 常量；引擎逻辑、crate 依赖、companion 协议不动
- **安全影响：** 数据仍本地明文；迁移仅做 rename，不复制、不删除；token/config 权限 0600 不变
- **如何防复发：** `rg -i hermes` 检查用户可见层归零；`app.brand` 是唯一品牌源；图标母版注释说明再生成方式
- **为何这不是补丁：** 品牌名收敛为单一事实源（i18n + tauri 配置 + 母版图标），数据目录改名走标准迁移，不靠环境碰巧

---

## 1. 方案（Plan）

- **目标：** 全用户可见面统一为 乐彼AI / lebi-AI，图标自研，数据目录改名且安全迁移
- **范围：** 做——tauri 配置/窗口/dmg、GUI i18n 与组件文案、CLI banner 与命令帮助、README/P0/P1/P2/docs、图标全套、Flutter 显示名与图标、数据目录迁移；**不做**——crate/二进制/事件名等内部标识、口号文案
- **用户路径变化：** 见 0b
- **技术要点：**
  - `crates/hermes-gui/tauri.conf.json`：productName → `lebi-AI`、title → 乐彼AI（zh）/ lebi-AI（en 由 UI 决定，窗口标题固定用 lebi-AI）、identifier → `com.lebi.ai`
  - `crates/hermes-core/src/paths.rs`：目录名/env 改名 + `maybe_migrate_data_root()`
  - i18n.ts：所有用户可见 Hermes 字样 → 品牌；`~/.small-rust-hermes` → `~/.lebi-ai`
  - `crates/hermes-core/src/banner.rs`：ASCII Hermes → lebi-AI 品牌横幅
  - 图标：numpy+PIL 生成母版 → `cargo tauri icon` → `dart run icons_launcher:create`
- **风险与回滚：** 迁移为纯移动（rename/并入缺项），不覆盖、不删除、不碰 `~/.lebi-law`；同名冲突留在旧目录作备份；改回旧版本仍可读旧目录。风险低。实测：本机 `~/.lebi-ai` 已有测试残留，迁移自动并入真实数据（331MB），旧目录收走
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 2026-08-06 · Codex

---

## 2. 实施

- **用户可见层：** `tauri.conf.json`（productName/title `lebi-AI`、identifier `com.lebi.ai`）；GUI i18n（`chat.header`/`onboarding.kicker`/微信/设置/记忆/技能/MCP 文案 → lebi-AI / 乐彼AI，`~/.small-rust-hermes` → `~/.lebi-ai`）；`index.html` title；CLI `about`、`hermes init` 标题、`banner.rs` ASCII 横幅（lebi-AI + 「越用越像你的手感」）；`danger.rs` 确认文案；companion 协议（`PRODUCT_NAME`/Care 标记 → lebi-AI，反射管线保留旧 `[Hermes Care]` 识别）；Flutter（`app.dart` title、Android label 乐彼AI、iOS CFBundleDisplayName lebi-AI、pubspec description、theme 注释）；图标全套（Tauri `icons/` + Flutter iOS/macOS/Android/assets）。
- **数据层：** `paths.rs` 默认根 `~/.lebi-ai`、env `LEBI_DATA_DIR`（兼容旧 `HERMES_DATA_DIR`）、`maybe_migrate_data_root()`：目标不存在→整目录 rename；目标已存在→并入缺失项（不覆盖不删除，冲突留旧目录作备份）；绝不触碰 `~/.lebi-law`；CLI/GUI 启动各调一次。
- **文档层：** P0/P1/P2/README/docs 品牌与数据目录同步；仓库根无新增 md。
- **清理：** 移除 `OnboardingRitual.tsx` 死导入（tsc 报错）；实测迁移后清理 `~/.lebi-ai` 与旧目录中的测试残留 jsonl（全部 `rationale:"test"`，非用户数据）。
- **技术层保留（有意）：** crate/二进制名 `hermes-*`、事件 `hermes://`、`hermes:inbox-changed`、localStorage key、`HERMES_LOG`；Flutter macOS bundle 名（Xcode 工程硬编码 `hermes_app.app`，无 flutter 工具链下不冒险改）。

## 3. 测试

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | `data_root()` 默认 | ends_with `.lebi-ai` | ✅ 单测 |
| 2 | `LEBI_DATA_DIR=/tmp/x` | 覆盖生效 | ✅ 单测 |
| 3 | 旧 `HERMES_DATA_DIR` 兜底 | 仍生效（兼容） | ✅ 单测 |
| 4 | 迁移：仅旧目录存在 | rename，内容不丢 | ✅ 单测 |
| 5 | 迁移：新目录已存在（部分） | 并入缺失项，不覆盖 | ✅ 单测 |
| 6 | 迁移：同名冲突 | 目标优先，旧副本保留 | ✅ 单测 |
| 7 | 迁移：`~/.lebi-law` | 绝不触碰 | ✅ 单测 |
| 8 | 真实数据迁移（本机） | 331MB 完整并入 `~/.lebi-ai`，doctor 正常 | ✅ 实测 |
| 9 | GUI 前端 | `tsc && vite build` 通过；dist 含品牌文案 | ✅ |
| 10 | fmt/clippy/test | 全绿 | ✅ 260 tests |

## 4. 验收

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 图标/窗口/引导/侧栏/打包品牌一致（lebi-AI / 乐彼AI） |
| 开箱即用未破坏 | ✅ | 无新运行时/DB；数据自动迁移一次 |
| 本地优先未破坏 | ✅ | 数据仍本地明文，`~/.lebi-law` 隔离保留 |
| 测试通过 | ✅ | 单测/实测/clippy/fmt 全绿 |
| 记录完整 | ✅ | 本台账四阶段 |
| 产品+架构两视角齐全 | ✅ | |
| 非修修补补（默认路径正确） | ✅ | 品牌单一事实源（i18n + tauri + 母版图标）；迁移为纯移动合并，不依赖环境碰巧 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理 | ✅ | 死导入清除；旧路径注释同步；dist 无用户可见 Hermes |

- **验收人：** Codex（工程验收）；用户 GUI 视觉验收待确认
- **验收日期：** 2026-08-06
- **结论：** ☑ 工程通过 · GUI/dmg 视觉待用户确认
- **遗留项：** Flutter macOS 显示名（Xcode 工程绑定，待有 flutter 工具链时处理）；GUI/dmg 图标视觉验收需用户打开确认
