# 变更记录：文档导入（合规）— MarkItDown 捆绑默认路径 · 共享引擎 · GUI/Server 1:1

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-document-import-compliant` |
| **日期** | 2026-08-03 |
| **状态** | **已验收**（用户 2026-08-03：消息 attachments 路径正确） |
| **负责人** | Grok |
| **关联** | 否决稿 [`20260803-upload-phase-a-markitdown`](./20260803-upload-phase-a-markitdown.md)；P0/P1/AGENTS；方案确认 2026-08-03 |

---

## 0. 用户价值（必填 · 站在用户角度）

> 若写不出「用户因此更好用 / 更高效 / 更能进化」，**不得开工**。

- **谁用：** 个人用户 / 开发者，**桌面 GUI 主路径**；手机经 `hermes-server` 应对等能力。
- **解决什么痛点：** 无法把 Word/PDF/Excel 交给本地 Agent 稳定阅读；口头粘贴成本高。
- **用完后用户多得到什么：** 在聊天里导入文档 → 自动变成可读 Markdown → Agent 用 `read` 分析；**不必自己装 Python / Office**。
- **好用性自检：**
  - [x] 不需要用户额外装数据库 / 消息队列 / **Python**（转换器随产品分发或等价内置）
  - [x] 步骤可感知：导入中 / 成功 chip 或路径 / 失败原因
  - [x] 不增加无意义确认；失败明确，不静默丢文件
  - [x] 高频路径：GUI 点选或拖入 ≤2 步到「Agent 能读」

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在 GUI 对话中要分析合同/表格/说明 PDF；或 Flutter 用户经 server 上传同等文件。
- **路径变化：**
  - **改前：** 无导入；或仅有已否决的 GUI 独占 + 本机碰巧有 markitdown。
  - **改后：**
    1. 打开 GUI（`scripts/run-gui.sh` / 发布包）→ 当前会话
    2. 📎 或拖入 docx/pdf/xlsx/…（最小 MVP 至少「选择文件」按钮）
    3. 本地转换 → 仅 MD 进 workspace → 发送时带路径清单
    4. Agent `read` 后回答
    5. 手机端：对等 REST 导入，同数据目录格式
- **成功标准（可观察）：**
  1. **干净机器**（无用户自装 Python markitdown）上，发布路径或「捆绑资源就位」的开发路径下，导入 docx/xlsx/文本 PDF 成功，且 `uploads/` 中**只有** `.md` + `.meta.json`。
  2. 缺转换器时：明确错误，**不**产生假成功附件；设置或 `check` 可诊断。
  3. `hermes-server` 存在与 GUI **同语义**的导入 API；落盘路径规则一致。
  4. 用户无需打开终端完成导入（GUI）；开发者可用 CLI/`invoke` 仅作调试。
  5. `cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace` 全绿（或本变更引入的缺口已修平）。
- **明确不做什么（本变更边界）：**
  - 不做扫描 PDF OCR 保证
  - 不做「导入即写入记忆/知识库」
  - 不做完整 Composer 美学大改（可最小 📎；精美 chip/拖入可同 PR 最小实现或紧随的 B 切片，但**不得**无 GUI 入口就验收）
  - 不把 Azure Document Intelligence 当默认
  - 不保留「仅 PATH 碰运气」为默认

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  1. 缺少「文档 → 明文 Markdown」的**引擎级**能力与**可分发默认路径**；
  2. 初版把能力挂在 GUI 表面 + 外部解释器，违反单一引擎与开箱即用。
- **正确的长期默认路径：**
  ```
  用户文件 bytes
    → 共享库 DocumentImport（hermes-tools 或独立模块，非 skill）
    → 捆绑的 markitdown 可执行文件（应用资源 / 数据目录 bin）
    → workspace/uploads/{session_id}/{id}_{stem}.md + .meta.json
    → 删临时原件
    → GUI 与 server 仅薄封装同一函数
    → Agent 仅 read 相对路径（引擎能力 ①，非 SKILL）
  ```
- **与引擎/各入口边界：**
  | 层 | 职责 |
  |----|------|
  | **① 引擎能力** | `DocumentImport` 库：校验、落盘、调转换器、meta；**不是** SKILL.md |
  | **hermes-gui** | Tauri `check_document_converter` / `import_document` → 调库 |
  | **hermes-server** | `GET/POST /api/v1/uploads/...` **1:1 语义** → 调同一库 |
  | **CLI（可选）** | `hermes import-doc` 调试，同库 |
  | **Agent tools** | 仍用 `read`；**不**在本变更开放任意路径 `convert` 工具（缩小攻击面） |
- **默认路径稳定（反碰运气）：**
  1. 解析转换器顺序（**固定**）：
     1. 环境变量 `HERMES_MARKITDOWN`（仅开发覆盖）
     2. **应用捆绑路径**（Tauri resource / 安装目录 `resources/markitdown`）
     3. **数据目录** `~/.small-rust-hermes/bin/markitdown`（`hermes init` 或首次导入可安装的官方 sidecar 副本）
     4. **不再**把系统 PATH 的随机 `markitdown` 当作默认成功条件（可选：仅 debug 日志提示，正式 available=false 除非 1–3 命中）
  2. 发布流水线 / 文档规定：release 必须带 sidecar 或提供一键写入 `bin/` 的安装步骤，使「下载 Hermes 即可用」成立。
  3. GUI 默认仍 `ui/dist` + `scripts/run-gui.sh`。
- **安全影响：**
  - 只转换本进程写入的 temp；禁止用户任意本机路径直转（防读盘）
  - 子进程无 shell；超时；白名单扩展名；大小上限
  - 不把原件默认送第三方（markitdown 离线；禁止默认 Azure）
- **如何防复发：**
  - 台账 + server/GUI 同测；`check_document_converter` 测捆绑路径优先
  - AGENTS 自检 1/5/15：额外运行时、共享引擎、lint/test
  - 初版否决记录保留，禁止回退 PATH 默认
- **为何这不是补丁：** 一次收敛「转换 = 引擎能力 + 捆绑默认可执行 + 多入口契约」，而非 GUI 临时 spawn。

---

## 1. 方案（Plan）

### 1.1 目标

交付**合规**的文档导入：用户无装 Python；只存 MD；GUI 可完成；server 1:1；测试与台账达标。

### 1.2 范围

| 做 | 不做 |
|----|------|
| 共享 `DocumentImport` 模块 | OCR / 图片主路径 |
| 捆绑转换器解析顺序 + 探测 API | 知识库自动入库 |
| GUI + server 1:1 命令/路由 | Agent 通用 convert 工具 |
| 存储约定、错误码、meta | 旧 `.doc` 保证 |
| GUI 最小导入入口（选文件） | 与 PATH 碰运气并存为默认 |
| 全仓 clippy/test 门槛 | 完美 Composer 动效 |
| 清理初版 GUI 独占逻辑 | — |

### 1.3 用户路径变化

- 改前：无 / 不可验收的实验 IPC  
- 改后：GUI 选文件 → 转 MD → 聊；失败可读；手机 API 同构  

### 1.4 技术要点

#### 存储（沿用已验证约定）

```
{workspace}/
  uploads/{session_id}/{file_id}_{safe_stem}.md
  uploads/{session_id}/{file_id}_{safe_stem}.meta.json
  .upload_tmp/...   # 仅临时，成功删除
