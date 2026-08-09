# 变更记录：C-SESS 工作情节闭环

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-csess-work-episode-loop` |
| **日期** | 2026-08-06 |
| **状态** | **已实施**（单测绿；真机 A 故事待用户验） |
| **蓝图** | `docs/work-companion-solution.md` §5.4 C-SESS；产品卡「第二次更准 / 手感」 |

---

## 0. 用户价值

离开一段有实质工作的对话后，能**批准一条工作情节**；下次做同类事时，系统更易检索到该情节并触发 Continuity（「上次类似…」）。

## 0b. 产品经理

- **场景：** 用户做完一轮工作 → 离开对话 → 审阅 → 接受情节 → 新对话同类任务  
- **成功标准：** 故事 A 可手测；无锚仍禁止装记得  
- **不做：** 不自动落盘情节（仍须批准）；不做生活向

## 0c. 架构师

- **根因：** reflection 常漏 episode / zone=general；检索不偏好情节；上下文未标 kind  
- **正确路径：** finalize 归一化 + 摘要种子候选；检索 continuity boost；context 标注 work-episode  

## 2. 实施

| 模块 | 改动 |
|------|------|
| `hermes-reflect/episode.rs` | normalize / seed / finalize；runner 解析后调用 |
| `hermes-reflect/prompt` | C-SESS 强制倾向至少一条 work-episode |
| `hermes-memory/relevance` | zone 进 token；work-episode ×1.4 加权 |
| GUI/Server/CLI context | Continuity 文案 + kind=work-episode |
| GUI ReflectionReview | 工作情节徽章；zone 显示 |
| i18n | session-end 提示优先情节 |

## 3. 测试

- [x] `cargo test -p hermes-reflect episode`  
- [x] `cargo test -p hermes-memory` relevance  
- [ ] 真机：离开会话 → 接受情节 → 新会话同类任务出现再认出  

## 4. 手测步骤（用户）

1. `scripts/run-gui.sh`（先 npm build）  
2. 新对话做一件实质工作（如改一版稿）≥ min_turns  
3. 新建/切换对话 → 审阅 → **接受**带「工作情节」徽章的候选  
4. 新对话说同类任务 → 应出现有锚的上次类似（若模型遵循协议）  
