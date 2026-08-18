# 应用内更新（点击即更）

| 字段 | 内容 |
|------|------|
| **版本** | v1.0 |
| **日期** | 2026-08-18 |
| **种类** | **D** 操作规格（非权威） |
| **效力** | 只规定桌面 GUI 的版本检查与点击更新。冲突以 [`../../PRODUCT_PRINCIPLES.md`](../../PRODUCT_PRINCIPLES.md) **P0 v0.11** 为准。不得扩大产品定义。 |
| **状态** | **实施中**（1.2.0 为带更新器的首包；点「更新」待再发一版。台账 [`../records/20260818-in-app-update.md`](../records/20260818-in-app-update.md)） |
| **关联** | [`settings-ia.md`](./settings-ia.md) · [`../guide/install.md`](../guide/install.md) · [`../releases/release-notes.md`](../releases/release-notes.md) |

---

## 0. 用户价值

用户在设置里看见「有新版本」，点一下「更新」，软件自己下、自己装、自己重启。不用离开应用，不用自己找安装包。

---

## 1. 已拍板决策

| # | 议题 | 结论 |
|---|------|------|
| **1** | 触发 | **只点才更。** 不后台静默安装，不启动弹窗逼更。 |
| **2** | 入口 | **设置 · 概览** 一行状态。不新开第 6 个 Tab，不塞进对话页。 |
| **3** | 点完之后 | **同一按钮路径走完**：下载 → 校验签名 → 安装 → 重启。进行中只显示进度。 |
| **4** | 文件从哪来 | GitHub Releases 只当仓库。用户界面不出现「去 GitHub」。 |
| **5** | 从哪一版起 | **下一版桌面包带上更新器。** 那一版之后，再发新包，概览才能点到「更新」。这是机制，不是过渡方案。 |

---

## 2. 用户怎么走完

只发生在**桌面 GUI**。对话、在办、回顾、授权都不被打断，除非用户自己点了「更新」并进入重启。

### 2.1 看见

打开 **设置**（默认落在 **概览**）。状态卡里，授权 / 对话就绪 / 微信之下，多一行 **版本**。

| 态 | 色 | 标题 | 副文 | 操作 |
|----|----|------|------|------|
| 正在检查 | 中性 | 正在检查更新… | 当前 x.y.z | 无 |
| 已是最新 | 成功 | 已是最新 | 当前 x.y.z | 「检查更新」（弱文字链） |
| 有新版本 | 警告 | 有新版本 x.y.z | 一句说明（`latest.json` 的 `notes`，没有就省略） | 主按钮 **更新** |
| 下载中 | 警告 | 正在更新… | 已下载 n% | 按钮禁用 |
| 安装中 | 警告 | 正在安装… | 装完会重启 | 按钮禁用 |
| 失败 | 危险 | 这次没更新成 | 人话原因（网络 / 校验失败 / 写不进应用目录） | **重试** |
| 开发态 | 中性 | 开发中的版本 | 当前 x.y.z · 开发运行不走安装更新 | 无按钮 |

缺 API Key 时，**去配置模型**仍是概览唯一强 CTA。版本行可以是警告，但不能压过配 Key。

### 2.2 点「更新」

1. 用户点 **更新**。
2. 行变成「正在更新…」，显示百分比；期间不能再点一次。
3. 下完并校验通过后自动安装。
4. 安装成功后**自动重启**进新版本。设置仍在概览，版本行变为「已是最新」。
5. 数据目录不动（`~/.lebi-ai` / `%USERPROFILE%\.lebi-ai`）。会话、在办、记忆、授权原样。

失败停在概览这一行，不弹系统对话框，不打开浏览器。

### 2.3 不检查的时候

- 进对话、写在办、看回顾：**不**弹更新。
- 启动：**不**弹窗。
- 未打开设置：不打扰。允许进程里静默问一次「有没有新版本」，只为概览打开时立刻能画出对的态；问的结果不弹层。

---

## 3. 看起来怎么样

- 复用概览已有 `StatusRow`：左色条 + 标题 + 副文 + 右侧一个操作。
- 文案短、中文、不出现 GitHub / Release / artifact / JSON。
- 进度用副文百分比，不加第二条进度条、不加全屏遮罩。
- Windows 安装器允许系统自带一个很小的被动进度窗（`installMode: passive`），应用内进度仍为主。
- 空 / 载 / 错三态都在这一行里结束，不另开页。

---

## 4. 架构（正确默认路径）

根因：现在的安装包只是「下一个安装包」，应用自己不知道、也不会换自己。

默认路径：

