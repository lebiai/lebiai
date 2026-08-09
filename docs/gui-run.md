# 桌面 GUI：如何打开（防白屏）

| 字段 | 内容 |
|------|------|
| **定位** | 操作说明；**非** P0/P1。规则以 `AGENTS.md` / `DEVELOPMENT_RULES.md` §六·附 C 为准。 |
| **权威约束** | 默认加载 `ui/dist`；禁止默认依赖 `localhost:5173` |

---

## 默认路径（必须）

```bash
# 仓库根
scripts/run-gui.sh
```

等价步骤：

```bash
cd crates/hermes-gui/ui
npm install          # 首次
npm run build        # 产出 dist/
cd ../../..
cargo run -p hermes-gui
```

`build.rs` 在 **dist 缺失** 时会尝试自动 `npm run build`；**改过前端源码**仍应显式 rebuild，否则可能仍是旧界面。

---

## 白屏根因

| 现象 | 原因 |
|------|------|
| 整页白 | 曾配置 `devUrl: http://localhost:5173` 时，**debug** `cargo run` 去连 Vite；5173 未开 → 空白 |
| 仍是旧 UI | 改了 `ui/src` 但未 `npm run build` |

**正确设计（已落地）：** `tauri.conf.json` **不设** `devUrl`，始终 `frontendDist: ./ui/dist`。

---

## 可选：热更新（HMR）

仅本地改样式/前端时可选，**不得**写进默认配置或用户文档主路径：

1. 终端 A：`cd crates/hermes-gui/ui && npm run dev`
2. 临时在 `tauri.conf.json` 加回 `devUrl` + `beforeDevCommand`（用完删掉），并用 `cargo tauri dev`
3. 或继续默认 dist 路径：改一点 build 一次（更稳）

---

## 打包

- `scripts/build-dmg.sh`：内部会 `npm run build`
- `scripts/build-exe.ps1`：Windows NSIS 安装包（需 Windows 宿主；或走 `.github/workflows/release.yml` CI）
- `tauri.conf.json` → `beforeBuildCommand`: `npm run build`（cwd `ui`）
- `tauri.macos.conf.json` → 仅 macOS 捆绑 markitdown sidecar（`bundle.resources`）
- 用户拿到安装包后的操作：见 [`docs/install.md`](./install.md)
