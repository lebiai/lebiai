# 应用内更新：发版签名

| 字段 | 内容 |
|------|------|
| **种类** | **E** 开发者手册（非权威） |
| **关联** | [`../spec/in-app-update.md`](../spec/in-app-update.md) |

更新器签名 ≠ 苹果公证 / Windows Authenticode。这一对密钥只证明「这个更新包是我们打的」。

## 密钥在哪

本机（已生成，不进 git）：

```
~/.tauri/lebi-ai.key        # 私钥
~/.tauri/lebi-ai.key.pub    # 公钥（已写入 tauri.conf.json）
~/.tauri/lebi-ai.key.pass   # 私钥口令
```

GitHub Actions 需要两个 repository secrets：

| Secret | 来源 |
|--------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | `~/.tauri/lebi-ai.key` 全文 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `~/.tauri/lebi-ai.key.pass` 全文 |

不要把私钥贴进 issue、聊天或仓库。丢了私钥或口令，已装的带更新器版本无法再走应用内更新。

## 写入 secrets（本机已登录 `gh`）

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/lebi-ai.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ~/.tauri/lebi-ai.key.pass
```

或在 GitHub → Settings → Secrets and variables → Actions 里手贴。

## 本地打安装包

`scripts/build-dmg.sh` / `scripts/build-exe.ps1` 会自动读 `~/.tauri/lebi-ai.key`。没有这份文件、又没设环境变量，打包会失败。

`scripts/run-gui.sh` / `cargo run -p hermes-gui` **不需要**密钥。
