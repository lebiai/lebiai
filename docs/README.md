# 文档目录

> **效力：** 本目录全部**非权威**。冲突以 [`PRODUCT_PRINCIPLES.md`](../PRODUCT_PRINCIPLES.md)（P0 v0.11）为准。
> **规则：** 先选种类 B–H 再写。选不出就不写。种类见 P0 第六条。

## A · 权威（仓库根，仅 4 个）

| 文件 | 层级 |
|------|------|
| [`../PRODUCT_PRINCIPLES.md`](../PRODUCT_PRINCIPLES.md) | P0 唯一产品法 |
| [`../DEVELOPMENT_RULES.md`](../DEVELOPMENT_RULES.md) | P1 开发法 |
| [`../AGENTS.md`](../AGENTS.md) | P2 协作入口 |
| [`../README.md`](../README.md) | P3 用户简介 |

## B · 用户说明书 · `guide/`

| 路径 | 内容 |
|------|------|
| [`guide/install.md`](./guide/install.md) | 安装与首次使用 |
| [`guide/api-key-guide.md`](./guide/api-key-guide.md) | 获取 API Key |
| [`guide/channel-allowlist.md`](./guide/channel-allowlist.md) | IM 发送方白名单 |

## C · 发布说明 · `releases/`

| 路径 | 内容 |
|------|------|
| [`releases/release-notes.md`](./releases/release-notes.md) | 1.2 用户向发布说明 |

## D · 操作规格 · `spec/`

| 路径 | 内容 |
|------|------|
| [`spec/work-companion-solution.md`](./spec/work-companion-solution.md) | 工作搭子体验规格（展开 P0） |
| [`spec/license-ux.md`](./spec/license-ux.md) | 授权 / 试用 / 续期交互 |
| [`spec/settings-ia.md`](./spec/settings-ia.md) | 设置页信息架构 |
| [`spec/in-app-update.md`](./spec/in-app-update.md) | 桌面应用内点击更新（实施中 · U3 未做） |
| [`spec/gui-ritual-motion.md`](./spec/gui-ritual-motion.md) | GUI 仪式感 / 动效 |
| [`spec/zaiban-work.md`](./spec/zaiban-work.md) | **在办+回顾统一规格 v2.0**（已拍板；目视仍开放） |
| [`spec/zaiban.md`](./spec/zaiban.md) | 在办 v1（被 v2 草案覆盖） |
| [`spec/zaiban-drawer-review.md`](./spec/zaiban-drawer-review.md) | 抽屉+回顾 v1.2（被 v2 草案覆盖） |

## E · 开发者手册 · `dev/`

| 路径 | 内容 |
|------|------|
| [`dev/gui-run.md`](./dev/gui-run.md) | 打开桌面 GUI |
| [`dev/updater-signing.md`](./dev/updater-signing.md) | 应用内更新的发版签名 |
| [`dev/REMOTE_ACCESS.md`](./dev/REMOTE_ACCESS.md) | server 远程访问 |
| [`dev/docker.md`](./dev/docker.md) | Docker（非默认路径） |
| [`dev/license-test.md`](./dev/license-test.md) | 授权怎么测、怎么发码 |
| [`dev/web-search.md`](./dev/web-search.md) | web_search 后端 |
| [`dev/mobile-extras.md`](./dev/mobile-extras.md) | 移动端推送等平台配置 |

## F · 台账 · `records/`

| 路径 | 内容 |
|------|------|
| [`records/`](./records/) | 一次变更的过程；索引见 [`records/README.md`](./records/README.md) |
| [`records/_TEMPLATE.md`](./records/_TEMPLATE.md) | 台账模板 |

## G · 探索 · `explore/`

| 路径 | 内容 |
|------|------|
| [`explore/nl-admin-control-plane.md`](./explore/nl-admin-control-plane.md) | 未立项。不得当现行目标 |

## H · 快照 · `snapshot/`

| 路径 | 内容 |
|------|------|
| [`snapshot/project-map.md`](./snapshot/project-map.md) | 项目地图（2026-08-17；过期以 P0 与代码为准） |
| [`snapshot/codebase-learning.md`](./snapshot/codebase-learning.md) | 代码学习快照 |
| [`snapshot/flutter-progress.md`](./snapshot/flutter-progress.md) | Flutter 进度快照 |

## 不是文档

运行时 `SKILL.md`、`scripts/`、签发 HTML、第三方 Agent 技能、脚手架 README：不进本表，不按 B–H 生产。

## 新增

1. 选 B–H，放入上表目录
2. 文首：种类、非权威、冲突以 P0 为准、日期
3. 在本索引加一行
4. 产品方向变了 → 先升 P0
5. 有用户 / 架构影响 → 同时写 F 台账
