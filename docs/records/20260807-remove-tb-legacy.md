# 变更记录：移除结核病案合成数据遗留

| 项 | 内容 |
|----|------|
| **编号** | `20260807-remove-tb-legacy` |
| **日期** | 2026-08-07 |
| **状态** | **已实施** |
| **关联** | 全量代码学习问题清单 C26；原台账 `20260803-tb-case-data` |

## 1. 方案

### 问题

`data/tb_cases_2026_h1.csv` + `scripts/gen_tb_cases.py` + `docs/tb-case-data.md`
是与其他项目相关的结核病案合成数据，与本产品（乐彼AI = 本地工作搭子）无关；
`hermes doctor` 是环境自检，并非医疗命令。属于「凡旧必清」范围内的无关遗留。

### 决策

- **删除**三个文件（数据、生成脚本、说明文档）。
- **保留**原台账 `20260803-tb-case-data.md` 作为历史；本文件为删除墓碑。
- **不自动删除**任何用户数据目录（无涉）。

### 成功标准

1. 仓库内无 `tb_cases_2026_h1.csv` / `gen_tb_cases.py` / `tb-case-data.md`。
2. `docs/README.md`、`docs/project-map.md`、`docs/records/README.md` 引用已同步。

## 2. 实施

| 文件 | 处理 |
|------|------|
| `data/tb_cases_2026_h1.csv` | 删除 |
| `scripts/gen_tb_cases.py` | 删除 |
| `docs/tb-case-data.md` | 删除 |
| `docs/README.md` | 移除索引行，新增本墓碑行 |
| `docs/project-map.md` | 台账行状态 → **已移除** |
| `docs/records/README.md` | 索引行状态 → **已移除** |

## 3. 测试

- [x] `ls` 确认三个文件不存在
- [x] `rg` 确认代码/文档无 `tb-case-data` / `gen_tb_cases` / `tb_cases_2026` 业务引用（仅墓碑与历史台账）

## 4. 验收

- 用户确认：该数据与本产品无关，删除后不再恢复。
