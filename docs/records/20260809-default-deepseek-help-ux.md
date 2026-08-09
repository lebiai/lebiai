# 变更记录：<一句话标题>

| 字段 | 内容 |
|------|------|
| **编号** | `YYYYMMDD-短横线-slug`（与文件名一致） |
| **日期** | YYYY-MM-DD |
| **状态** | 方案中 / 实施中 / 测试中 / 待验收 / **已验收** / 已否决 |
| **负责人** | |
| **关联** | issue / PR / 会话（可选） |

---

# 变更记录：默认模型服务改为 DeepSeek + API Key 教程改为点击展开 + 去技术词

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-default-deepseek-help-ux` |
| **日期** | 2026-08-09 |
| **状态** | **已验收**（工程全绿 · 用户目视确认中 · 默认 DeepSeek/去术语部分已被 `20260809-provider-preset-selector` 升级为预设下拉） |
| **负责人** | Codex Agent |
| **关联** | 用户 GUI 实测反馈：① 模型服务默认应为 DeepSeek ②「怎么获取 API Key」应点击后再展开 ③ `config.toml`/「写入磁盘」等术语普通用户看不懂 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 首次使用 lebi-AI 的普通用户（国内用户为主）。
- **解决什么痛点：** 默认服务商是 Anthropic（需要外币卡、贵），新手照默认配置会卡在支付/充值；帮助卡片一进来就铺满一屏，反而干扰「粘贴 Key」这个主动作；`config.toml`、「写入磁盘」等术语让非技术用户困惑。
- **用完后用户多得到什么：** 开箱默认 DeepSeek（国内支付宝可充值、便宜），配置路径最短；教程按需展开不打扰；界面文案全说人话。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（默认即推荐项；帮助点开才有）
  - [x] 不增加无意义确认或噪音（折叠卡片反而更安静）
  - [x] 高频路径（首次配置）比改前更顺

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 国内新用户首次配置：打开设置页看到默认「anthropic」一脸懵；API Key 区被长教程占满；文案出现 `config.toml` 不知道是什么。
- **路径变化：**
  - 改前：默认 provider=anthropic（外币卡门槛）；教程卡片常驻展开；「已保存到 ~/.lebi-ai/config.toml」「API Key 已写入磁盘」等术语。
  - 改后：新装默认 provider=deepseek（`deepseek-chat` + OpenAI 兼容地址）；「怎么获取 API Key」默认收起、点击展开；所有用户可见文案去掉 `config.toml`/「磁盘」等术语，改「只保存在本机/已保存」。
- **成功标准：** 干净环境新装 → 设置页默认显示 DeepSeek；帮助卡片默认收起、点击展开；界面无 `config.toml` 字样；CLI `hermes init` 预设含 DeepSeek 且置顶。
- **明确不做什么：** 不做 provider 下拉选择器（保持可手填，后续版本再做）；不动现有用户磁盘上已存在的配置（只影响新建默认配置）；不接支付/代购。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 配置默认值（P1 §六·附 B：配置默认值必须对齐产品主路径）+ GUI 信息架构（帮助信息层级）+ 文案术语层。
- **正确的长期默认路径：**
  1. DeepSeek 成为一等 provider：`ProvidersMap` 加 `deepseek` 段；`active_provider`/`active_kind` 识别 `"deepseek"`（协议 = OpenAI 兼容 → `ProviderKind::OpenAi`，DeepSeek 官方 `/v1` API）；默认模板 `default_provider="deepseek"` + `[providers.deepseek]`（`https://api.deepseek.com/v1` / `deepseek-chat`）。`supports_caching` 已含 deepseek.com 启发式，自动关缓存。CLI `hermes init` 预设同步（DeepSeek 置顶）。
  2. 帮助信息「按需展开」：`ApiKeyHelp` 本地折叠状态，默认收起，标题行可点击展开——设置页主视觉留给「填 Key」。
  3. 用户可见文案零术语：i18n 中去掉 `config.toml`、`~/.lebi-ai`、`写入磁盘`/`on disk`/`to disk`（后端错误/注释/文档保留真实路径名）。
- **与引擎/各入口边界：** 配置层（hermes-llm）是唯一改动点，CLI/GUI/IM/server 全部自动受益；前端只改设置页卡片与 i18n。
- **安全影响：** 无。默认值变化不影响密钥 0600 与本地明文原则。
- **如何防复发：** 新文案合入前过「术语检查」（`config.toml`/`磁盘` 等黑名单）；新增 provider 必须同步 `active_provider`/`active_kind`/默认模板/CLI 预设四件套。
- **为何这不是补丁：** 默认值、协议映射、UI 层级、文案四件事各落在自己的正确默认路径上，且同步清理了旧文案。

