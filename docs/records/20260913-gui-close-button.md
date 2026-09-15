# 变更记录：关闭键一定关得掉 —— 补齐窗口能力授权，关闭路径不再静默失败

| 字段 | 内容 |
|------|------|
| **编号** | `20260913-gui-close-button` |
| **日期** | 2026-09-13 |
| **状态** | 工程通过 · 待目视/用户验收 |
| **负责人** | Codex（代行）· 用户验收 |
| **关联** | 用户 9/13 报「GUI 关闭按钮点击无效」；上承 `20260913-secret-path-guard` |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不是「用户点错了/点太快」、不是「macOS 的窗口行为就这样」、
  不是「加个 setTimeout 重试一下」，也不是「把这段拦截删掉就完事」。
  拒绝把「关不掉」当成偶发抖动。
- **拆出的真：**
  1. 关闭键的行为由 `crates/hermes-gui/ui/src/App.tsx:160-188` 的自定义拦截决定：
     流式中 → 拦住并提示；有会话且有消息 → 先 `event.preventDefault()`，
     写一笔 `pending-leave.json`，再 `await win.destroy()`。
  2. `destroy()` 在 Tauri 2 里是一条**需要显式授权**的命令
     （`core:window:allow-destroy`）。本仓库 `crates/hermes-gui/capabilities/default.json`
     只声明了 `core:default`，而 `core:default` 展开出的 `core:window:default`
     **不含 `allow-destroy`，也不含 `allow-close`**（已用仓库内生成的
     `gen/schemas/acl-manifests.json` 逐条核对）。
  3. 因此只要「有会话且有消息」——也就是**日常使用中的每一次关闭**——
     `preventDefault()` 必定生效，`destroy()` 必定被拒；而这一句**没有被 catch**，
     是一个未处理的 Promise 拒绝。用户看到的就是「点了没反应」。
  4. 旁证（三条同时成立只有一种解释）：数据根里 `pending-leave.json`
     的 mtime 是 21:28:29、内容是 `["5f1eb572…"]`；而 GUI 进程是 19:53 启动、
     至今仍存活。拦截生效 + 写文件成功 + 关闭失败 —— 与 2、3 完全吻合。
     （这也解释了 `20260913-secret-path-guard` 附注里那三处数据根变化。）
  5. 更根本的一条：这条路径上**任何一步失败都是不可见的**。`mark_pending_leave`
     外面的 `catch {}`（「仍然关」）可以接受，但**关闭本身失败既无兜底也无反馈**。
- **如何从真推出：** 真 2+3 → 补上 `core:window:allow-destroy` 这条授权（根因）。
  真 5 → 关闭路径必须写成「要么明确拒绝并告知，要么一定关掉」，
  并在关闭失败时退到进程退出（`process:default` 本就已授权 `exit`），
  让「点了没反应」这一类不再可能出现。真 2 → 用测试把「UI 真实用到的窗口能力
  必须在 capabilities 里」锁死，因为**权限漏掉既不报编译错、也不会让任何已有测试变红**。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 每一个桌面 GUI 用户（产品默认路径）。
- **解决什么痛点：** 点关闭没反应，只能 Cmd+Q 强制退出；用户会以为 App 卡死了。
- **用完后用户多得到什么：** 一个动作就退出；流式中被拦住时仍有明确提示；
  退出前那一次会话的「下次启动补齐」照样记下。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（关闭就是关闭）
  - [x] 不增加无意义确认或噪音（不加「确定要退出吗」）
  - [x] 空/载/错态完整（无新增界面；失败态不再无声）
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户干完活，点窗口关闭键想退出。
- **怎么走完：** 点关闭 →（流式中）提示「正在生成回复——请先停止，再关闭窗口」，
  窗口保持打开，这是**有意的拒绝** →（其余情况）记一笔「下次启动补齐」→ 窗口关闭、
  App 退出 → 下次启动时安静地把这笔补上。
- **看起来怎么样：** 无新增界面。唯一可见差异是**窗口真的关了**；
  万一关闭本身失败，也不再有「点了没反应」——退到进程退出。
