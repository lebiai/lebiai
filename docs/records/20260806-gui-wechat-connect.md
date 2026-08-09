# 变更记录：GUI 内嵌微信连接（分发场景扫码免终端）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-gui-wechat-connect` |
| **日期** | 2026-08-06 |
| **状态** | 待验收（工程通过 · GUI 真机扫码待手测） |
| **负责人** | 架构 / GUI 实施 |
| **关联** | 会话：微信渠道端到端验证（本机扫码成功，bot_id=a7f1852efc52@im.bot） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 终端客户 —— 每人一台电脑、安装同一个打包好的 dmg，**无终端能力**（P0：GUI 场景免终端）。
- **解决什么痛点：** 当前微信连接只能靠 `hermes wechat login` 在**终端**渲染二维码；打包成 dmg 后客户没有终端，微信渠道对客户等于不可用。
- **用完后用户多得到什么：** 首次启动 GUI → 设置页/引导页点「连接微信」→ 手机上扫码 → 启用，全程点鼠标零命令；之后在微信里就能对话。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（复用现有引擎与 `~/.small-rust-hermes/`）
  - [x] 步骤可感知、可预期（扫码 → 已连接 → 启用）
  - [x] 不增加无意义确认或噪音（登录是低频一次性操作）
  - [x] 高频路径比改前更快（客户原来根本没有可用路径）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 客户拿到 dmg 打开 GUI，想在微信里随时找 Hermes 办事；电脑就在手边，但客户不会（也不该需要）开终端。
- **路径变化：**
  - 改前：`hermes wechat login`（终端二维码）→ `hermes wechat run`（终端常驻）→ 微信对话。客户无法完成。
  - 改后：GUI「微信连接」→ 画布二维码 → 手机微信扫码确认 → 一键启用（后台长轮询）→ 微信对话。零终端。
- **成功标准：** 客户在 GUI 里完成扫码并看到「已连接（bot_id=…）」；微信发消息能收到回复；关闭 GUI 或 token 失效时有明确提示并可一键重扫。
- **明确不做什么：**
  - 不做微信远程文件访问（另立需求，安全方案需单独设计）
  - 不做飞书 / Telegram 的 GUI 连接（同架构可扩展，本次只做微信）
  - 不做 Windows 打包配置（当前分发为 macOS dmg；Windows 另议）
  - 不做多 bot / 多账号管理（每人一台电脑，天然单 bot）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 入口能力缺失（UI 加载 / 命令面）。登录协议（`hermes-weixin::auth::LoginSession`）与凭证（`wechat.toml`）已正确，缺的是「无终端 surface 的登录与运行入口」。
- **正确的长期默认路径：** 扫码登录与长轮询是**引擎能力**，GUI 与 CLI 都调用同一实现：
  1. `LoginSession` 增加「取 QR matrix」能力（现只有 `render_terminal()` unicode 渲染），matrix 经 Tauri command 交给前端画布绘制。
  2. `wechat run` 的主循环（轮询 / cursor 持久化 / 断线重试 / token 失效判定）下沉为**共享服务函数**（`hermes-weixin::service::serve`，消息处理以回调/trait 注入），CLI 与 GUI 共用——禁止 GUI 复制一套平行循环（P0 第九条）。
  3. 新增 Tauri commands：`wechat_login_start` / `wechat_login_poll`（含过期自动刷新）/ `wechat_start` / `wechat_stop` / `wechat_status`；长轮询以 tokio task 常驻 GUI 进程，退出时 graceful shutdown（复用现有 Ctrl-C 的 shutdown 语义）。
- **与引擎/各入口边界：** 协议与循环单一来源；CLI `wechat login/run` 行为不变；`hermes-server` 不新增微信路由（手机端管理微信不在本次范围，避免契约漂移）。
- **安全影响：** 凭证仍 `~/.small-rust-hermes/wechat.toml`（0600，复用 `StoredCreds::save`）；token 只存后端，前端只见「已连接/未连接」；二维码短时效、过期自动刷新；微信工具白名单（`CHAT_TOOL_WHITELIST`）**不变**，不因 GUI 接入而放开任何工具。
- **如何防复发：** 加一条开发规则——「IM 登录/长轮询入口必须复用 `hermes-weixin::service`，禁止新入口另起循环」；GUI 设置页与 CLI 共用同一凭证文件，状态以文件为准。
- **为何这不是补丁：** 这是把已存在的 CLI 登录/运行能力按「多入口共享引擎」原则补上缺失的 GUI surface，不是临时特判。

