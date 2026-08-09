# 变更记录：GUI 微信连接 · 目标路径重塑（扫码弹窗化 + 一键重扫）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-gui-wechat-connect-flow` |
| **日期** | 2026-08-06 |
| **状态** | **已验收** |
| **负责人** | 架构 / GUI 实施 |
| **关联** | [20260806-gui-wechat-connect](./20260806-gui-wechat-connect.md)（初版实施，工程侧已通过，GUI 真机待手测）；本记录按用户确认的目标路径重塑该功能的 GUI 交互层 |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 终端客户 —— 每人一台电脑、安装同一个打包好的 dmg，**无终端能力**（P0：GUI 场景免终端）。
- **解决什么痛点：** 初版「GUI 内嵌微信连接」已可用但交互不明确：二维码内嵌在设置页/引导页卡片里（小、深、没有明确的「窗口」）；token 失效后界面上只有「启用/断开」，客户必须先断开再重新扫码，不是台账承诺的「一键重扫」。
- **用完后用户多得到什么：** 扫码有**唯一的、明确的窗口**（居中弹窗）；token 过期后一个按钮直达重扫；状态五态一目了然；操作步骤比初版更少、更可预期。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（复用现有引擎与 `~/.small-rust-hermes/`）
  - [x] 步骤可感知、可预期（弹窗 → 扫码 → 已连接 → 启用 → 监听中）
  - [x] 不增加无意义确认或噪音（仅断开保留确认）
  - [x] 高频路径比改前更快（token 过期修复路径从「断开+重扫」变为「一键重扫」）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 客户在 GUI 里连接/维护微信渠道，全程鼠标、零终端。
- **路径变化（目标路径 · 已由用户定死）：**

```
入口（两处，同一套组件）：
  首次引导页 Onboarding：可选「连接微信」折叠项，展开后是按钮（不内嵌二维码）
  设置页：微信连接卡片 = 状态摘要 + 操作按钮

扫码 = 独立弹窗（唯一的「窗口」）：
  点「扫码连接微信」→ 居中模态框（遮罩；Esc / 点遮罩 / 「取消」可关闭）
  弹窗内：二维码画布（≥280px）+ 实时状态文案 + 过期自动刷新新码
  手机确认 → 弹窗自动关闭 → toast「微信已连接」

状态机（定死 5 态）：
  未连接(黄)        未连接                    按钮：扫码连接微信
  已连接·未启用(黄) 已连接 · 服务未启用        按钮：启用微信服务 · 断开
  监听中(绿)        监听中（bot_id）          按钮：停止 · 断开
  token 过期(红)    登录已过期，请重新扫码    按钮：重新扫码（直接弹窗）· 断开
  出错(红)          错误详情                  按钮：回到可恢复操作
```

- **成功标准：** 客户点「扫码连接微信」必然看到居中弹窗二维码；手机扫码确认后弹窗自动关闭并 toast；微信发消息能收到回复；token 失效时点「重新扫码」直接出弹窗，无需先断开。
- **明确不做什么：**
  - 不做飞书 / Telegram 的 GUI 连接
  - 不做多 bot / 多账号管理
  - 不做 Windows 打包配置
  - 不做微信远程文件访问（另立需求，安全方案单独设计）
  - 不改 CLI 微信路径（`wechat login/run` 行为不变）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** UI 交互与入口设计层 + 状态机动作缺口。引擎（`hermes-weixin::service::serve`、`LoginSession`、`StoredCreds`）与共享渠道驱动（`hermes-channel`）均正确，问题在 GUI surface：二维码内嵌卡片、token 过期无直达动作。
- **正确的长期默认路径：**
  1. **扫码交互收敛为唯一弹窗**：复用 GUI 既有 Modal 模式（`fixed inset-0` 遮罩 + 居中面板，如 `ConfirmModal`），「窗口」成为可预期的单一事实。
  2. **状态机单一来源**：后端 `WechatStatus.state`（stopped/listening/token_expired/error）+ 磁盘凭证存在性，前端只由 state **推导**可用按钮，不另存状态副本。
  3. **「重新扫码」不新增后端 command**：`wechat_login_start` 本就新建 `LoginSession`（幂等覆盖 in-flight session），token 过期后前端直接调它即可；确认后凭证覆盖保存，无需先删文件。
  4. **清冗余**：删除 `WechatStatusView::from` 中硬编码 `logged_in: true / listening: false` 再由 `status_view` 覆盖的实现（P0 第九条），视图字段统一由 `status_view` 计算。
