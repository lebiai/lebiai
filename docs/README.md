# 项目文档目录

除仓库根目录**权威文档**外，**所有**说明类 Markdown 必须放在本目录（或其子目录）中。

## 仓库根目录允许的 Markdown（仅此 4 个）

| 文件 | 层级 |
|------|------|
| [`../PRODUCT_PRINCIPLES.md`](../PRODUCT_PRINCIPLES.md) | P0 产品法 |
| [`../DEVELOPMENT_RULES.md`](../DEVELOPMENT_RULES.md) | P1 开发法 |
| [`../AGENTS.md`](../AGENTS.md) | P2 Agent 入口 |
| [`../README.md`](../README.md) | P3 简介 |

根目录**禁止**再增加其他 `*.md`。需要新说明 → 放在 `docs/` 下，并在本索引登记。

> 例外（非「文档」）：代码内嵌 `SKILL.md`、引擎/客户端自带的技能文件等运行时资源。

## 本目录结构

| 路径 | 内容 |
|------|------|
| [`work-companion-solution.md`](./work-companion-solution.md) | **工作与陪伴完整蓝图**（哲学、四环、本体、协议、验收矩阵；方向唯一全貌） |
| [`project-map.md`](./project-map.md) | **项目全景地图**（产品目标、架构、入口、数据布局、已验收、缺口优先级） |
| [`install.md`](./install.md) | **安装与首次使用指引**（用户拿到 DMG/EXE 后：安装、首次放行、配置 Key、数据位置、卸载、FAQ） |
| [`api-key-guide.md`](./api-key-guide.md) | **获取 API Key 教程**（为什么需要、去哪拿、三步获取、费用/安全/FAQ；GUI 设置页内嵌摘要） |
| [`gui-run.md`](./gui-run.md) | **打开桌面 GUI**（默认 `ui/dist`、防白屏、可选 HMR） |
| [`gui-ritual-motion.md`](./gui-ritual-motion.md) | **GUI 仪式感/动效 P0**（token、启幕、首页、落印） |
| [`REMOTE_ACCESS.md`](./REMOTE_ACCESS.md) | **远程访问与安全**（token 鉴权、本机/局域网/公网、TLS 反代） |
| [`docker.md`](./docker.md) | **Docker 部署**（微信 bot 长跑、通用 CLI、镜像说明） |
| [`flutter-progress.md`](./flutter-progress.md) | **Flutter 客户端进度看板**（原根目录 TODO.md，M0–M4 状态） |
| [`codebase-learning.md`](./codebase-learning.md) | **全量代码学习报告**（权威共识 + 16 crate 精读 + 数据流 + 问题清单，2026-08-07） |
| [`records/`](./records/) | 变更台账：方案 → 实施 → 测试 → 验收（强制） |
| （墓碑）[`records/20260805-template-feature-removed.md`](./records/20260805-template-feature-removed.md) | 文档模板功能已产品废弃；中间设计/台账已删除，勿再实现占位符填槽 |
| （墓碑）[`records/20260807-remove-tb-legacy.md`](./records/20260807-remove-tb-legacy.md) | 结核病案合成数据遗留已删除（与本产品无关） |
| （脚本）[`../scripts/setup-markitdown-sidecar.sh`](../scripts/setup-markitdown-sidecar.sh) | 开发兜底：转换器装到 `~/.lebi-ai/bin` |
| （脚本）[`../scripts/prepare-markitdown-bundle.sh`](../scripts/prepare-markitdown-bundle.sh) | **发布默认：** 生成 App Resources 内 `markitdown-sidecar`（`build-dmg.sh` 会调用） |

## 新增文档约定

1. 放在 `docs/<子目录>/`，文件名用英文或拼音 slug
2. 在本 `README.md` 的结构表中加一行
3. 不得与 P0/P1 冲突；冲突时改 P0/P1，不另立「第二权威」
4. 有用户/交付影响的改动仍须走 `docs/records/` 四阶段流程