```

- 默认 **不保留原件**；失败不留假成功 md  
- frontmatter：`original_name` / `source_ext` / `converted_by`  
- 白名单：`pdf` `docx` `xlsx` `csv` `txt` `md`  
- 限制：20MB；超时 120s；空 MD → `empty_markdown`

#### 共享库 API（示意）

```rust
// 建议位置：crates/hermes-tools/src/document_import.rs
// 或 crates/hermes-convert（若 tools 过重再拆；优先 tools 内模块免新 crate）

pub struct ImportRequest { session_id, file_name, bytes, delete_original, ... }
pub struct ImportResult { file_id, md_rel_path, display_name, chars, ... }
pub struct ConverterStatus { available, binary_path, version, error }

pub fn check_converter(paths: &ConverterPathConfig) -> ConverterStatus;
pub fn import_document(workspace: &Path, cfg: &ConverterPathConfig, req: ImportRequest) -> Result<ImportResult, ImportError>;
```

#### IPC / HTTP 1:1

| GUI (Tauri) | Server (REST) |
|-------------|----------------|
| `check_document_converter` | `GET /api/v1/uploads/converter` |
| `import_document` | `POST /api/v1/uploads` body: sessionId, fileName, bytesBase64, deleteOriginal? |

DTO camelCase 与初版语义对齐，便于前端复用类型。

#### MarkItDown 捆绑策略（满足「无额外运行时」）

| 阶段 | 动作 |
|------|------|
| **开发** | `scripts/setup-markitdown-sidecar.sh`：用 `uv`/`pip` 在仓库或 `~/.small-rust-hermes/bin` 安装固定版本 markitdown\[docx,pdf,xlsx\]，**写入数据目录 bin**；`run-gui.sh` 可检测并提示跑 setup（非「碰巧 PATH」） |
| **发布** | `scripts/build-dmg.sh` / 打包步骤 **复制** 预构建 sidecar 到 app Resources；运行时优先 Resources |
| **运行时** | 仅按 0c 顺序解析；缺省 → available=false + 可操作错误文案（「运行 setup 脚本 / 重装应用」） |

版本钉扎：如 `markitdown==0.1.6` + extras，写入 meta.`converter_version`。

**诚实边界：** sidecar 内部仍是 Python 运行时，但**对用户不可见、不需用户安装**——与「用户装 Node 跑 Vite」不同，类比「应用自带 helper 二进制」。P0 清单「用户需要额外安装什么吗？→ 无」成立。

#### GUI 最小入口（验收必需）

- Chat `InputArea` 或设置旁：**一个 📎 选文件**（可先不拖入）
- 导入中禁用重复点选；失败 toast；成功加入 `pendingAttachments`，发送时拼：

```text
{用户输入}

