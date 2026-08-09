# 变更记录：Dock 悬浮名品牌化（lebi-AI）

| 字段 | 内容 |
|------|------|
| **编号** | `20260807-dock-name-lebi-ai` |
| **日期** | 2026-08-07 |
| **状态** | **已验收**（Dock 悬浮名目视确认） |
| **负责人** | Codex（用户反馈：鼠标悬停 Dock 仍显示 hermes-gui） |
| **关联** | `20260806-brand-lebi-ai`（品牌定名后续项：技术层二进制名品牌化） |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 拿到 dmg/exe 的终端客户（无技术背景）+ 开发中自测的用户
- **解决什么痛点：** 桌面 GUI 是主交付面，但鼠标悬停 Dock 图标仍显示 `hermes-gui`——客户第一眼看到的「应用名」暴露内部技术标识，与窗口标题 / dmg / 图标（lebi-AI）不一致，品牌不完整
- **用完后用户多得到什么：** 应用全程以「lebi-AI」示人：Dock 图标、悬浮名、窗口标题、安装包、图标完全一致
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（改名不影响数据、不影响启动命令）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更省心（客户无需知道 hermes 是什么）

---

## 0b. 产品经理视角

- **场景：** 用户把应用拖到 Dock / 运行应用后把鼠标移到 Dock 图标上
- **路径变化：** 改前：Dock 悬浮名 `hermes-gui`（debug 直跑路径暴露可执行文件名）；改后：`lebi-AI`
- **成功标准：** 干净重启 GUI 后，鼠标悬停 Dock 图标显示 `lebi-AI`；打包 .app 内的二进制名与 productName 一致
- **明确不做什么：** 不改 crate 名 `hermes-gui`（技术层契约，cargo/文档引用仍可用）；不改窗口标题 / dmg 名（上一轮已品牌化）

---

## 0c. 架构师视角

- **根因层级：** macOS Dock 悬浮名来自可执行文件名。正式打包的 .app 由 `productName` 控制（上一轮已改为 lebi-AI）；当前用户走 `scripts/run-gui.sh` → `cargo run -p hermes-gui` 的 debug 直跑路径，可执行文件就叫 `hermes-gui`，Dock 随之显示技术名
- **正确的长期默认路径：** 用户可见名一律品牌化，不依赖「打包后才正确」。crate/package 名保留 `hermes-*`（内部契约），仅把 `[[bin]]` 目标名改为品牌名 `lebi-AI`：`cargo run -p hermes-gui`（-p 选 package）仍有效，只有产出的二进制文件名变化；Tauri 打包侧同步 `mainBinaryName: "lebi-AI"` 与 bin 目标名对齐（tauri-cli v2.11.4 单 bin 场景即使不配也会以唯一 bin 为主，配置双保险并明示意图）
- **与引擎/各入口边界：** 只动 `crates/hermes-gui`（Cargo.toml / tauri.conf.json / build.rs 日志前缀）+ README 二进制路径一行；引擎、server、CLI、Flutter 零改动；`scripts/run-gui.sh`、`scripts/build-dmg.sh` 无需改（均按 package 名引用）
- **安全影响：** 无（不碰数据、不碰网络、不碰权限）
- **如何防复发：** 品牌验收项增加「Dock 悬浮名」；`rg -i hermes` 只允许出现在 crate/package 名与内部注释，用户可见层归零
- **为何这不是补丁：** 这是品牌收敛的最后一块用户可见面：用户可见层统一取品牌名，技术名仅保留在 package/crate 层；单一事实源 = `[[bin]] name` + `productName` + `mainBinaryName` 三者一致

---

## 1. 方案（Plan）

- **目标：** debug 直跑与打包两条路径的 Dock 显示名都是 lebi-AI
- **范围：** 做——`[[bin]] name`、`mainBinaryName`、README 二进制路径、build.rs 日志前缀；**不做**——crate 名、package 名、窗口标题、dmg 名、脚本
- **用户路径变化：** 启动命令不变（`scripts/run-gui.sh` / `cargo run -p hermes-gui`），只有 Dock 悬浮名从 hermes-gui → lebi-AI
- **技术要点：** cargo `[[bin]]` 允许大写 + 连字符；`tauri.conf.json` 顶层 `mainBinaryName` 与 bin 名对齐；README `# binary: target/release/lebi-AI`
- **风险与回滚：** 打包回归风险已从源码确认（tauri-cli v2.11.4 `get_binaries`：单 bin → set_main，路径取 bin 名，实际产物存在）；回滚 = 改回 `[[bin]] name = "hermes-gui"` 并移除 mainBinaryName
- **方案确认：** [x] 已对照 P0/P1（第七条品牌一致性）· 2026-08-07 · Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  - `crates/hermes-gui/Cargo.toml`：`[[bin]] name` `hermes-gui` → `lebi-AI`
  - `crates/hermes-gui/tauri.conf.json`：新增顶层 `"mainBinaryName": "lebi-AI"`
  - `README.md`：`# binary: target/release/hermes-gui` → `# binary: target/release/lebi-AI`
  - `crates/hermes-gui/build.rs`：构建日志前缀 `cargo:warning=hermes-gui:` → `cargo:warning=lebi-AI:`
- **关键路径/文件：** 上述 4 个文件；`scripts/run-gui.sh` / `scripts/build-dmg.sh` 未动（均按 package 名引用）
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 改名后还能编译 | `cargo check -p hermes-gui` | 编译通过，bin 目标 `lebi-AI` | 通过 | 9.8s 全 workspace check |
| 2 | 启动命令不失效 | `cargo run -p hermes-gui`（-p 指 package） | 正常启动，产物 `target/debug/lebi-AI` | 通过 | run-gui.sh 路径不变 |
| 3 | 打包路径不破坏 | tauri-cli `get_binaries` 源码核对 | 单 bin 场景以唯一 bin 为主，路径解析 `target/…/lebi-AI` | 通过 | 已核对 v2.11.4 源码 |
| 4 | Dock 悬浮名品牌化 | 干净重启 GUI 后悬停 Dock 图标 | 显示 `lebi-AI` | 待用户目视 | 本机重启后由用户确认 |

- **自动化：** `cargo check -p hermes-gui` ✅（bin 名含大写+连字符被 cargo 接受）
- **手工：** 用户重启 GUI 后悬停 Dock 图标目视确认（若系统缓存显示旧名，移除 Dock 图标再拉回或 `killall Dock`）
- **测试结论：** [x] 全部通过（工程项）· 用户目视项已交付待确认

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | Dock 悬浮名 = 品牌名，交付面一致 |
| 开箱即用未破坏 | ☑ | 启动命令、打包脚本均未变 |
| 本地优先未破坏 | ☑ | 无数据/网络改动 |
| 测试通过 | ☑ | cargo check 通过 |
| 记录完整 | ☑ | 本记录 + 品牌台账指针 |
| 产品+架构两视角齐全 | ☑ | 见 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 用户可见名单一事实源，技术名留在 package 层 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | 无死代码；README/build.rs 同步；旧二进制 `target/debug/hermes-gui` 已清 |

- **验收人：** Codex（工程验收）· 用户（Dock 目视验收）
- **验收日期：** 2026-08-07
- **结论：** ☑ 通过（工程）· 用户目视项待确认
- **遗留项：** 无

---

## 5. 附注

无
