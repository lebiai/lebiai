# 变更记录：桌面安装包（macOS DMG + Windows EXE）与用户安装指引

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-installers-dmg-exe-guide` |
| **日期** | 2026-08-09 |
| **状态** | **已验收**（工程全绿 · DMG 实跑；Windows/CI 待实跑） |
| **负责人** | Codex Agent |
| **关联** | README「Package a macOS DMG」；`docs/gui-run.md` 打包节；`docs/records/20260803-markitdown-release-bundle.md` |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 拿到安装包的个人用户（macOS / Windows），与后续打包的开发者/发布者。
- **解决什么痛点：** 目前只有 macOS DMG 脚本且未验证；Windows 完全没有 exe 安装包路径（普通用户不会编译）；拿到安装包后「双击会不会被拦、要不要装别的东西、数据存在哪、怎么配 API Key」全无说明。
- **用完后用户多得到什么：** macOS 一键产出可分发 DMG；Windows 有官方路径产出 NSIS 安装包（本机或 CI）；拿到安装包的用户有清晰的「安装 → 首次放行 → 配置 Key → 开始对话 → 数据在哪」指引。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（安装包即开即用；WebView2 缺失时 NSIS 自动装引导）
  - [x] 步骤可感知、可预期（指引覆盖首次放行与配置）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径（下载 → 安装 → 对话）比改前更顺

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户从发布渠道下载 lebi-AI 安装包，双击安装，首次打开被系统安全拦截；不知道配什么、数据放哪。
- **路径变化：**
  - 改前：只有 macOS 构建脚本；无 Windows 产物；无任何「拿到包后怎么办」的说明。
  - 改后：macOS 用户「打开 DMG → 拖入应用程序 → 右键打开放行 → 三屏引导填 Key → 对话」；Windows 用户「运行 setup.exe → SmartScreen 放行 → 开始菜单启动 → 同上」。开发者可用 `scripts/build-exe.ps1` 或 CI release workflow 产出两种安装包。
- **成功标准：** 可观察——DMG 实际构建成功且含 markitdown sidecar；Windows NSIS 构建命令/CI 存在且产出路径明确；`docs/install.md` 完整覆盖 macOS/Windows 安装、首次放行、配置、数据位置、卸载与常见问题。
- **明确不做什么：** 不做代码签名/公证（标注为分发前必做、另立任务）；不做 Windows 版文档导入 sidecar 捆绑（macOS-only，Windows 走 data-dir/`HERMES_MARKITDOWN` 回落并诚实说明）；不做 Linux 桌面包；不做自动更新。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 发布链路（tauri bundle 配置 + 打包脚本 + CI + 用户文档）缺失 Windows 路径与安装后指引。
- **正确的长期默认路径：**
  - `tauri.conf.json` `bundle.targets` 设为 `"all"`：Tauri 按宿主平台解析（macOS→app+dmg；Windows→msi+nsis+app），脚本用 `--bundles dmg` / `--bundles nsis` 钉死产物，消除「配置里写死平台名导致跨平台构建失败」。
  - markitdown sidecar 移到 `tauri.macos.conf.json`（Tauri v2 平台配置合并）：248MB macOS venv 只进 DMG，不进 Windows exe；GUI 运行时解析逻辑（资源目录 → crate resources → data-dir → env）不变，Windows 无捆绑时自然回落到 data-dir/`HERMES_MARKITDOWN`（`document_import.rs` 已支持，无新代码）。
  - Windows exe 的唯一正确产出路径 = Windows 宿主（本机 `scripts/build-exe.ps1`）或 CI（`release.yml` windows-latest）；macOS 上无法交叉编译 Tauri，文档与脚本都明示，不靠环境碰巧。
  - 用户指引放 `docs/install.md`（根目录 md 白名单约束），README 只放入口。
- **与引擎/各入口边界：** 纯打包与文档改动，不触及 `hermes-core` 引擎与运行时契约；GUI 代码零改动。
- **安全影响：** 安装包未签名 → 文档必须如实说明 Gatekeeper / SmartScreen 拦截与放行方式；不引入任何新网络出站。
- **如何防复发：** 打包配置只允许在 `tauri*.conf.json` 与 `scripts/`、`.github/workflows/` 中改动；新增平台产物必须同步 `docs/install.md` 与 README；「文档诚实」检查（P1 §六·附 B）覆盖打包节。
- **为何这不是补丁：** 每项改动都落在 Tauri 平台配置、标准打包脚本、CI artifact、用户文档的单一默认路径上，不添加任何 if/特判或环境依赖。

---

## 1. 方案（Plan）

- **目标：**
  1. macOS：验证 `scripts/build-dmg.sh` 实跑产出 DMG（含 sidecar）。
  2. Windows：新增 `scripts/build-exe.ps1`（NSIS setup.exe）+ `release.yml` CI 工作流（macos dmg + windows nsis）。
  3. 用户指引：新增 `docs/install.md`（macOS/Windows 安装、首次放行、配置、数据、卸载、FAQ），README 与 `docs/README.md` 登记。
  4. 台账完整走完 方案→实施→测试→验收。
- **范围：** 做：上述 1–4。**不做**：代码签名/公证、Windows sidecar 捆绑、Linux 打包、自动更新、发布到 GitHub Releases 的自动化下载页。
- **用户路径变化：** 见 0b。
- **技术要点：** `tauri.conf.json`（targets "all" + 移除 resources）；新增 `tauri.macos.conf.json`（resources 移入）；新增 `scripts/build-exe.ps1`；新增 `.github/workflows/release.yml`；新增 `docs/install.md`；改 README 打包节 + docs/README.md 索引 + docs/gui-run.md 打包节。
- **风险与回滚：** CI 工作流无法在本机实测（YAML 按既有 ci.yml 模式编写，脚本与本地一致）；DMG 构建耗时长（sidecar 248MB + release 编译）。回滚：恢复 tauri.conf.json，删除新增文件。
- **方案确认：** [x] 已对照 P0/P1（含第七条/第九条）· 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**（正确设计的最小实现）
  1. `crates/hermes-gui/tauri.conf.json`：`bundle.targets` `["app"]` → `"all"`（Tauri 按宿主平台解析：macOS→app+dmg、Windows→msi+nsis+app）；`bundle.resources`（markitdown sidecar）从基配置移出。
  2. 新增 `crates/hermes-gui/tauri.macos.conf.json`：`bundle.resources` 移入（Tauri v2 平台配置自动合并），248MB macOS venv 只进 DMG、不进 Windows exe。
  3. 新增 `scripts/build-exe.ps1`：Windows 宿主上产出 NSIS `*-setup.exe`（npm build → `cargo tauri build --bundles nsis`）；无 Windows 宿主时明示走 CI；不含 sidecar（Windows 文档导入回落 data-dir/`HERMES_MARKITDOWN`）。
  4. 新增 `.github/workflows/release.yml`：`workflow_dispatch` + tag `v*` 触发；macos-14 → `build-dmg.sh` 产出 dmg；windows-latest → `build-exe.ps1` 产出 setup.exe；artifacts 上传；未签名（签名另立任务）。
  5. 新增 `docs/install.md`：用户拿到安装包后的完整操作（macOS/Windows 安装、未签名放行、三屏引导 + API Key 配置、数据位置与备份、卸载、升级、Windows 文档导入说明、FAQ）。
  6. 文档登记：`README.md`（新增「Package a Windows EXE」「After downloading an installer」）；`docs/README.md` 索引加 `install.md`；`docs/gui-run.md` 打包节补 build-exe.ps1 / 平台配置 / install.md。
- **关键路径/文件：** 上述 6 项；运行时（GUI/引擎）零改动。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 构建 macOS 安装包 | `scripts/build-dmg.sh` | 产出 dmg | 通过 | `target/release/bundle/dmg/lebi-AI_0.1.1_aarch64.dmg`（112M） |
| 2 | 安装包内含文档转换器 | 挂载 dmg → 检查 `Contents/Resources/resources/markitdown-sidecar` | sidecar 存在且可执行 | 通过 | 实测 `markitdown --version` → 0.1.6（sidecar 299M） |
| 3 | 平台配置合并生效 | dmg 内 Resources 检查 | 仅 macOS 捆绑 sidecar | 通过 | 基配置无 resources；macos 配置生效 |
| 4 | Windows 打包脚本存在且逻辑正确 | 静态审查 `build-exe.ps1` | 命令与产物路径明确 | 通过（未实跑） | 需 Windows 宿主/CI 验证，记录为遗留项 |
| 5 | CI release 工作流 | 静态审查 + YAML 解析 | 两 job、产物路径正确 | 通过（未实跑） | 记录为遗留项 |
| 6 | 用户指引完整 | 通读 `docs/install.md` | 覆盖安装/放行/配置/数据/FAQ | 通过 | 与首次引导三屏实现一致（`OnboardingRitual.tsx`） |

- **自动化：** `cargo clippy --workspace --all-targets -- -D warnings` ✅ 全绿；`cargo test --workspace` ✅ 全绿；`cd crates/hermes-gui/ui && npm run build` ✅ 通过（chunk 体积警告为存量）；tauri JSON/YAML 解析校验 ✅。
- **手工：** DMG 真机构建 + 挂载验证 ✅；Windows exe / CI 需 Windows 环境实跑（遗留项）。
- **测试结论：** [x] 全部通过 · [ ] 有已知问题（列出）：Windows 构建与 CI 未实跑（本机无 Windows），脚本与既有 `build-dmg.sh`/`ci.yml` 模式一致。

---

## 4. 验收（Accept）

对照**质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 拿到安装包的用户有清晰路径；Windows 有了官方打包路径 |
| 开箱即用未破坏 | ☑ | 安装包即开即用；WebView2 首次自动装；无新增运行时 |
| 本地优先未破坏 | ☑ | 数据仍本地明文；无新出站 |
| 测试通过 | ☑ | clippy/test/npm/DMG 实跑全绿；Windows/CI 静态审查 |
| 记录完整 | ☑ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ☑ | 见 0b/0c |
| 非修修补补（默认路径正确） | ☑ | 平台配置合并 + 标准脚本 + CI artifact + 用户文档，无特判 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | 无 Rust 代码改动；文档旧表述同步更新；无遗留双轨 |

- **验收人：** Codex Agent（用户待确认）
- **验收日期：** 2026-08-09
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** Windows 端实跑（本机 `build-exe.ps1` 或 CI `release.yml` 首次运行）；代码签名/公证（正式分发前）；`docs/install.md` 与 README 文案请用户确认。

---

## 5. 附注

- 实跑产物：`target/release/bundle/dmg/lebi-AI_0.1.1_aarch64.dmg`（112MB；内含 markitdown sidecar 299MB）。
- 未签名说明已写入 `docs/install.md`（macOS Gatekeeper / Windows SmartScreen 放行路径）。