[attachments]
- uploads/{session}/….md (original: …, N chars)
```

- 可选一句 system 提示：对 `uploads/` 路径先 `read`（`context.rs` 小改）

#### 初版代码清理（P0 第九条）

- 删除或搬空 `hermes-gui` 内重复实现，改为调共享库  
- 命令名可统一为 `check_document_converter` / `import_document`（若改名同步删旧 export）  
- 不留「仅 PATH」注释当默认文档

### 1.5 风险与回滚

| 风险 | 缓解 |
|------|------|
| sidecar 体积大 | 仅 docs 相关 extras；发布说明体积 |
| macOS 公证 / 可执行权限 | 打包脚本 chmod + 签名清单 |
| 扫描 PDF 空 | empty_markdown，产品文案 |
| workspace clippy 既有债 | 本变更必须修到全绿或单独立项 blocker 不可验收 |
| 回滚 | feature 可关：无 sidecar 时导入失败不影响纯文本聊天 |

### 1.6 方案确认

- [x] 已对照 P0/P1（含第七条、第九条、开箱即用、server 1:1）· 日期/人：2026-08-03 / 用户「方案确认」  
- [x] 用户确认本方案后再实施  

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. 共享库 `hermes_tools::document_import`：校验、落盘、调 sidecar、meta；解析序 `HERMES_MARKITDOWN` → bundled → `~/.small-rust-hermes/bin/markitdown`（**不用 PATH 作默认成功**）
  2. `scripts/setup-markitdown-sidecar.sh` 安装固定版本到数据目录 bin
  3. GUI 薄封装 `check_document_converter` / `import_document`；删除初版内联转换逻辑
  4. Server：`GET /api/v1/uploads/converter`、`POST /api/v1/uploads`（auth 层内）
  5. GUI `InputArea` 📎 + chip + 发送拼 `[attachments]` 路径清单
  6. `context.rs` 增加 uploads 先 `read` 提示
  7. 修 workspace clippy 债（unused imports、session 测试、ensure_writer）
- **关键路径/文件：**
  - `crates/hermes-tools/src/document_import.rs`
  - `crates/hermes-gui/src/commands/upload.rs`（薄）
  - `crates/hermes-server/src/routes/uploads.rs`
  - `crates/hermes-gui/ui/src/components/chat/InputArea.tsx`
  - `scripts/setup-markitdown-sidecar.sh`
- **偏离方案处：** 发布包 Resources 捆绑尚未进 `build-dmg.sh`（开发默认定数据目录 setup；发布捆绑为遗留 Phase C 打包项）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 数据目录 sidecar 可用 | setup 脚本后 check | available | **通过** | 自动化 csv e2e |
| 2 | 导入 txt | import_document | md+meta | **通过** | unit |
| 3 | 导入 csv（markitdown） | data bin | 含表格内容 | **通过** | unit e2e |
| 4 | 拒绝 png | import | unsupported_type | **通过** | unit |
| 5 | 过大文件 | >20MB | too_large | **通过** | unit |
| 6 | 无转换器路径 | 假路径 cfg | available=false | **通过** | unit |
| 7 | server 路由编译 | clippy hermes-server | 通过 | **通过** | 编译级 |
| 8 | 发送带附件 | GUI 手测 | 消息含路径 | **通过** | 用户确认 attachments 清单 |
| 9 | workspace clippy/test | 全仓 | 全绿 | **通过** | 见下 |
| 10 | 真实 .doc 判决书 | 包某…doc | MD 1841 chars 可读 | **通过** | HTML-in-OLE |

- **自动化：**
  - `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
  - `cargo test --workspace` → 全绿
  - `cargo test -p hermes-tools document_import` → 通过（含真实 .doc）
  - `npx tsc --noEmit`（gui ui）→ exit 0
