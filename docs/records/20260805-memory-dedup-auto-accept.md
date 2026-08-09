# 变更记录：记忆写路径近重复门控 + AutoAccept 日志诚实

| 字段 | 内容 |
|------|------|
| **编号** | `20260805-memory-dedup-auto-accept` |
| **日期** | 2026-08-05 |
| **状态** | 测试通过（单测）· 待真机手测 auto-accept |
| **负责人** | agent |
| **关联** | G1 / G4（FsMemoryStore · auto-accept 深挖） |

---

## 0. 用户价值

- **谁用：** CLI 用户（micro-reflection auto-accept）、agent 调 `memory_save`
- **痛点：** auto-accept / 裸 save 可重复写入近重复记忆，知识库只增不收敛；失败仍记 AutoAccept 污染统计
- **用完后：** 近重复被拒绝并可提示用 supersedes 替换；accept 率只计真正落盘

---

## 0b. 产品经理视角

- **场景：** 开启 `auto_accept_memories` 或 agent `memory_save` 写入近似事实
- **路径变化：** 改前直接 put → 改后先 `check_near_duplicate`，冲突则跳过/报错；带 supersedes 的替换路径不受阻
- **成功标准：** 近重复拒绝可见；supersedes 写入仍成功；失败不记 AutoAccept
- **不做什么：** 不把 dedup 塞进 `put()` 本体（会误伤人审 full reflection / distill）

---

## 0c. 架构师视角

- **根因：** `check_conflict_tfidf` 从未接线；AutoAccept 日志在 put 外无条件执行
- **正确路径：** trait `check_near_duplicate` → auto-accept / 无 supersedes 的 `memory_save` 调用；有 supersedes 跳过
- **阈值：** `DEFAULT_DEDUP_THRESHOLD = 0.55`（与 distill 对齐）

---

## 实施

- [x] `MemoryStore::check_near_duplicate` + `DEFAULT_DEDUP_THRESHOLD`
- [x] CLI micro auto-accept：dedup 门控 + 仅 Ok 记 AutoAccept + 有 conflicts 整批不 auto
- [x] `memory_save` 无 supersedes 时 dedup
- [x] 单测（store + tools）

---

## 测试

```bash
cargo test -p hermes-memory
cargo test -p hermes-tools
cargo check -p hermes-cli
```