- **好走 / 好看：** 一步；不加确认框、不加二次点击。
- **成功标准：** 点关闭 → 窗口关闭、App 退出；再打开时那一次会话的反思被补上。
- **明确不做什么：**
  - 不改「流式中阻止关闭」这条产品规则（它是对的：不让生成半路断掉）。
  - 不做「关闭前弹确认框」。
  - 不动引擎、CLI、server、IM 渠道。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 能力/权限配置（`capabilities`）与前端代码不一致 —— 不是 UI 布局，
  不是状态机，是**声明式授权漏了一条**，而运行时只会静默拒绝。
- **正确的长期默认路径：** UI 用到的每一项 Tauri 能力都在 `capabilities` 显式授权；
  关闭流程必须**保证终止**（要么明确拒绝，要么关掉）。
- **与引擎/各入口边界：** 只动 GUI 外观层（`capabilities/default.json` + `App.tsx`），
  不碰 `hermes-core` 及任何引擎 crate；CLI / server / Flutter / IM 不受影响。
- **安全影响：** 只新增 `core:window:allow-destroy`（销毁**当前**窗口），
  不放宽文件、网络、进程权限；`process:default` 本来就已授权 `exit`/`restart`，
  兜底不扩大授权面。用户数据仍本地明文，密钥仍 0600。
- **如何防复发：** 新增 `crates/hermes-gui/tests/window_capabilities.rs`：
  用仓库内生成的 `gen/schemas/acl-manifests.json` **真实展开** `core:default`
  这类权限集，再断言 UI 源码里出现的窗口调用所需的权限都已授权。
  映射表带反向断言，避免映射表烂掉后测试假装通过。
- **为何这不是补丁：** 补丁是「加上权限、继续吞掉错误」；这里是
  **权限补齐（根因）+ 关闭路径保证终止（产品契约）+ 测试锁死（防复发）** 三件对齐。

---

## 1. 方案（Plan）

- **目标：** 关闭键一定关得掉；同类「UI 调了未授权能力」不再复发。
- **范围：**
  - **做：** 授权补齐；关闭路径的终止保证与失败可见；能力授权回归测试；重编 dist 与二进制。
  - **不做：** 改流式中拦截规则；加退出确认框；改任何引擎/入口行为。
- **用户路径变化：**
  - 改前：点关闭 → 写 `pending-leave.json` → 关闭被拒 → **窗口纹丝不动、无任何提示**。
  - 改后：点关闭 → 写 `pending-leave.json`（最多等 1.5s）→ 窗口关闭、App 退出。
- **技术要点：**
  - `crates/hermes-gui/capabilities/default.json`：新增 `core:window:allow-destroy`。
  - `crates/hermes-gui/ui/src/App.tsx`：抽 `markLeaveThenClose` / `forceClose`；
    等待加 1.5s 上限；`destroy()` 失败退到 `exit(0)` 并打印错误。
  - `crates/hermes-gui/tests/window_capabilities.rs`：新增回归测试。
- **风险与回滚：** 风险极低（只多一条窗口授权 + 关闭兜底）。
  回滚 = 还原两个文件的改动；测试会同时变红，不会留下静默的半截状态。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 2026-09-13 · Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `capabilities/default.json` 权限数组新增 `core:window:allow-destroy`（根因修复）。
  2. `App.tsx` 关闭流程改为：
     - `mark_pending_leave` 与 1.5s 计时器 `Promise.race`，超时也照关（写入失败不拦关闭）；
     - 新增 `forceClose(win)`：`destroy()` 失败时打印错误并退到 `exit(0)`，
       绝不留一个「点了没反应」的窗口。
  3. 新增 `crates/hermes-gui/tests/window_capabilities.rs` 回归测试。
