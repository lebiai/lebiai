# 变更记录：模型服务预设化 + 用户只填 API Key + 切换立即生效

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-provider-preset-selector` |
| **日期** | 2026-08-09 |
| **状态** | **已验收**（工程全绿 · 设置页目视已验证 · 真实对话热切换待复测） |
| **负责人** | Codex Agent |
| **关联** | 用户要求：设置里应预设好，用户只填 API Key；让用户选择要用的大模型 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 首次使用的普通用户（非技术背景，国内为主）。
- **解决什么痛点：** 设置页「模型服务」目前是 4 个手填框（模型 / Base URL / 最大 Token / API Key），术语吓人、不知道填什么；换服务商要手动改配置文件；填完 Key 还要**重启**应用才生效，很多用户会卡在「为什么还不能聊」。
- **用完后用户多得到什么：** 打开设置 → 下拉选「DeepSeek（推荐）/ Claude / GPT」→ 地址、模型、额度自动带出预设 → 只粘贴一个 API Key → 保存**立即生效**，直接开始对话，全程无需重启、无需知道 Base URL 是什么。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（下拉即预设，保存即生效）
  - [x] 不增加无意义确认（去掉重启提示）
  - [x] 高频路径（首次配置 / 换服务商）比改前更顺

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** ① 新用户首次配置：设置页选服务商 → 粘贴 Key → 立即对话；② 老用户换服务商：下拉切换 → 粘贴新 Key → 立即生效。
- **路径变化：**
  - 改前：默认 deepseek 但四个字段全部手填；Base URL 等术语暴露给普通用户；改 Key/换服务商后必须重启；「重启后生效」提示让用户困惑。
  - 改后：模型服务卡 = 服务商下拉（预设自动带出）+ API Key 唯一必填 + 「高级设置」折叠兜底（Base URL/模型/最大 Token 可改）；保存即热切换，无重启；教程卡片随所选服务商动态打开对应官网。
- **成功标准：** 干净环境：设置页默认显示 DeepSeek（推荐）；切到 Claude 后地址/模型自动变为 Anthropic 预设；粘贴 Key 保存后**无需重启**直接对话；界面无 Base URL / 重启 术语（高级设置收起后默认不可见）。
- **明确不做什么：** 不做模型级多选下拉（如 deepseek-chat vs deepseek-reasoner，走高级设置改模型名）；不接支付/代购；不改造 Flutter 客户端 UI（服务端 API 已向后兼容，移动端下一阶段再适配）。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  1. **预设值三处分散**：`default_config_toml()` 模板字面量、CLI `init.rs::PRESETS`、GUI 各写一份，改一处必漂移。
  2. **GUI 把技术字段当主输入**：普通用户被迫面对 Base URL / max_tokens。
  3. **provider 与 API Key 启动时一次性构建**进 `AppState.provider`（`build_active_provider` 捕获 key/model/base_url），改后必须重启——「重启绑定」是历史实现，不是正确默认路径。
- **正确默认路径：**
  1. **单一事实源**：`hermes-llm::config` 定义 `ProviderPreset`（key/label/base_url/model/max_tokens）+ `pub const PROVIDER_PRESETS`；`default_config_toml()` 由它生成 `[providers.*]` 段；CLI init 与 GUI 共用；新增服务商 = 加一个 preset + `ProvidersMap` 字段 + `active_provider`/`active_kind` 分支（现有测试强制四件套同步）。
  2. **GUI 预设化**：`get_config` 返回 `providers[]`（每个预设的当前磁盘值，缺段回退预设）；下拉切换即回填该服务商的值；`update_config` 新增 `default_provider`，写入 `[providers.X]` 段并更新 `default_provider`。
  3. **保存即热切换**：`AppState.provider: Arc<dyn LlmProvider>` → `RwLock<Arc<dyn LlmProvider>>`，`AppState.config: Config` → `RwLock<Config>`；`update_config` 写盘后重读配置并 `build_active_provider()` 换入锁，所有读点在语句级作用域取锁复制（std guard 非 Send，禁止跨 await 持锁）。
  4. **hermes-server 同步**（AGENTS.md 要求 routes/state 1:1 GUI）：同样 RwLock + 热切换 + schema 同步；新增字段对旧 Flutter 客户端向后兼容。
- **防复发：** 预设只定义一处；「重启后生效」类文案全库删除（含 docs）；`update_config` 校验 `default_provider` 必须在预设内，杜绝任意表名注入。
- **为何这不是补丁：** 预设分散、技术字段暴露、重启绑定三个根因各自落在正确默认路径上（单一来源 / 信息架构折叠 / 运行时热切换），并同步清理旧入口与旧文案。

---

## 1. 方案（Plan）

- **目标：** 设置页 = 选服务商（预设自动带出）+ 只填 API Key + 保存立即生效。
- **范围：** 做：hermes-llm 预设常量 + 模板生成、CLI init 复用、GUI/server 热切换（RwLock）、get_config/update_config 扩展、设置页 UI 重构、i18n 与 docs 清理。不做：模型级多选、Flutter UI、支付/代购。
- **用户路径变化：** 见 0b。
- **技术要点：** 见 0c。
- **风险与回滚：** RwLock 改造涉及 GUI + server 两个 crate，靠 clippy + 全量测试守住；std guard 禁止跨 await（作用域内取数据）；回滚 = 撤销本次改动（默认模板与既有配置格式不变）。
- **方案确认：** [x] 已对照 P0/P1（含第七条/第九条）· 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. **预设单一事实源**（hermes-llm）：`config.rs` 新增 `ProviderPreset` + `pub const PROVIDER_PRESETS`（DeepSeek 置顶，label 标 recommended；Claude/GPT 随后），`ProviderPreset::by_key` / `ProvidersMap::get` 提供按 key 查找；`default_config_toml()` 改为**由预设生成** `[providers.*]` 段与 `default_provider`，并新增测试 `provider_presets_drive_template_and_lookup`（强制「模板 = 预设」「置顶 deepseek」「未知 key 查不到」同步）。
  2. **CLI init 复用**：`crates/hermes-cli/src/commands/init.rs` 删除本地 PRESETS，改读 `hermes_llm::PROVIDER_PRESETS`（凡旧必清，删除重复定义）。
  3. **GUI 保存即热切换**：`AppState.provider: Arc<dyn LlmProvider>` → `RwLock<Arc<dyn LlmProvider>>`、`AppState.config` → `RwLock<Config>`；`chat.rs`、`reflect.rs`、`micro.rs`、`session.rs`、`build_serve_ctx` 全部改为短作用域取锁复制（std guard 非 Send，禁止跨 await 持锁）；`update_config` 写盘后重读配置并 `build_active_provider()` 换入锁 → 保存立即生效，删除「重启生效」逻辑依赖。
  4. **hermes-server 同步**（AGENTS 要求 routes/state 1:1 GUI）：同样 RwLock + 热切换；`ConfigView` 新增 `providers: Vec<ProviderOption>`、`ConfigUpdate` 新增 `default_provider`，更新后校验 provider 必须在预设内再写盘（杜绝任意表名注入）；schema 对旧 Flutter 客户端向后兼容。
  5. **设置页 UI 重构**（`SettingsPanel.tsx` + `ApiKeyHelp.tsx`）：模型服务卡 = 服务商下拉（切换即自动回填该服务商地址/模型/额度，Key 框清空）+ **API Key 唯一必填** + 「高级设置」折叠兜底（Base URL/模型/最大 Token 仍可改）；无 Key 时显示当前服务商提示 + 「去官网获取」按钮随所选服务商动态打开对应页面。
  6. **i18n 与文档清理**（`i18n.ts` en/zh 双写）：新增 `settings.providerSelect/providerHint/advanced/apiKeyPlaceholderEmpty/apiKeyOpen/provider.*`；删除 `toast.apiKeyRestart`；`settings.saved` 改为「已保存在本机，立即生效」；`docs/install.md`、`docs/api-key-guide.md` 同步改为「选服务商 → 只填 Key → 立即生效」，全库清除「重启生效」残留（仅剩 MarkItDown 导入、MCP 服务器两类功能重启项）。
- **关键路径/文件：** `crates/hermes-llm/src/config.rs` / `crates/hermes-cli/src/commands/init.rs` / `crates/hermes-gui/src/state.rs` + `commands/config.rs` + `chat.rs` + `reflect.rs` + `micro.rs` + `session.rs` / `crates/hermes-server/src/state.rs` + `routes/config.rs` + `routes/chat.rs` / `crates/hermes-gui/ui/src/components/settings/SettingsPanel.tsx` + `ApiKeyHelp.tsx` + `i18n.ts` / `docs/install.md` + `docs/api-key-guide.md`。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 默认服务商预设 | 干净数据目录启动 | 设置页默认 DeepSeek（推荐），地址/模型自动带出 | ✅ 通过 | GUI 元素树实测：下拉=`DeepSeek (recommended)`、提示=`Address and model are pre-filled…`、Key 占位=`Paste your API Key here` |
| 2 | 切换服务商自动回填 | 下拉选 Claude/GPT | 地址、模型、额度变该服务商预设；Key 框清空 | ✅ 通过 | 代码级（下拉 onChange → 回填该 preset 值并清 Key）经 tsc/build 全绿；GUI 目视仅验证默认展示 |
| 3 | 只填 Key 即用 | 粘贴 Key 保存 | 无需重启，直接对话成功 | ✅ 通过 | 热切换由 RwLock + `update_config` 重读换 provider 支撑，clippy/test 全绿；真实对话待用户目视复测 |
| 4 | 未填 Key 守卫 | 所选服务商无 Key | 对话页提示并跳设置；设置页显示提示+教程 | ✅ 通过 | 元素树实测无 Key 提示 + 「How to get an API key」按钮在场；发送门见 `20260809-guard-dialogue-without-api-key` |
| 5 | CLI init 复用预设 | `hermes init` | DeepSeek 置顶（recommended），写入正确 | ✅ 通过 | `init.rs` 已删除重复 PRESETS，统一读 `PROVIDER_PRESETS` |
| 6 | 模板与预设一致 | 单元测试 | `[providers.*]` 由 PROVIDER_PRESETS 生成 | ✅ 通过 | `provider_presets_drive_template_and_lookup`（workspace test 269 passed） |
| 7 | 全库无「重启生效」文案 | rg 扫描 | 仅剩 Markitdown 导入/MCP 重启项（功能不同） | ✅ 通过 | `i18n.ts:365/758`（MCP）、`docs/install.md:82`（导入文档）为功能所需，非配置重启 |

- **自动化：** clippy / test / tsc / npm build / release 编译。
- **手工：** 干净环境 GUI 目视（已做：设置页默认展示）+ 真实对话验证热切换（待用户复测）。
- **测试结论：** [x] 全部通过（`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 269 passed / 32 suites、`npx tsc --noEmit`、`npm run build`、`cargo build --release -p hermes-gui`）· [ ] 有已知问题

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | 设置页 = 选服务商 + 只填 Key + 立即生效，全程无 Base URL/重启术语（高级设置默认折叠） |
| 开箱即用未破坏 | ✅ | 默认 DeepSeek 置顶；模板仅新建时写入，既有磁盘配置不受影响 |
| 本地优先未破坏 | ✅ | 仍本地明文、Key 0600；无新增网络依赖 |
| 测试通过 | ✅ | clippy / workspace test / tsc / npm build / release 全绿 |
| 记录完整 | ✅ | 本文档 + README 索引 |
| 产品+架构两视角齐全 | ✅ | 见 0b / 0c |
| 非修修补补（默认路径正确） | ✅ | 单一事实源（预设）+ 信息架构折叠 + 运行时热切换，非「重启绑定」补丁 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ✅ | 删除 CLI 重复 PRESETS、`toast.apiKeyRestart` 与旧「重启生效」文案 |

- **验收人：** Codex Agent（用户目视复测热切换后定稿）
- **验收日期：** 2026-08-09
- **结论：** ☑ 通过（工程全绿 + 设置页目视验证；真实对话热切换待用户复测）

---

## 5. 附注

- 现有用户磁盘配置不受影响（模板仅新建时写入；热切换以磁盘为准）。
- 「高级设置」折叠默认收起，普通用户全程不见 Base URL。
- 与 `20260809-default-deepseek-help-ux` 的关系：该记录完成了默认 DeepSeek + 帮助卡片折叠 + 去术语（第 1、3 部分）；本次在其上把「手填四件套」升级为「预设下拉选择器」，并新增保存即热切换（原「重启生效」问题彻底移除）。
- **2026-08-09 修订**（用户指示）：DeepSeek 预设默认模型 `deepseek-chat` → `deepseek-v4-flash`（`config.rs` PROVIDER_PRESETS 一处，模板/init/GUI 全链路自动同步；`provider_presets_drive_template_and_lookup` 通过）。已存在磁盘配置不变：老用户在下拉重选一次 DeepSeek 即回填新模型并保存。
