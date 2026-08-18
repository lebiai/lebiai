# 变更记录：我的材料 · 立项实施

| 字段 | 内容 |
|------|------|
| **编号** | `20260818-work-sources-impl` |
| **日期** | 2026-08-18 |
| **状态** | **工程通过** · 待目视（1–6 补洞） |
| **型** | 产品 |
| **关联** | [`../spec/work-sources.md`](../spec/work-sources.md) · [`../explore/work-sources.md`](../explore/work-sources.md) |

---

## 0-fp. 第一性原理

- **拒绝的类比：** 不是知识库产品、不是网盘、不是把记忆撑成百科。
- **拆出的真：** 普通人只对话、丢 Word/PDF；成功是下次问已经用上；假命中比漏检更伤；开口不能读每一份文件；丢掉必须看得见。
- **如何推出：** 新对象 `sources`；丢进 Word/PDF 自动留下+撤销；每轮打内存索引；Know 第三页「我的材料」常驻丢掉。

## 0. 用户价值

丢进合同/口径，下次直接问就能按他自己的材料答。不用建库、不用检索、能撤销、能丢掉。

## 0b. 产品经理

- **场景：** 桌面对话。
- **怎么走完：** 丢 Word/PDF → 当次能用且自动留下（撤销）→ 下次直接问 → 「我的材料」可打开/丢掉。
- **看起来：** 不新增顶栏；toast 一句+撤销；丢掉图标常驻。
- **不做什么：** 不接 IM/Flutter；不蒸正文；不预装行业包。

## 0c. 架构师

- **根因：** 只有当次 uploads，没有跨次原文对象。
- **默认路径：** `hermes-sources` 目录+catalog；GUI `import_document` 后 ingest；`begin_turn` 检索注入 `[lebi-AI Materials]`（可剥除）；IM 不传 hits。
- **为何不是补丁：** 独立对象，不进 memories/。

## 1. 方案

拍板：立项；Word/PDF 自动留下+撤销；定名「我的材料」；常驻删除。

## 2. 实施

见 crate `hermes-sources`、GUI commands/source、Know 第三页、upload 自动留下。

补洞（2026-08-18 建议 1–6）：撤销新版恢复上一版；转换失败仍留原件；`source_list`/`source_read`（IM 不接）；换话题丢焦点；材料页搜索/日期/上一版；故事 F–I 单测。

再补（出处 + 正文搜索）：`sanitize_material_citations` 落盘并 `textCorrected`；`list_matching` 搜正文；真实 docx（textutil）圆路测试。桌面 GUI 目视仍待用户。

再补（不每次问留下）：第一次才 toast；之后静默。新版 / 读不出字仍提一句。

未完成全量见 [`20260818-open-work.md`](./20260818-open-work.md)。

## 3. 测试

`cargo test -p hermes-sources -p hermes-core -p hermes-channel`；`npm run build`（gui ui）。目视：丢 PDF、撤销、丢掉、问材料里的话。

## 4. 验收

工程绿 + 目视主路径后再改「已验收」。