```
发版 CI
  → 打出用户安装包（dmg / nsis）
  → 同时打出更新器包（macOS .app.tar.gz + .sig；Windows setup.exe + .sig）
  → 写出 latest.json 挂到该次 GitHub Release
桌面应用（已带更新器的版本）
  → 打开设置概览时 GET latest.json
  → 版本更新 → 用户点「更新」
  → 按下包 URL 下载 → 用内置公钥验签 → 安装 → 重启
```

| 项 | 选定 |
|----|------|
| 实现 | Tauri 2 `tauri-plugin-updater` + `@tauri-apps/plugin-updater`；重启用 `tauri-plugin-process` 的 `relaunch` |
| 内置对话框 | **关**（`dialog: false`）。界面只走设置概览 |
| 清单 | 静态 JSON，不自建更新服务器 |
| 地址 | `https://github.com/lebiai/lebiai/releases/latest/download/latest.json` |
| 验签 | 更新器专用密钥。公钥写进 `tauri.conf.json`；私钥只放本机与 GitHub Actions secrets。**丢了私钥就发不出后续应用内更新** |
| 和苹果/微软代码签名 | 两回事。更新器签名管「这包是不是我们打的」；Gatekeeper / SmartScreen 仍按 [`../guide/install.md`](../guide/install.md) |
| CSP | 下载走 Rust 插件，不改 webview 的 `connect-src` |
| 平台 | 只做当前 CI 已出的：**darwin-aarch64**、**windows-x86_64**。不编造没打的包 |
| 非桌面 | CLI / `hermes-server` / Flutter / IM：**不做** |

`latest.json` 形状（字段名服从 Tauri，URL 随版本变）：

```json
{
  "version": "x.y.z",
  "notes": "一句人话",
  "pub_date": "2026-08-18T00:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "<.sig 文件原文>",
      "url": "https://github.com/lebiai/lebiai/releases/download/vX.Y.Z/lebi-AI.app.tar.gz"
    },
    "windows-x86_64": {
      "signature": "<.sig 文件原文>",
      "url": "https://github.com/lebiai/lebiai/releases/download/vX.Y.Z/lebi-AI_X.Y.Z_x64-setup.exe"
    }
  }
}
```

签名校验失败 → **拒绝安装**（fail-closed）。

---

## 5. 发版链要改什么

沿用现有 `.github/workflows/release.yml`（tag `v*` → macOS job + Windows job → publish 删旧再建）。增量：

1. 两个构建 job 注入 secrets：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
2. `tauri.conf.json`：`bundle.createUpdaterArtifacts: true`，以及 `plugins.updater`（pubkey + endpoints）。
3. macOS 额外上传 `target/release/bundle/macos/lebi-AI.app.tar.gz` 与 `.sig`（dmg 仍上传，给人手第一次安装）。
4. Windows 额外上传 `*-setup.exe.sig`（exe 已有）。
5. publish 在挂 Release 前用两个 `.sig` + 可预期的资源 URL 写出 `latest.json`，一并挂上。

第一次安装仍用 dmg / exe。应用内更新走 `.app.tar.gz`（macOS）和已签名的 setup.exe（Windows）。

---

## 6. 明确不做什么

- 不静默更新、不强制更新、不启动弹窗
- 不把用户送到浏览器下包（失败也不）
- 不自建更新服务器、不用 gist、不在仓库主分支另挂一份会漂的 `latest.json`
- 不新开设置 Tab，不在对话页做横幅
- 不做 Intel mac / Linux 桌面（CI 没打就不写进 `platforms`）
- 不做手机 / CLI / IM 自更新
- 不把更新器私钥、Apple 证书、Microsoft 证书写进 git
- 不在本方案里做苹果公证 / Windows Authenticode（那是安装指引里已有的分发后续）

---

## 7. 成功标准

1. 带更新器的版本打开设置概览，≤2 秒内版本行有明确态（检查中 / 已最新 / 有新版本 / 失败）。
2. 有新版本时，用户只点一次「更新」，无需离开应用，重启后版本号已变。
3. 数据目录内容更新前后一致。
4. 签名不对的包装不进去。
5. 用户能用一句话复述：设置里看见新版本，点更新，等它重启。
6. 界面任何一态都不出现「GitHub」。

---

## 8. 实施分期

| 阶段 | 内容 |
|------|------|
| **U0** | 生成本地更新器密钥；公钥进仓库；私钥进 Actions secrets。不发版 |
| **U1** | GUI 接入插件 + 概览版本行（检查 / 点更 / 进度 / 失败） |
| **U2** | CI 出更新器产物 + `latest.json`；改 `install.md` §7 |
| **U3** | 打下一版桌面包。再发一版后，用真机点一次「更新」验收 |

未到 U3 真机点通，不得把「应用内更新」写进用户向发布说明当已交付。
