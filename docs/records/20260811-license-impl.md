# 变更记录：授权 / 试用 / 续期实现（license-ux）

| 字段 | 内容 |
|------|------|
| **编号** | `20260811-license-impl` |
| **日期** | 2026-08-11 |
| **状态** | **工程通过**（产品目视 ⬜） |
| **关联** | [`../spec/license-ux.md`](../spec/license-ux.md)、`20260811-license-ux-spec` |

---

## 0. 用户价值

交付 dmg/exe 后：试用 3 天 → 临期日更提醒 → 过期全屏贴码 → 微信 iodine001 购码续期；侧栏+设置电池可见剩余。

## 0b. 产品经理

- 路径对齐 `license-ux.md` 冻结规格。
- 续期 = 同一 `apply_license`。
- 不绑机。

## 0c. 架构师

- **验签：** Ed25519，`LEBI1.<payload_b64url>.<sig_b64url>`，公钥内置于 `hermes-core`。
- **状态：** `~/.lebi-ai/license.json`（token / trial_started_at / last_nudge_date）。
- **闸门：** `send_message` + 前端 `canUseMain`。
- **签发：** `scripts/issue-license.py`（dev seed；生产换钥）。

## 2. 实施

| 区域 | 内容 |
|------|------|
| `hermes-core/src/license.rs` | 验签、试用、状态机、apply、nudge |
| `hermes-gui` commands/license | get / apply / mark_nudge |
| UI | 电池、设置卡、全屏锁、临期弹窗、i18n |
| `scripts/issue-license.py` | 签发 |

## 3. 测试

| # | 结果 |
|---|------|
| `cargo test -p hermes-core license` | ✅ 8 passed |
| `cargo check -p hermes-gui` | ✅ |
| `npx tsc --noEmit` | ✅ |
| 目视 GUI | ⬜ |

## 4. 签发示例

```bash
pip install pynacl
python3 scripts/issue-license.py --days 365 --plan year
# 将打印的 LEBI1.... 交给用户粘贴
```

**生产前：** 重新生成 Ed25519 密钥对，更新 `PUBLIC_KEY_BYTES` 与签发 seed，勿使用仓库 dev seed。