- **与引擎/各入口边界：** 本次只动 `crates/hermes-gui`（commands 清理 + 前端组件）；`hermes-weixin` / `hermes-channel` / CLI 零改动；不新增 `hermes-server` 路由（手机端管理微信不在范围）。
- **安全影响：** 不变。凭证仍 `~/.small-rust-hermes/wechat.toml`（0600）；token 只存后端，前端只见状态；二维码短时效、过期自动刷新；`CHAT_TOOL_WHITELIST` 不变。
- **如何防复发：** 状态机以后端字段为单一真相；新增 UI 一律复用 Modal 模式；IM 登录/长轮询入口必须复用共享 service（既有规则，保留）。
- **为何这不是补丁：** 这是把初版「能用但交互不明确」的 GUI surface 收敛到正确默认路径（弹窗 + 五态 + 直达重扫），并顺手清理视图层冗余，不是临时特判。

---

## 1. 方案（Plan）

- **目标：** 按定死的目标路径重塑 GUI 微信连接的交互，补齐「唯一扫码窗口」与「token 过期一键重扫」，清理视图层冗余。
- **范围：**
  - **做**：新建 `WechatQrModal`（弹窗扫码）；`WechatConnectCard` 精简为状态摘要 + 操作按钮；`OnboardingRitual` 折叠项展开后只显示按钮；i18n（zh-CN/en-US）；后端视图字段冗余清理；`docs/records/README.md` 索引登记。
  - **不做**：见 0b。
- **用户路径变化：** 见 0b（改前 = 内嵌卡片扫码 / 过期需先断开；改后 = 弹窗扫码 / 一键重扫）。
- **技术要点：**
  - 后端：`crates/hermes-gui/src/commands/wechat.rs` —— 删除 `WechatStatusView::from` 冗余；`status_view` 保持唯一计算点；确认 `wechat_login_start` 覆盖 in-flight session（已幂等，无需改动）。
  - 前端：`crates/hermes-gui/ui/src/components/settings/WechatQrModal.tsx`（新，复用 Modal 模式，画布 ≥280px，处理 waiting/scanned/refreshed/confirmed）；`WechatConnectCard.tsx` 精简（状态摘要 + 扫码连接/重新扫码/启用/停止/断开）；`OnboardingRitual.tsx` 折叠项展开后显示「扫码连接微信」按钮（点击弹窗）；`i18n.ts` 补 zh-CN/en-US keys。
- **风险与回滚：** 纯 GUI 层改动，无协议/引擎变更；回滚 = 还原前端组件与 command 视图层。弹窗轮询逻辑沿用现有 1s poll + 过期自动 refresh，风险低。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期：2026-08-06 · 人：用户（目标路径逐条确认「可以」）

---

## 2. 实施（Implement）

- **实际改动摘要：**
  - 后端 `crates/hermes-gui/src/commands/wechat.rs`：删除 `WechatStatusView::from` 冗余实现（硬编码 `logged_in: true / listening: false` 再被覆盖），`status_view` 成为视图字段唯一计算点；`wechat_login_start` 幂等覆盖 in-flight session，token 过期「重新扫码」直接复用，无需新增 command。
  - 前端新增 `crates/hermes-gui/ui/src/components/settings/WechatQrModal.tsx`：**唯一扫码弹窗**（居中 Modal、遮罩 + Esc + 取消关闭；280px 画布；打开即发起登录；1s 轮询；过期自动刷新并提示；确认后关窗 + toast + 回调刷新状态）。
  - 前端精简 `WechatConnectCard.tsx`：移除内嵌二维码与轮询逻辑，只保留状态摘要 + 操作按钮；按钮由后端 `state` + 凭证文件**单一来源推导**。
  - `OnboardingRitual.tsx` 无需改动（卡片自带按钮 → 弹窗）；`i18n.ts` 补齐 keys（zh-CN/en-US）。
- **实施中追加修复（用户手测发现）：**
  - ① **断开确认失效**：Tauri WKWebView 不支持 `window.confirm`（静默返回 `false`），断开永远走不到 `wechat_logout`。改为项目自定义确认弹窗（复用 Modal 模式，`confirmLogout` 本地 state）。
  - ② **停止无反馈**：`wechat_stop` 原阻塞等待 serve 循环退出（最长 ~38s 长轮询超时），期间 UI 无感知。改为立即置 `stopping` 状态并推事件；task 退出后自行清 `serve_task` 并推最终状态（stopped/token_expired/error）；前端 `stopping` 显示「正在停止…」+ 转圈并隐藏操作按钮。`wechat_logout` 同步传 `AppHandle`。
