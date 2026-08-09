# 变更记录：文档卫生 — 删除已废弃模板设计与中间取消台账

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-doc-hygiene-dead-templates` |
| **日期** | 2026-08-06 |
| **状态** | **已验收** |
| **关联** | `20260805-template-feature-removed`；P0 第六条 / 第九条；AGENTS「文档摆放铁律」 |

---

## 0. 用户价值

- **谁用：** 协作者（人类 + AI agent）
- **痛点：** 已产品废弃的模板系统仍有 2 份设计定稿 + 7 份中间取消台账；AI 易被「如何实现 template_fill」误导，检索噪音大
- **用完后：** 文档树只保留**墓碑**一条（为何删除、禁止恢复）；索引无断链、无假活路径

## 0b. 产品经理视角

- **场景：** 重新梳理项目 / 改代码前读 docs
- **路径：** 改前（多份已取消设计与台账并存）→ 改后（唯一墓碑 + 现行能力文档）
- **成功标准：** `docs/` 无 `template-system*.md`；records 无中间取消模板台账；`docs/README` / `records/README` / `project-map` 已同步
- **不做什么：** 不改运行时代码；不删 `20260805-template-feature-removed`；不删仍有效的 `workspace/outputs` 等台账

## 0c. 架构师视角

- **根因：** 功能删除后设计文档与过程台账未同步清旧（违反「凡旧必清」）
- **正确默认：** 废弃功能 → 删死文 + 留一条墓碑；权威仍仅根目录 4 个 md
- **为何非补丁：** 对齐 P0 文档位置与代码卫生，降低错误实现复发面

---

## 1. 方案

删除已标注「产品废弃 / 已取消」且不再被实现引用的文档；索引与墓碑回写。

## 2. 实施

### 已删除

| 路径 | 原因 |
|------|------|
| `docs/template-system.md` | v1.x 设计定稿，功能已删 |
| `docs/template-system-v2.md` | v2 设计定稿，功能已删 |
| `docs/records/20260803-template-system-design.md` | 中间取消台账 |
| `docs/records/20260803-template-p0.md` | 同上 |
| `docs/records/20260803-template-p1.md` | 同上 |
| `docs/records/20260803-template-p2.md` | 同上 |
| `docs/records/20260803-office-template-delivery.md` | 同上 |
| `docs/records/20260804-office-delivery-p3a.md` | 同上 |
| `docs/records/20260804-template-v2-1to1.md` | 同上 |

### 保留

| 路径 | 原因 |
|------|------|
| `docs/records/20260805-template-feature-removed.md` | 唯一墓碑：范围、原因、禁止恢复 |
| `docs/records/20260803-workspace-outputs-default.md` | `outputs/` 规则仍有效，与模板无关 |

### 索引更新

- `docs/README.md`：去掉 template-system 行；登记墓碑
- `docs/records/README.md`：去掉中间取消行；加本记录
- `docs/project-map.md`：主线表对齐
- 墓碑正文「文档」节改为「已删除」

## 3. 测试

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | `ls docs/*.md` | 无 template-system* | 通过 |
| 2 | `ls docs/records/*template*` | 仅 feature-removed | 通过 |
| 3 | 根目录 `*.md` | 仍仅 4 权威 | 通过 |
| 4 | docs/README 结构表无断链 | 链接可达 | 通过 |

## 4. 验收

| 门槛 | 结果 |
|------|------|
| 用户价值成立 | ✅ 协作不再被死模板路径误导 |
| 开箱/本地优先 | ✅ 未触运行逻辑 |
| 文档位置 | ✅ 根目录白名单未破坏 |
| 代码卫生 | ✅ 死文删除；墓碑保留 |
| 记录完整 | ✅ 本文件 |

- **验收人：** Agent（用户委托：深度学习 + 删无用文档）
- **日期：** 2026-08-06
- **结论：** 通过
