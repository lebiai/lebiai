# 授权功能：怎么测 + 怎么日常发码

> **种类：** E 开发者手册（非权威）。产品事实以 P0 v0.9 为准。交互规格见 [`../spec/license-ux.md`](../spec/license-ux.md)。

实现台账见 [`../records/20260811-license-impl.md`](../records/20260811-license-impl.md)。

---

## 1. 日常发码（推荐）

### 方式 A：HTML 签发页（你要的常态化）

1. 用浏览器打开本地文件：

```text
newdata/small-rust-hermes/scripts/license-issuer.html
```

macOS 可：

```bash
open newdata/small-rust-hermes/scripts/license-issuer.html
```

2. 选时长（1 月 / 季度 / 年）→ 填备注（客户/订单）→ **生成授权码** → **复制**  
3. 发给客户，让他在 App **设置 → 账户 → 授权** 粘贴。

> 首次打开需**能访问外网**一次（加载浏览器签名库 esm.sh）。  
> **不要把该 HTML 发给客户或挂公网**（内含签发密钥）。

### 方式 B：命令行

```bash
cd newdata/small-rust-hermes
pip install pynacl
python3 scripts/issue-license.py --days 365 --plan year
```

---

## 2. 端到端测试清单

### 准备

```bash
cd newdata/small-rust-hermes
./scripts/run-gui.sh
```

数据文件：`~/.lebi-ai/license.json`

### T1 · 试用

| 步骤 | 期望 |
|------|------|
| 删除 `license.json` 后启动（或新数据目录） | 侧栏电池显示试用 |
| 设置 → 授权 | 可见试用剩余、到期日 |
| 能正常对话（有 API Key 时） | 成功 |

### T2 · 激活正式授权

| 步骤 | 期望 |
|------|------|
| HTML 生成 **30 天**码 | 得到 `LEBI1....` |
| 设置里粘贴 → 激活 | Toast 显示可用至某日；电池变「已授权」 |

### T3 · 临期（≤3 天）

| 步骤 | 期望 |
|------|------|
| 生成 **1 天或 2 天**码并激活 | 电池警告色 |
| 重启或切到后台再回前台 | 约 1s 后可能出现续费弹窗（当天只一次） |
| 点「稍后提醒」 | 关掉；当天不再弹 |
| 再贴更长码 | 弹窗逻辑结束 |

### T4 · 过期全屏锁

1. **完全退出** App  
2. 编辑 `~/.lebi-ai/license.json`，例如：

```json
{
  "trialStartedAt": "2026-08-01T00:00:00+00:00",
  "token": null
}
```

（`trialStartedAt` 须早于现在至少 3 天）

3. 启动 App  

| 期望 |
|------|
| 全屏不可关：品牌 + slogan + 输入框 + 微信 iodine001 |
| 无法发对话 |
| 粘贴有效码后全屏消失，可继续用 |

### T5 · 误贴旧码

| 步骤 | 期望 |
|------|------|
| 当前已有 30 天授权，再贴只剩 1 天的码 | 提示「到期日更早，未更换」 |

### T6 · 续期

| 步骤 | 期望 |
|------|------|
| 再生成 365 天码粘贴 | 同一按钮；成功；到期日变长 |

---

## 3. 调试命令速查

```bash
# 看当前授权状态文件
cat ~/.lebi-ai/license.json | python3 -m json.tool

# 重置试用
rm ~/.lebi-ai/license.json

# 命令行发 1 天测试码
python3 scripts/issue-license.py --days 1 --plan test
```

---

## 4. 和产品路径的对应

| 用户状态 | 怎么造出来 |
|----------|------------|
| 试用 | 无 token + 试用未满 3 天 |
| 临期 | token 或试用剩余 ≤ 3 天 |
| 锁定 | 试用结束且无有效 token |
| 已授权充裕 | 有效 token 剩余 > 3 天 |
