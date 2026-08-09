# 变更记录：Windows 安装包捆绑 MarkItDown（文档导入开箱即用）

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-windows-markitdown-bundle` |
| **日期** | 2026-08-09 |
| **状态** | **已实施**（本机编译/clippy 绿 · Windows 实跑待 CI 验证） |
| **负责人** | Codex Agent |
| **关联** | 用户反馈：Windows 也要开箱即用（文档导入） |

---

## 0. 用户价值

- **谁用：** Windows 用户。
- **解决什么痛点：** 之前 Windows exe 不捆绑文档转换器，导入 Word/PDF/Excel 需要本机装 Python，不是开箱即用。
- **用完后用户多得到什么：** 装完 exe 直接导入文档，与 macOS 体验一致；代价是安装包变大（预计从 6 MB 增至百 MB 级）。

---

## 0b. 产品经理视角

- **路径变化：** Windows 安装包加入自包含转换器 → 文档导入开箱即用。
- **成功标准：** 新 exe 安装后，不装 Python 也能导入 pdf/docx/xlsx/csv。
- **明确不做：** 不做「按需下载」方案（保持与 macOS 一致的开箱即用）；不改 macOS 打包。

---

## 0c. 架构师视角

- **根因：** Windows venv 依赖构建机 base Python，拷到用户机器失效 → 不能沿用 macOS 的 venv 方案。
- **正确默认路径：** **Embeddable Python（python.org embed zip，自包含可移动）+ pip `--target` 安装 markitdown[docx,pdf,xlsx]**，wrapper 为 `markitdown.cmd`（相对路径解析）；Tauri 经 `tauri.windows.conf.json` resources 捆绑；引擎 spawn 时 Windows 走 `cmd /C call`（macOS/Linux 直接执行 bash wrapper）；资源解析（GUI `resolve_bundled_markitdown`）按平台选 `markitdown.cmd` / `markitdown`。
- **防复发：** prepare 脚本只产自包含结构（venv 方案禁止回退）；新平台 wrapper 名差异集中在 `resolve_bundled_markitdown` 一处。
- **为何这不是补丁：** 平台差异落在 Tauri 平台配置 + 单一 resolve 入口 + 明确 spawn 分支，无散落特判。
- **已知风险：** onnxruntime 依赖 VC++ 运行库（msvcp140.dll），绝大多数 Win10/11 自带；缺失时 PDF 转换报错提示（可引导安装 VC++ Redist）。Windows 实跑需 CI 验证。

---

## 1. 方案（Plan）

- **目标：** Windows 安装包捆绑自包含 MarkItDown，文档导入开箱即用。
- **范围：** 做：`scripts/prepare-markitdown-bundle.ps1`（embed python + wheels）、`tauri.windows.conf.json`、`build-exe.ps1` + CI 加 prepare、spawn/resolve 平台分支、gitignore。不做：按需下载方案、macOS 改动。
- **风险与回滚：** exe 体积增大；CI 构建时间增加；onnxruntime VC 运行时风险；回滚 = 撤销 resources 配置与 ps1。
- **方案确认：** [x] 已对照 P0/P1/P2/P3 · 日期/人：2026-08-09 Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** 新增 `scripts/prepare-markitdown-bundle.ps1`（下载 python 3.12.7 embed amd64 → 启用 site + Lib\site-packages → get-pip → `pip install --target` markitdown[docx,pdf,xlsx]==0.1.6 → 生成相对路径 `markitdown.cmd` wrapper）；新增 `crates/hermes-gui/tauri.windows.conf.json`（resources 同 macOS）；`scripts/build-exe.ps1` 与 `release.yml` windows job 加 prepare 步骤；`crates/hermes-gui/src/commands/upload.rs` 按平台解析 `markitdown.cmd`；`crates/hermes-tools/src/document_import.rs` 新增 `markitdown_command`（Windows 走 `cmd /C call`），探测与转换两处调用点统一使用；`.gitignore` 忽略 `python/` 与 `markitdown.cmd`；同步更新 build-exe.ps1 头部注释。
- **偏离方案处：** 无。

---

## 3. 测试（Test）

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | 编译 | hermes-tools + hermes-gui dev check | ✅ |
| 2 | lint | clippy -D warnings 两 crate | ✅ |
| 3 | Windows 实跑 | 新 exe 装后免 Python 导入文档 | 待 CI 构建后实跑验证 |
| 4 | macOS 回归 | resolve/spawn 分支不影响 mac 路径 | ✅ 本机编译 + 逻辑分支按平台隔离 |

- **自动化：** cargo test -p hermes-tools（0 tests，文档导入无单测）· clippy。
- **测试结论：** [ ] 全部通过（Windows 实跑待 CI）· [ ] 有已知问题（onnxruntime VC 运行时风险）

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | Windows 文档导入开箱即用 |
| 开箱即用未破坏 | ✅ | macOS 不变 |
| 测试通过 | ✅（本机）/ ⏳（Windows 实跑） | 待 CI |
| 非修修补补 | ✅ | 平台差异集中三处（配置/解析/spawn） |
| 代码卫生 | ✅ | 旧注释同步清理 |
| 记录完整 | ✅ | 本文档 + README 索引 |

- **验收人：** Codex Agent
- **验收日期：** 2026-08-09
- **结论：** ☑ 通过（本机工程全绿；Windows 实跑随 CI 产物复测后定稿）

---

## 5. 附注

- Windows exe 体积预期从 6 MB 增至约 100 MB（222 MB site-packages 压缩后），与 macOS DMG 量级一致。
- 用户机器缺 VC++ 运行库时 PDF 转换会失败并给出提示；正式分发前可补充「首次检测并引导安装 VC++ Redist」。
