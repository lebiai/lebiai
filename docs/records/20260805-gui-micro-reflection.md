# 变更记录：GUI micro-reflection 正确架构（重做）

| 字段 | 内容 |
|------|------|
| **编号** | `20260805-gui-micro-reflection` |
| **日期** | 2026-08-05 |
| **状态** | 单测/check 通过 · 待真机 |
| **负责人** | agent |

---

## 0. 用户价值

GUI 对话过程中也能捕获「记住…」等教学瞬间；不挡输入；默认不自动写盘；与 CLI 同一套规则。

---

## 0b. 产品经理

| 项 | 内容 |
|----|------|
| **场景** | 聊天中途说「记住我偏好 vim」 |
| **路径** | turn Done → 继续输入 → 后台 micro → banner/审阅（仅当前会话） |
| **成功标准** | Done 后立即可输入；候选可审；auto-accept 受 config+dedup |
| **不做** | 不把 micro 塞进 stream Channel；不默认 auto 写 skill |

---

## 0c. 架构师（正确默认路径）

### 分层

```
hermes-reflect::run_micro_after_turn   ← 唯一管线 (gate + LLM + apply + recompile)
        ▲
        │
   ┌────┴────┬────────────┐
   CLI       GUI          server
   eprintln  Event emit   WS push
```

### GUI 关键纠正

| 错误做法（上一版） | 正确做法 |
|--------------------|----------|
| MicroReflection 挂在 turn 的 `Channel` 上（Done 后推） | **全局 Tauri Event** `hermes://micro-reflection` |
| GUI/server 各写一套 micro 逻辑 | 共享 `run_micro_after_turn` |
| 与 stream 生命周期耦合 | stream = turn；micro = app 生命周期，按 `sessionId` 过滤 |

### 时序

```
run_turn → Done (Channel) → 落盘
              │
              └─ spawn_after_turn
                    run_micro_after_turn
                    → emit hermes://micro-reflection
                    → 前端 listen（activeSessionId 匹配才展示）
```

---

## 实施清单

- [x] `hermes-reflect/src/micro_run.rs`
- [x] `hermes-gui/src/commands/micro.rs` + Event
- [x] chat.rs 瘦身为 hook
- [x] 去掉 ChatStreamEvent::MicroReflection
- [x] server 复用 micro_run，WS 推送
- [x] CLI 复用 micro_run
- [x] 前端 `bindMicroReflectionListener` + session 过滤
- [x] MicroReviewModal / banner 保留

---

## 测试

```bash
cargo test -p hermes-reflect
cargo check -p hermes-gui -p hermes-cli -p hermes-server
```