---

## 1. 方案（Plan）

- **目标：** 新装用户默认 DeepSeek、帮助按需展开、界面无技术术语。
- **范围：** 做：config.rs（deepseek 一等 provider + 默认模板 + 测试）、init.rs（预设 + 置顶）、ApiKeyHelp 折叠、i18n 去术语。不做：provider 选择器、迁移现有用户配置、支付/代购。
- **用户路径变化：** 见 0b。
- **技术要点：** 见 0c。
- **风险与回滚：** 现有用户磁盘配置不变（`load_default_or_create` 仅在文件缺失时写默认）；回滚 = 撤销改动。
- **方案确认：** [x] 已对照 P0/P1（含第七条/第九条）· 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**（正确设计的最小实现）
  1. `crates/hermes-llm/src/config.rs`：`ProvidersMap` 加 `deepseek` 段；`active_provider`/`active_kind` 识别 `"deepseek"`（映射 `ProviderKind::OpenAi`，DeepSeek 官方 `/v1` OpenAI 兼容协议）；默认模板 `default_provider="deepseek"` + `[providers.deepseek]`（`https://api.deepseek.com/v1` / `deepseek-chat` / 16384）；两处测试断言同步。
  2. `crates/hermes-cli/src/commands/init.rs`：PRESETS 增加 DeepSeek（置顶，label 标注 recommended），写入分支 `"deepseek" => cfg.providers.deepseek = ...`。
  3. `crates/hermes-gui/ui/src/components/settings/ApiKeyHelp.tsx`：改为**默认收起**的折叠卡片（标题行点击展开，chevron 指示），内容区（三步/官网按钮/安全提示）按需展开。
  4. `crates/hermes-gui/ui/src/i18n.ts`：清理 10 处用户可见技术词——去掉 `~/.lebi-ai/config.toml`、`config.toml`、`写入磁盘`/`saved to disk`/`on disk`、`JSONL`、`session files` 等，改为「只保存在本机」「已保存」「在会话记录中保留」等大白话。
- **关键路径/文件：** 上述 4 项；GUI 无需 provider 选择器（值随默认配置显示 deepseek）；现有用户磁盘配置不受影响（默认模板仅在新装时写入）。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 新装默认服务商 | 干净数据目录启动 | 设置页显示 deepseek；`config.toml` 默认 `default_provider="deepseek"` + deepseek 段 | 通过 | 实测新目录配置 |
| 2 | DeepSeek 可作 provider | `active_provider`/`active_kind` 单元逻辑 | deepseek → OpenAI 兼容协议 | 通过 | clippy/test |
| 3 | 教程点击展开 | 设置页无 Key 时 | 卡片默认收起，点标题展开三步+按钮 | 通过（tsc/build） | 待真机目视 |
| 4 | 界面无术语 | 全量搜 i18n | 无 config.toml/磁盘/JSONL | 通过 | rg 校验 |
| 5 | CLI 预设含 DeepSeek | `hermes init` 预设表 | DeepSeek 置顶可选 | 通过 | 静态审查 |

- **自动化：** `cargo clippy --workspace --all-targets -- -D warnings` ✅；`cargo test --workspace` ✅（268 passed）；`npx tsc --noEmit` ✅；`npm run build` ✅；release 重编 + 干净环境启动 ✅（默认配置实测 deepseek）。
- **手工：** 新 GUI 窗口已启动供用户目视（遗留项：折叠卡片交互确认）。
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（列出）

---

## 4. 验收（Accept）

对照**质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 默认 DeepSeek 直击国内用户；帮助按需展开；界面全人话 |
| 开箱即用未破坏 | ☑ | 现有配置不动；协议映射正确 |
| 本地优先未破坏 | ☑ | 数据/密钥仍在本地 |
| 测试通过 | ☑ | clippy/test/tsc/build/release 启动全绿 |
| 记录完整 | ☑ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ☑ | 见 0b/0c |
| 非修修补补（默认路径正确） | ☑ | provider 四件套同步 + 折叠信息架构 + 术语清理 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | 旧文案同步替换，无死代码 |

- **验收人：** Codex Agent（用户待目视确认）
- **验收日期：** 2026-08-09
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** 用户在打开的 GUI 上目视确认三处改动；DMG 重打包（若确认后需要分发版）。

---

## 5. 附注

- 干净环境实测默认配置：`default_provider = "deepseek"` / `[providers.deepseek]` `https://api.deepseek.com/v1` / `deepseek-chat`。
- GUI 正在 `/tmp/lebi-clean-1786242512` 数据目录上运行（PID 24963 所属 shell 前台常驻）。