- **手工：** GUI 📎 导入 .doc → 消息 attachments 正确；磁盘 MD 正文可读 → **通过**
- **测试结论：** [x] 全部通过

---

## 4. 验收（Accept）

对照 **质量门槛**（`DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | GUI 导入 → MD → 消息可引用 → Agent 可 read |
| 开箱即用未破坏 | ☑ | setup → 数据目录 bin；非 PATH 默认（发布捆绑仍遗留） |
| 本地优先未破坏 | ☑ | 离线转换 + workspace 明文 MD |
| 测试通过 | ☑ | 自动化全绿 + 用户真机 .doc 通过 |
| 记录完整 | ☑ | 本文件 + README 索引 |
| 产品+架构两视角齐全 | ☑ | §0b/0c |
| 非修修补补（默认路径正确） | ☑ | 共享库 + 解析序 + server 1:1 + .doc HTML-in-OLE |
| 代码卫生 | ☑ | GUI 独占逻辑已收敛 |

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过
- **遗留项：**
  1. ~~发布包将 markitdown 打入 app Resources~~ → 见 [`20260803-markitdown-release-bundle`](./20260803-markitdown-release-bundle.md)（待验收）
  2. 拖入 / 气泡附件卡精修（体验增强，非本变更必验）

---

## 5. 附注

### 5.1 与否决稿差异摘要

| 点 | 否决稿 | 本方案 |
|----|--------|--------|
| 转换器 | PATH / 环境碰巧 | 捆绑 + 数据目录 bin，固定解析序 |
| 代码位置 | 仅 hermes-gui | 共享库 + GUI + server |
| 验收 | 局部单测 + invoke | GUI 入口 + 全仓门槛 + 台账四阶段 |
| 用户装 Python | 隐含需要 | **禁止作为默认** |

### 5.2 P0 决策清单自检（方案阶段）

1. [x] 用户额外安装？→ **无**（sidecar 随应用/setup）  
2. [x] 非技术用户 GUI？→ **有最小 📎**  
3. [x] 更好用？→ 文档可直接问 Agent  
4. [x] 数据本地明文？→ MD + meta  
5. [x] 默认不外传？→ 离线 markitdown  
6. [x] 共享引擎？→ 是  
7. [x] server 1:1？→ 是  
15. [ ] lint/test 全绿 → **实施后勾选**

### 5.3 实施前请用户确认的 4 点

1. 默认仍 **只存 MD、删原件**（设置项可后置）  
2. 开发机通过 **`scripts/setup-markitdown-sidecar.sh`** 写入 `~/.small-rust-hermes/bin`，**不**再依赖 PATH 作为默认  
3. 本变更 **必须含 GUI 最小 📎**，否则不验收  
4. workspace clippy 既有 unused import 等债 **纳入本变更修绿**（或先单独修绿再合入）  

确认后将 §1.6 勾选并进入 §2 实施。