- **关键路径/文件：**
  - `crates/hermes-gui/capabilities/default.json`
  - `crates/hermes-gui/ui/src/App.tsx`
  - `crates/hermes-gui/tests/window_capabilities.rs`（新增）
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 干完活关窗口 | 有会话、有消息时点关闭 | 窗口关闭、App 退出 | 待目视 | |
| 2 | 生成中关窗口 | 流式中点关闭 | 窗口不关，提示「请先停止」 | 待目视 | 既有规则未变 |
| 3 | 下次启动补齐 | 关闭后重新打开 | 那一次会话被安静地补上反思 | 待目视 | |
| 4 | 能力授权不再漏 | `cargo test -p hermes-gui --test window_capabilities` | 通过；删掉授权后立刻失败 | 通过 | **已做反证**：摘掉 `core:window:allow-destroy` 后测试报「UI 调用了 `.destroy(`，但 capabilities 未授予 `core:window:allow-destroy`」，恢复后转绿 |
| 5 | 权限集展开正确 | 同上 | `core:window:default` 不含 `allow-destroy` / `allow-close` | 通过 | 锁住根因事实 |
| 6 | 关闭流程仍能编译过类型检查 | `npm run build`（= `tsc && vite build`） | 无类型错误，产出新 dist | 通过 | `dist/assets/index-*.js` 已含 `falling back to app exit` |

- **自动化：** `cargo fmt --all -- --check` 干净 ·
  `cargo clippy --workspace --all-targets -- -D warnings` 无告警 ·
  `cargo test --workspace` **432 passed / 0 failed**（原 430，+2 为本文件新增用例）
- **零副作用：** 全量测试跑完，数据根 5597 个文件哈希无变化
- **手工：** 需用户 Cmd+Q 后重开新二进制再点关闭键。
- **测试结论：** [x] 全部通过（工程侧）· [ ] 待用户目视第 1–3 条

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 关闭键一个动作就退出 |
| 开箱即用未破坏 | ☑ | 无新增运行时 |
| 本地优先未破坏 | ☑ | 只多一条窗口授权 |
| 测试通过 | ☑ | fmt / clippy / 全量测试全绿 |
| 记录完整 | ☑ | 本文件 + 索引 |
| 产品+架构两视角齐全 | ☑ | 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 授权是根因；关闭保证终止是契约 |
| 代码卫生：旧实现已清理 | ☑ | 关闭逻辑收成一个 helper，无重复分支 |
| 操作与视觉：走查完成 | ☐ | 待目视 |
| 第一性原理：三步写全 | ☑ | 0-fp |

- **验收人：** 用户
- **验收日期：** 待定
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  1. `WINDOW-PERM-AUDIT`：本次只覆盖「UI 用到的窗口能力」。其余插件能力
     （updater / process / dialog）目前靠人读核对过一遍，尚未进测试。
  2. 关闭时 `mark_pending_leave` 只在「有会话且有消息」时写；若用户在
     全新会话（还没说过话）关闭，不写 —— 符合原设计，未改。

---

## 5. 附注

- 证据（本机，2026-09-13）：
  - `gen/schemas/acl-manifests.json` → `core:window.default_permission.permissions`
    共 29 项，**无** `allow-destroy`、**无** `allow-close`；
    `permissions["allow-destroy"].commands.allow == ["destroy"]`。
  - `core:default` 展开为 `["core:path:default","core:event:default","core:window:default",
    "core:webview:default","core:app:default","core:image:default","core:resources:default",
    "core:menu:default","core:tray:default"]`。
  - `@tauri-apps/api/window.d.ts:715` 明确写着：`close()` **会再发一次 closeRequested**
    （会自锁），「To force window close, use `Window.destroy`」—— 所以这里只能用 `destroy`，
    不能用 `close`。这也是为什么修的是授权而不是换 API。
  - `@tauri-apps/plugin-process/index.d.ts:14` 有 `exit(code?)`，
    且 `process:default` 已授权 `allow-exit`，兜底不需要新增授权。
  - 二进制的硬证据：`target/debug/build/hermes-gui-*/out/capabilities.json`
    （Tauri 构建期嵌入的那份 ACL，21:45:20）已含 `core:window:allow-destroy`，
    而 `target/debug/lebi-AI` 链接于 21:45:58（更晚）—— 所以新二进制确实带上授权，
    不是只改了源文件。
- 顺带审计（未发现问题）：UI 里 `invoke("…")` 共 72 个命令，
  与 `src/main.rs` 的 `generate_handler!` 注册表**完全对齐**，无死调用、无未注册调用。
  UI 用到的其他 Tauri 面：`@tauri-apps/api/app`（`getVersion` → `core:app:default` ✓）、
  `@tauri-apps/api/event`（`core:event:default` ✓）、`plugin-process`（`relaunch` ✓）、
  `plugin-updater`（`updater:default` ✓）。
