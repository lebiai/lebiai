# 变更记录：Release publish 修复（正文为空 + 废弃 overwrite 入参）

| 字段 | 内容 |
|------|------|
| **编号** | `20260809-release-publish-fix` |
| **日期** | 2026-08-09 |
| **状态** | **已验收**（v1.0.0 正文 + 双安装包复验通过） |
| **负责人** | Codex Agent |
| **关联** | `20260809-release-v1.0`（v1.0.0 发布收尾） |

---

## 0. 用户价值

- **谁用：** 所有打开 GitHub Releases 页面下载安装包的用户。
- **解决什么痛点：** v1.0.0 Release 页有安装包但正文为空——用户看不到「1.0 有什么」；此前发版想「重新挂包」也做不到（旧包静默保留）。
- **用完后用户多得到什么：** Release 页正文 = 1.0 发布说明；每次发版自动得到「最新正文 + 最新安装包」，重复发布也能收敛到最新状态。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（tag → 云构建 → Release 自动更新）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更省心（发版 = 一条 git tag 命令）

---

## 0b. 产品经理视角

- **场景：** 用户在 GitHub Releases 页找 v1.0.0 安装包并想了解这次更新内容。
- **路径变化：** 改前：Release 有包无正文；`overwrite: true` 无效导致重复发布时正文/资产不可控。改后：publish 先删旧 Release 再全新创建，正文与安装包每次重建。
- **成功标准：** Release v1.0.0 页显示 1.0 发布说明正文，且可下载 dmg + exe。
- **明确不做什么：** 不做签名/公证（沿用既定边界）；不改产品功能；不改版本号（仍为 v1.0.0）。

---

## 0c. 架构师视角

- **根因层级：** CI 发布配置（workflow 输入契约 + action 语义）。
- **根因：** ① `softprops/action-gh-release@v2`（实际 v2.6.2）已移除 `overwrite` 入参（仅保留 `overwrite_files`），runner 只发 warning 并忽略，导致「重复发布覆盖」从未生效；② 该 action 的 update 路径用 `workflowBody || existingReleaseBody`，遇到先前已存在的空正文 Release 时不会补正文。
- **正确的长期默认路径：** publish 永远「先删后建」——用仓库内自带 `gh` + `GITHUB_TOKEN` 删除旧 Release（不存在则忽略），再让 action 走 create 路径，`body_path` 每次注入 `docs/release-notes.md`；结果与历史状态无关，幂等。
- **边界：** 删除只针对同名 tag 的 Release，不动 tag 本身；无 Release 时 `|| true` 静默通过。
- **如何防复发：** 移除失效入参 `overwrite`；「先删后建」写死为默认路径；发布说明仍是仓库文件 `docs/release-notes.md`，随 git 版本化。
- **为何这不是补丁：** 不再依赖 action 的 update 语义（黑盒、随版本漂移），改为与历史无关的确定性重建。

---

## 1. 方案（Plan）

- 做：① `release.yml` publish 步骤加「Clear stale release」（`gh release delete --yes || true`）② 删除无效入参 `overwrite: true` ③ 推送 main + 更新 tag v1.0.0 触发重建 ④ 复验正文与安装包 ⑤ 台账。
- 不做：签名/公证；版本号变更；本地打包。

---

## 2. 实施（Implement）

- `.github/workflows/release.yml`：publish 步骤新增 `gh release delete` 步骤（`GH_TOKEN` + `RELEASE_TAG` 推导同 `tag_name`），移除 `overwrite: true`。
- 台账：新增本记录；`20260809-release-v1.0` 状态更新为已验收。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 1 | Release 有正文 | 打开 v1.0.0 | 正文为 1.0 发布说明 | ✅（复验后确认） |
| 2 | Release 有安装包 | 打开 v1.0.0 | dmg + exe 可下载 | ✅ |
| 3 | 重复发布可覆盖 | 再次推 tag | Release 重建为最新正文+包，无旧残留 | ✅（先删后建） |

- **自动化：** 推送 tag 触发 `release.yml`（macOS + Windows 云构建 → publish），无本地构建；CI.yml 同时门禁 fmt/check/clippy/test。

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅ | Release 页正文 + 安装包齐全 |
| 开箱即用未破坏 | ✅ | 安装包为同一 v1.0.0 产物 |
| 本地优先未破坏 | ✅ | 无新增联网/运行时 |
| 测试通过 | ✅ | release.yml 云构建全绿 |
| 记录完整 | ✅ | |
| 产品+架构两视角齐全 | ✅ | |
| 非修修补补（默认路径正确） | ✅ | 先删后建，幂等 |
| 代码卫生 | ✅ | 移除失效入参，注释同步 |

- **验收人：** 用户
- **结论：** ✅ 通过
- **遗留项：** 代码签名与公证（Windows/macOS）留待后续分发阶段。

---

## 5. 附注

- 发版最快路径（本仓库）：`git tag vX.Y.Z && git push origin vX.Y.Z` → 云构建 + 自动挂 Release，本机零编译。
- 首次出现空正文的机制：旧版 workflow 的 publish 无 checkout，`body_path` 读文件失败退化为空正文；本轮已一并由「先删后建 + 必带 checkout」覆盖。
