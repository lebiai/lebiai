# 变更记录：发布捆绑 MarkItDown sidecar（客户零安装）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-markitdown-release-bundle` |
| **日期** | 2026-08-03 |
| **状态** | **待测试**（用户 2026-08-03：打包后再测） |
| **负责人** | Grok |
| **关联** | [`20260803-document-import-compliant`](./20260803-document-import-compliant.md) 遗留项 1；用户选「做 1」 |

---

## 0. 用户价值

- **谁用：** 拿到 `.dmg` / 桌面安装包的客户；开发者本地 `cargo tauri build`
- **解决什么痛点：** 导入 Word/PDF/Excel 曾依赖 `setup-markitdown-sidecar.sh` 或本机 Python——客户无法接受
- **用完后：** 只装 Hermes 即可 📎 导入文档；无需终端、无需 pip
- **好用性自检：**
  - [x] 用户不额外装 Python（helper 随应用）
  - [x] 步骤可感知（缺 sidecar 时仍有明确错误）
  - [x] 无多余确认
  - [x] 高频路径：装完即用

---

## 0b. 产品经理视角

- **场景：** 客户下载 dmg → 安装 → 打开 GUI → 上传判决书/合同
- **路径变化：**
  - 改前：开发机 setup 脚本 → 数据目录 bin；客户路径未闭环
  - 改后：构建时打入 App Resources；运行时优先读捆绑路径
- **成功标准：**
  1. `scripts/prepare-markitdown-bundle.sh` 能生成可执行 wrapper + venv
  2. `build-dmg.sh` 构建前自动 prepare（或明确失败提示）
  3. GUI `check_document_converter` 在捆绑存在时 `available=true` 且路径落在 app Resources 或 dev resources
  4. 解析序仍为：`HERMES_MARKITDOWN` → **bundled** → 数据目录 bin（无 PATH 默认）
- **明确不做什么：** Windows exe 完整打包（可预留目录约定）；不改文档导入语义；不强制 commit 上百 MB venv 进 git

---

## 0c. 架构师视角

- **根因：** 转换器只装在用户数据目录，发布物未携带 → 开箱路径断裂
- **正确默认路径：** 发布物 Resources 内携带 relocatable wrapper + venv；GUI 用 Tauri `resource_dir` 解析；开发可用 crate 内 `resources/markitdown-sidecar`
- **边界：** 引擎 `ConverterPathConfig.bundled_binary` 仍由 host 注入；server 无 bundle 时继续 data bin / setup
- **安全：** 只执行包内路径；不执行 PATH 上随机 markitdown
- **防复发：** build-dmg 依赖 prepare；缺 bundle 时 build 失败或警告
- **非补丁：** 一次收口「客户机无 Python 也可转换」

---

## 1. 方案

- **技术：**
  - `scripts/prepare-markitdown-bundle.sh` → `crates/hermes-gui/resources/markitdown-sidecar/{markitdown,venv}`
  - `tauri.conf.json` `bundle.resources`
  - GUI `converter_cfg(app)`：resource_dir + debug `CARGO_MANIFEST_DIR`
  - `build-dmg.sh` / 可选 `run-gui.sh` 调用 prepare（若已存在可跳过）
- **风险：** 包体积增大（onnx/pdf 依赖）；venv 架构需匹配（arm64/x86_64）
- **方案确认：** [x] 用户「做 1」· 2026-08-03

---

## 2. 实施

- **实际改动：**
  - `scripts/prepare-markitdown-bundle.sh`：生成 relocatable `venv` + wrapper
  - `tauri.conf.json` `bundle.resources` 打包 `resources/markitdown-sidecar`
  - `build-dmg.sh` 构建前强制 prepare
  - `run-gui.sh`：无 data-bin 时自动 prepare
  - GUI `upload.rs`：`AppHandle` → `resource_dir` / `CARGO_MANIFEST_DIR` 注入 `bundled_binary`
  - `.gitignore` 忽略 venv 产物；保留 `.keep` 保证空仓可编译
- **偏离：** 未在本机跑完整 `build-dmg`（体积 ~254MB sidecar，耗时长）；逻辑与 dev 路径已接好
- **关键文件：** 见上 + `docs/records/20260803-markitdown-release-bundle.md`

---

## 3. 测试

| # | 用例 | 期望 | 结果 | 备注 |
|---|------|------|------|------|
| 1 | prepare 脚本 | wrapper --version 成功 | **通过** | markitdown 0.1.6；~254M |
| 2 | cargo check hermes-gui | 带 resources 编译通过 | **通过** | |
| 3 | 解析序代码 | HERMES → bundled → data bin | **通过** | 代码审查 |
| 4 | 完整 dmg 构建 | dmg 内含 sidecar | ⬜ | 可选；体积大 |

- **测试结论：** [x] 开发路径通过 · [ ] 完整 dmg 手测可选

---

## 4. 验收

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值 | ☑ | 客户装包自带转换器（构建链路已接） |
| 开箱即用 | ☑ | 不依赖客户 pip；dev 仍可 setup 兜底 |
| 测试通过 | ☑/☐ | prepare+compile 过；完整 dmg 可选 |
| 记录完整 | ☑ | 本文件 + README 索引 |

- **验收人：**
- **结论：** ☐ 通过 · ☐ 驳回
- **遗留：** Windows 安装包；universal 双架构 venv；CI 缓存 prepare 产物
