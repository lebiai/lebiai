# 变更记录：修复 chatStore.ts TS 类型错误（build 恢复全绿）

| 字段 | 内容 |
|------|------|
| **编号** | `20260806-fix-chatstore-ts`（与文件名一致） |
| **日期** | 2026-08-06 |
| **状态** | **已验收** |
| **负责人** | Codex |
| **关联** | 遗留项②：`docs/records/20260806-gui-wechat-connect.md`；`crates/hermes-gui/ui/src/store/chatStore.ts` |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 开发者 / 打包流程（GUI 分发 dmg/exe 前必经 `npm run build`）
- **解决什么痛点：** `npm run build` 被既有 2 个 TS 类型错误卡死，GUI 无法出包
- **用完后用户多得到什么：** 一键构建恢复全绿，分发路径不再被旧错误阻塞
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更快或更省心

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户按 `scripts/run-gui.sh` 或直接 `npm run build` 构建 GUI 分发包
- **路径变化：** 改前 = tsc 报 `Expected 0 arguments, but got 1` + `Property 'replace' does not exist`，build 失败 → 改后 = tsc 0 错误，vite build 成功出 dist
- **成功标准：** `tsc --noEmit` 0 错；`vite build` 成功产出 `dist/`
- **明确不做什么：** 不动其他既有代码 / 不重排 store / 不引入新依赖

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 状态获取层（UI store）笔误——`chatStore.ts:404` 把 translator 多包了一层函数 `const t = () => useUiStore.getState().t;`，导致 `t("key", params)` 返回的是 translator 函数而非字符串
- **正确的长期默认路径：** 直接用 `useUiStore.getState().t` 取函数体，与文件内其他 5+ 处（218/221/227/382/468/594/703 行）完全一致
- **与引擎/各入口边界：** 纯前端 i18n 取用，不涉及 `hermes-core` / server 路由
- **安全影响：** 无（不改数据与权限路径）
- **如何防复发：** `package.json` build 脚本 = `tsc && vite build`，回归即被 tsc 拦截
- **为何这不是补丁：** 修复的是错误源头（返回类型错误），不是加 `as` 断言 / `// @ts-ignore` 遮错

---

## 1. 方案（Plan）

- **目标：** GUI 前端构建恢复全绿
- **范围：** 做 = 修 `chatStore.ts` 一处 translator 取用 / **不做** = 其他遗留项、重构
- **用户路径变化：** build 失败 → build 成功
- **技术要点：** `crates/hermes-gui/ui/src/store/chatStore.ts`；验证 `node node_modules/typescript/bin/tsc --noEmit` + vite build
- **风险与回滚：** 低；单行改动，可随时还原
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期：2026-08-06 · 人：Codex

---

## 2. 实施（Implement）

- **实际改动摘要：** `const t = () => useUiStore.getState().t;` → `const t = useUiStore.getState().t;`（去掉多余包裹函数，与全文件惯例一致）
- **关键路径/文件：** `crates/hermes-gui/ui/src/store/chatStore.ts:404`
- **偏离方案处：** 无

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 构建不再被 TS 类型错误卡住 | `node node_modules/typescript/bin/tsc --noEmit` | 0 错误 | 通过 | `TSC_OK` |
| 2 | GUI 前端能正常出包 | `node node_modules/vite/bin/vite.js build` | 成功产出 `dist/` | 通过 | 1.18s，产物正常；chunk >500kB 为既有提示，非错误 |

- **自动化：** tsc + vite build 均通过（`npm run build` 的 npm 包装进程本次卡死为环境问题，直接跑等价两步命令已验证真实管线）
- **手工：** 无（纯构建回归）
- **测试结论：** [x] 全部通过 · [ ] 有已知问题

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 构建恢复全绿，出包不再被卡 |
| 开箱即用未破坏 | ☑ | 不改运行路径，仅修构建错误 |
| 本地优先未破坏 | ☑ | 无外部依赖变化 |
| 测试通过 | ☑ | tsc 0 错 + vite build 成功 |
| 记录完整 | ☑ | 本台账 |
| 产品+架构两视角齐全 | ☑ | 见 0b / 0c |
| 非修修补补（默认路径正确） | ☑ | 修错误源头，与全文件惯例对齐 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理（P0 第九条） | ☑ | 单行修复，无新增死代码 |

- **验收人：** Codex
- **验收日期：** 2026-08-06
- **结论：** ☑ 通过 · ☐ 驳回（原因：）
- **遗留项：** 主台账 `20260806-gui-wechat-connect.md` 的「GUI 真机扫码验收」仍待用户手测（本遗留项②已闭环）

---

## 5. 附注

- tsc 输出：无（0 错误）
- vite build 输出：`dist/index.html` 0.43 kB；`dist/assets/index-*.css` 91.78 kB；`dist/assets/index-*.js` 518.71 kB