---

## 1. 方案（Plan）

- **目标：** 打包分发（dmg）场景下，客户在 GUI 内完成微信扫码连接与启停，零终端。
- **范围：**
  - **做**：`hermes-weixin` 增加 service 层（共享 run 循环 + login matrix 输出）；GUI 设置页「微信连接」区块 + Tauri commands + 前端画布二维码 + 状态展示；OnboardingRitual 可选「连接微信」步骤；i18n（zh-CN/en-US）。
  - **不做**：见 0b。
- **用户路径变化：** 见 0b。
- **技术要点：**
  - `crates/hermes-weixin`：`LoginSession` 增加 `matrix()`/图片数据输出；新增 `service::serve(creds, on_message, shutdown)`（把 `crates/hermes-cli/src/commands/wechat.rs` 主循环抽出，CLI 改调共享函数）。
  - `crates/hermes-gui`：新增 wechat commands + state（task handle / 状态机：Stopped→Connecting→Listening→TokenExpired）；事件用 Tauri emit 推给前端。
  - `crates/hermes-gui/ui`：`SettingsPanel` 加「微信连接」；`OnboardingRitual` 可选步骤；canvas/SVG 画二维码；连接状态与「重新扫码」入口。
  - 分发前置（另立事项）：dmg 需 Developer ID 签名 + 公证，否则客户被 Gatekeeper 拦截。
- **风险与回滚：**
  - iLink 服务准入 / 登录限频未知（本机已验证单账号可用）——若服务收紧，GUI 只影响微信渠道，其余入口不受影响；可回滚为 CLI-only（旧路径保留在共享 service 之上，非双轨）。
  - GUI 内嵌长轮询与主循环共存：tokio task 生命周期管理需测试（退出、重启、重复启停）。
  - 二维码画布为无终端环境唯一方式：需真机扫码验证刷新与确认路径。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期：2026-08-06 · 人：用户确认（onboarding 可选扫码 · 仅微信，飞书/Telegram 暂缓）

---

## 2. 实施（Implement）

- **实际改动摘要：**
  - 新增 `crates/hermes-channel`：共享渠道驱动（`Channel` trait、`ServeCtx`、`UserState`、`serve_inbound`、`handle_text_message`、`ContextSources`、`compose_system_prompt`/`inject_time_header`、`CHAT_TOOL_WHITELIST`）——从 CLI 抽取，CLI 与 GUI 单一来源。
  - `hermes-weixin`：`LoginSession::matrix()`（QR 布尔矩阵供画布）、`refresh`/`ExpiredSignal` 公开（GUI 自驱轮询）、新增 `service::serve` 共享长轮询循环（poll/cursor/重试/过期判定/非文本拒绝/消息回调）、`Client` 实现 `Channel`。
  - `hermes-feishu` / `hermes-telegram`：`Channel` 实现迁入各自协议 crate（孤儿规则）。
  - CLI：删除 `commands/channel.rs`（旧循环已清）；`context.rs` 变薄壳；`chat/mod.rs` 新增 `build_channel_ctx()` 并 re-export 共享提示词；`wechat/feishu/telegram` 改调共享层（wechat run 使用 `service::serve` + `handle_text_message`）。
  - GUI：新增 `WechatState`（login/serve_users/shutdown/serve_task/status，全部 Arc 共享）；6 个 Tauri commands（`wechat_login_start/poll/status/start/stop/logout`，事件 `wechat-status` 推送）；前端 `WechatConnectCard`（画布二维码 + 状态 + 启停/断开）；`SettingsPanel` 插入连接卡片；`OnboardingRitual` 可选「连接微信」折叠步骤；i18n keys（zh-CN/en-US）。
  - 文档：README crate 列表与 `docs/project-map.md` 架构表补 `hermes-channel`。