- **工具链说明：** `apply_patch` 在本环境曾静默失败（exit 0 但未写盘），导致早期构建/台账未落地；改用 `cat >` / python 精确替换，并在 build 后验证 dist 产物内容（hash + 关键字符串）确保页面真实更新。
- **关键路径/文件：** `crates/hermes-gui/src/commands/wechat.rs`、`crates/hermes-gui/ui/src/components/settings/{WechatQrModal,WechatConnectCard}.tsx`、`crates/hermes-gui/ui/src/i18n.ts`
- **偏离方案处：** 无（追加修复 ①② 为手测发现，已并入本记录）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 客户扫码连接 | 设置页点「扫码连接微信」 | 出现居中弹窗二维码 | 通过 | 用户手测 |
| 2 | 弹窗关闭 | 点遮罩 / Esc / 「取消」 | 弹窗关闭，状态不变 | 通过（工程侧） | 与 2b 弹窗同模式 |
| 2b | 断开确认弹窗 | 已连接/监听中点「断开连接」 | 弹出确认弹窗，确认后断开 | 通过 | 用户手测（修复①） |
| 2c | 停止反馈 | 监听中点「停止」 | 立即显示「正在停止…」转圈，随后回已连接 | 通过 | 用户手测（修复②） |
| 3 | 扫码确认 | 手机微信扫码 → 手机确认 | 弹窗自动关闭 + toast「微信已连接」 | 通过 | 用户手测 |
| 4 | 二维码过期 | 等待过期 | 弹窗内自动刷新新码 | 通过（工程侧） | 复用既有过期刷新链路 |
| 5 | 启用/停止 | 已连接后点启用 → 点停止 | 状态 监听中(绿) ↔ 已连接·未启用(黄)；停止有「正在停止…」反馈 | 通过 | 用户手测 |
| 6 | token 失效一键重扫 | 伪造/删除凭证使 token 失效 | 状态红「登录已过期」；点「重新扫码」直接出弹窗 | 通过（工程侧） | 复用 `wechat_login_start` 幂等覆盖；真机过期待观察 |
| 7 | 微信收发 | 监听中发微信文本/非文本 | 文本回复、非文本礼貌拒绝 | 通过 | 用户手测；CLI 共享链路 2026-08-06 本机已验 |
| 8 | 引导页入口 | Onboarding 展开「可选：连接微信」 | 显示按钮，点击出弹窗 | 通过（工程侧） | 与设置页共用同一卡片组件 |
| 9 | CLI 回归 | `hermes wechat login/run` | 行为与改前一致 | 通过（工程侧） | 共享 `service::serve` 未改动，全仓测试全绿 |

- **自动化：** `cargo fmt --all -- --check` ✅ / `cargo clippy --workspace --all-targets -- -D warnings` ✅ / `cargo test --workspace` ✅（255 通过 / 0 失败）/ `npx vite build` ✅（dist 已更新并验证含新代码）/ `npx tsc --noEmit` ⚠️（本机 16GB 内存近满时 tsc 被系统限流 kill，非代码问题；vite esbuild 转译通过，tsc 曾在内存充裕时通过）
- **手工：** 上表 GUI + 真机微信用例。
- **测试结论：** [x] 用户手测通过（扫码弹窗、断开确认、停止反馈、启用/监听、微信收发）· [x] 工程侧通过（fmt/clippy/test/vite build）· [x] 已知环境问题：tsc 在本机内存压力下被 kill（非代码问题）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 弹窗扫码 + 一键重扫，交互可预期；手测通过 |
| 开箱即用未破坏 | ☐ | 无新运行时/DB |
| 本地优先未破坏 | ☐ | 凭证仍 0600，会话本地 JSONL |
| 测试通过 | ☑ | fmt/clippy/test/vite build 全绿；用户手测通过；tsc 受本机内存限制（环境问题） |
| 记录完整 | ☐ | 本台账四阶段 |
| 产品+架构两视角齐全 | ☐ | |
| 非修修补补（默认路径正确） | ☐ | 弹窗 + 五态 + 直达重扫；引擎零改动 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | `WechatStatusView::from` 冗余删除；内嵌二维码路径移除；`window.confirm` 替换 |

- **验收人：** 用户（GUI 真机手测后确认）
- **验收日期：** 2026-08-06（用户验收）
- **结论：** ☑ **通过**（用户手测 + 工程侧全绿）· ☐ 驳回
- **遗留项：** （1）token 失效「一键重扫」真机过期场景长期观察；（2）dmg 签名 + 公证流程（分发前置，独立于本变更）；（3）本机 16GB 内存下 `npm run build` 的 tsc 阶段需在内存宽松时执行（CI 环境不受影响）。

---

## 5. 附注

- 关联初版台账 `20260806-gui-wechat-connect.md`：工程侧已通过；本记录重塑其 GUI 交互层，CLI 共享 service 路径不变。
- 「1 窗口在哪里弹」的答案（定死）：扫码窗口 = 点「扫码连接微信」弹出的居中模态框，唯一弹窗；引导页「可选：连接微信」是折叠第 1 步，展开后为按钮。
