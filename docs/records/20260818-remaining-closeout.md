# 变更记录：审计余项收口（桌面撒谎 · 单队列 · 写盘门 · 安全 · 卫生）

| 字段 | 内容 |
|------|------|
| **编号** | `20260818-remaining-closeout` |
| **日期** | 2026-08-18 |
| **状态** | 实施中 |
| **型** | 产品 |
| **关联** | 用户：开工 · 账本 1–4 |

---

## 0-fp

- **拒绝的类比：** 不是只擦看得见的再假装账清了；不是把手机做成第二桌面。
- **拆出的真：** 文案与界面必须同一条路；待审只能有一个入口；写长期状态要过同一道门；无确认面不能半接线；密钥不能被普通 bash 读走。
- **如何推出：** 按账本 1→4 改正确默认路径，同次清旧。授权/slot/手机材料写明已拍，不偷偷扩。

## 0. 用户价值

对话不再中途逼确认；过了的债能看见；删/清 Key 要点头；进化进一个待审；微信/安装/bash 不再比桌面松。

## 0b. 产品经理

- **场景：** 桌面 GUI 为主；IM/server 只收口撒谎与半接线。
- **怎么走完：** 候选在「它记得的」待审；Cue 先问过了的；清 Key / 复位目录 / 丢掉先确认一句。
- **不做什么：** 不锁 CLI；不扩 slot 词表；不给手机接材料/在办写；不在 GUI 接 subagent。

## 0c. 架构师

- **根因：** 双队列、双入口、确认缺失、server 装了规格禁止的写工具、沙箱读全开。
- **默认路径：** inbox 一条；GUI 不写 deferred；memory_save 走 `memory_passes_gate`；server 卸 commitment_store；seatbelt 拒密钥路径。

## 1. 方案

见实施。已拍：授权仍只锁 GUI；slot 不扩；手机不接材料。

## 2. 实施

见 diff。

## 3. 测试

- `cargo fmt -- --check` 通过（批量 fmt 已落盘）。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过（修 `url_safety.rs` 未用导入 `Ipv6Addr`；清 `SkillCandidateProposed` 死变体与前端 `proposedSkills` 死状态；`source.rs` 冗余闭包改直传）。
- `cargo test --workspace` 全绿（414 passed / 0 failed）。
- `tsc --noEmit` + `vite build` 通过。

## 4. 验收

工程门禁已绿。桌面目视（待审入口、Cue 先问过了、清 Key 确认）仍待用户在窗口走查。