- **关键路径/文件：** `crates/hermes-channel/{channel,context,system_prompt}.rs`、`crates/hermes-weixin/src/{auth,client,service}.rs`、`crates/hermes-cli/src/commands/{wechat,feishu,telegram,chat/mod,context,mod}.rs`、`crates/hermes-gui/src/{state.rs,commands/wechat.rs,main.rs}`、`crates/hermes-gui/ui/src/components/settings/{WechatConnectCard,SettingsPanel}.tsx`、`crates/hermes-gui/ui/src/components/ritual/OnboardingRitual.tsx`、`crates/hermes-gui/ui/src/i18n.ts`
- **偏离方案处：** 无（方案确认：onboarding 可选扫码 · 仅微信）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 客户首次连接 | GUI 点「连接微信」→ 扫码 → 手机确认 | 显示已连接（bot_id） | 待测 | 真机（GUI 窗口） |
| 2 | 二维码过期 | 等待过期 | 画布自动刷新新码 | 待测 | GUI 手工 |
| 3 | 微信收发 | 启用后微信发消息 | 正常回复；状态区显示监听中 | 通过（CLI 共享 service 链路）| GUI 状态区显示待真机确认 |
| 4 | token 失效 | 删除 wechat.toml / 伪造 token | GUI 提示过期并提供一键重扫 | 待测 | |
| 5 | 升级保留 | 覆盖安装新 dmg | 凭证与会话保留 | 待测 | 分发验证 |
| 6 | CLI 回归 | `hermes wechat login/run`（新共享 service） | 行为与改前一致 | 通过（监听启动 + 微信收发正常，2026-08-06） | 旧循环已删除 |

- **自动化：** `cargo fmt --all -- --check` ✅ / `cargo clippy --workspace --all-targets -- -D warnings` ✅ / `cargo test --workspace` ✅（全绿）/ `vite build` ✅（dist 已更新）。
- **手工：** 上述用例 1–5（GUI + 真机微信）。
- **测试结论：** [x] 工程侧全部通过（fmt/clippy/test/vite build/CLI 收发）· [x] 已知问题：GUI 真机扫码流程待手工验收（本环境无 GUI 交互）；`npm run build` 的 `tsc` 阶段因工作区既有 `chatStore.ts` 2 个类型错误失败（非本变更引入，vite build 正常）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 分发客户 GUI 内扫码零终端 |
| 开箱即用未破坏 | ☑ | 单一引擎多入口，未引入运行时/DB |
| 本地优先未破坏 | ☑ | 凭证仍 `wechat.toml` 0600，会话本地 JSONL |
| 测试通过 | ☑ | fmt/clippy/test/vite build 全绿；CLI 收发验证通过 |
| 记录完整 | ☑ | 本台账四阶段 |
| 产品+架构两视角齐全 | ☑ | |
| 非修修补补（默认路径正确） | ☑ | 共享 `hermes-channel`/`service::serve`，GUI 不 fork |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | `commands/channel.rs` 删除；`chat/system_prompt.rs` 删除；CLI 主循环移除 |

- **验收人：** 用户（GUI 真机扫码手测后确认）
- **验收日期：** 2026-08-06（工程侧）
- **结论：** ☐ 通过（工程侧已通过，GUI 真机扫码待用户手测后转正式验收）· ☐ 驳回
- **遗留项：** （1）GUI 真机扫码流程手工验收（`scripts/run-gui.sh` → 设置 → 微信连接 → 扫码 → 启用）；（2）工作区既有 `chatStore.ts` 的 2 个 TS 类型错误（非本变更引入，建议单独台账处理）；（3）iLink 服务准入与限频观察；（4）dmg 签名 + 公证流程（分发前置，独立于本变更）。

---

## 5. 附注

- 本机已验证：微信渠道 CLI 端到端可用（2026-08-06，bot_id=a7f1852efc52@im.bot，会话落盘 `~/.small-rust-hermes/sessions/wechat/`）。
- 会话记录位置：`docs/records/20260806-gui-wechat-connect.md`（本文档）。
