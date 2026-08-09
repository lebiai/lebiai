# 变更记录：通用 Hermes / 律师版 数据目录隔离

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-product-data-isolation` |
| **日期** | 2026-08-03 |
| **状态** | **已实施 · 待打开双 GUI 冒烟** |
| **负责人** | Grok（用户委托 · 方案 2 两边一起改） |
| **关联** | 用户反馈通用 GUI 看到律师技能/会话 |

---

## 0. 用户价值

- **谁用：** 同时开发/使用「通用 Hermes」与「乐彼-律师版」的人
- **痛点：** 两产品共用 `~/.small-rust-hermes`，技能与历史串台
- **用完后：** 各产品默认不同数据根；律师数据可迁移到新目录；通用版可干净启动

---

## 0b. 产品经理

- **路径：** 通用 → `~/.small-rust-hermes`；律师 → `~/.lebi-law`
- **成功标准：** 打开通用 GUI 不再默认出现 case-analysis 等律师技能与律师会话（除非用户自己装）
- **不做什么：** 不删除用户旧目录；不合并两产品代码

---

## 0c. 架构师

- **根因：** 硬编码同一 `$HOME/.small-rust-hermes`
- **正确路径：** `hermes_core::data_root()` + 产品常量默认名；`HERMES_DATA_DIR` 覆盖；律师 `maybe_migrate_from_legacy` 从旧目录**复制**（不删）
- **为何非补丁：** 单一真相源 data_root，两端默认名不同

---

## 1–2. 方案与实施

| 产品 | 默认数据根 | 代码位置 |
|------|------------|----------|
| newdata 通用 Hermes | `~/.small-rust-hermes` | `hermes-core/src/paths.rs` |
| code 律师版 | `~/.lebi-law` | 同上 + `maybe_migrate_from_legacy` |

两端路径构造改为 `data_root()` / `data_path(...)`。

---

## 3. 测试

| # | 用例 | 期望 |
|---|------|------|
| 1 | `data_root` 默认名 | 通用 ends_with `.small-rust-hermes`；律师 `.lebi-law` |
| 2 | `HERMES_DATA_DIR=/tmp/x` | 两边都用 /tmp/x |
| 3 | 律师首次启动 | 有旧律师标记则复制到 ~/.lebi-law |
| 4 | 通用 GUI | 不读 ~/.lebi-law；旧目录仍在则仍能见旧数据（用户可自行清理律师技能） |

---

## 4. 验收

### 本机已执行（2026-08-03）

- [x] 律师数据复制到 `~/.lebi-law`（含 skills/sessions/knowledge/law-corpus）
- [x] `~/.lebi-law/config.toml` 的 `workspace.root` 指向 `~/.lebi-law/workspace`
- [x] 从 `~/.small-rust-hermes` **移走**律师专属：技能包、knowledge.db、law-corpus、identity、templates → `_quarantine_lawyer/`
- [x] 通用 `skills/` 仅剩通用/用户技能（无 case-analysis 等）
- [x] 历史会话 jsonl 已迁出通用目录（副本在律师目录）
- [x] 两端 `ui/dist` 已 build；`cargo check` 通过

### 打开命令（确认无白屏、数据不串）

```bash
# 通用 Hermes
cd newdata/small-rust-hermes && scripts/run-gui.sh

# 律师版
cd code/small-rust-hermes && scripts/run-gui.sh
```

| 检查 | 通用 | 律师 |
|------|------|------|
| 数据根 | `~/.small-rust-hermes` | `~/.lebi-law` |
| 技能列表 | 无 case-analysis 等律师包 | 有律师五场景 |
| 界面 | 非白屏（dist） | 非白屏（dist） |
