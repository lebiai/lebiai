# 变更记录：GUI 默认走 ui/dist（去掉 5173）防白屏 + 权威文档锁定

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-gui-dist-default-no-white-screen` |
| **日期** | 2026-08-03 |
| **状态** | **待验收**（需确认窗口非白屏） |
| **负责人** | Grok（用户委托） |
| **关联** | 用户反馈白屏；律师版 `tauri.conf` 无 devUrl；P0 禁止环境运气依赖 |

---

## 0. 用户价值

- **谁用：** 打开桌面 Hermes 的协作者 / 最终用户路径上的开发验证
- **痛点：** `cargo run -p hermes-gui` 整页白屏，误以为「没 build」；根因是 debug 连未启动的 Vite :5173
- **用完后：** 默认一条命令可开有界面的 GUI；文档写死路径，Agent 不再误开白屏

---

## 0b. 产品经理视角

- **场景：** 开发/验收要打开 GUI
- **路径：** 改前（碰 5173 或忘记 build）→ 改后（`scripts/run-gui.sh` / dist 默认）
- **成功标准：** 启动后可见侧栏与主界面，非空白 WebView
- **不做什么：** 不为 HMR 永久打开 devUrl；不改产品业务功能

---

## 0c. 架构师视角

- **根因：** `tauri.conf.json` 配置 `devUrl: http://localhost:5173` 时，debug 构建加载开发服务器而非 `frontendDist`
- **正确默认路径：** 始终 `frontendDist: ./ui/dist`；打包 `beforeBuildCommand` 构建前端；`build.rs` 在 dist 缺失时自动 npm build；脚本 `run-gui.sh` 为协作入口
- **为何不是补丁：** 删掉错误默认（5173），与律师版及 P0「禁止环境运气」一致，而非遮白屏

---

## 1. 方案

- 对齐律师版：去掉 devUrl；beforeBuildCommand = npm run build
- 脚本 + 权威文档（AGENTS / DEVELOPMENT_RULES / PRODUCT 一句 / README）
- 非权威 `docs/gui-run.md` 细节

---

## 2. 实施

- `crates/hermes-gui/tauri.conf.json`：删除 devUrl/beforeDevCommand；加 beforeBuildCommand
- `crates/hermes-gui/build.rs`：dist 缺失则 npm install/build
- `scripts/run-gui.sh`
- `AGENTS.md`、`DEVELOPMENT_RULES.md` §六·附 C、`PRODUCT_PRINCIPLES` 默认路径句、`README`、`docs/gui-run.md`、索引

---

## 3. 测试

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | `scripts/run-gui.sh` | 窗口有 UI | 待手测 |
| 2 | 配置无 devUrl | 不请求 5173 | 代码已改 |
| 3 | dist 存在时 cargo run | 加载 dist | 待手测 |

- **自动化：** 配置 diff 审阅
- **手工：** 启动 GUI 目视

---

## 4. 验收

待用户确认非白屏后勾选。
