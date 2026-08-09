# 变更记录：GUI 仪式感 · 动效 · 统一视觉语言

| 字段 | 内容 |
|------|------|
| **编号** | `20260805-gui-ritual-motion-ux` |
| **日期** | 2026-08-05 |
| **状态** | **已并入 system 验收**（全站 A–E 见 `20260805-gui-ritual-system`，用户 2026-08-06「仪式通过」） |
| **负责人** | agent + 用户 |
| **关联** | `20260805-gui-ritual-system`；micro-reflection / accept 路径 |

---

## 0. 用户价值（必填 · 站在用户角度）

> 若写不出「用户因此更好用 / 更高效 / 更能进化」，**不得开工**。

- **谁用：** 个人用户 / 开发者，**主交付面 `hermes-gui`**（桌面）；非行业专家
- **解决什么痛点：**
  - 界面偏「冷冰冰文字墙」，空首页与首次打开缺少信任与引导
  - 进化（批准记忆/技能）是 Hermes 核心价值，但 UI 几乎无「被看见」的反馈
  - 局部紫/蓝/卡片风格已有 token，但动效与仪式若各自堆砌会风格分裂
- **用完后用户多得到什么：**
  - **更好用：** 首次知道「本地 + 你批准才写入」；空首页可感知、愿点第一句
  - **更高效：** 动效不挡输入；可跳过欢迎；主路径步骤不增加
  - **更能进化：** 批准落盘时有统一「落印」反馈，强化可审进化的掌控感与再参与意愿
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（纯 GUI 前端 + 现有本地文件）
  - [x] 步骤可感知、可预期（欢迎可 Skip；仪式挂在批准之后）
  - [x] 不增加无意义确认（不新增危险 confirm；不强制多步向导）
  - [x] 高频路径（打字/流式）不因动效变慢

---

## 0b. 产品经理视角（必填）

- **场景：**
  1. **首次打开 GUI**（无/有 API Key）— 需要信任与方向
  2. **空会话首页** — 需要「活」的入口，不是说明书墙
  3. **Accept 记忆/技能**（Reflect / micro 审阅 / session-end）— 需要「我的裁定生效了」
  4. **会话结束反思完成** — 需要收束，不是又一个任务弹窗
- **路径变化：**
  - **改前：** 进 App →（可选 SetupBanner）→ 静态 Welcome 四卡 → 对话；Accept 仅 toast/列表变
  - **改后：** 进 App →（一次性）可跳过启幕仪式 → 动态统一首页 → 对话；Accept → 统一「落印」动效；session-end 完成 → 轻量「收卷」摘要
- **成功标准（可观察）：**
  1. 首次启动出现欢迎层，可 **Skip**，完成后不再默认出现（本地标记）
  2. 空首页：背景有克制呼吸/光晕；场景卡 stagger 入场 + hover 抬升；仍用现有 `app-*` token
  3. Accept memory/skill 成功：出现同一套 Ritual 反馈（≤600ms），不挡下一操作
  4. 亮/暗色一致；`prefers-reduced-motion` 下降级为淡入
  5. Reflect / micro / session-end 的 header/卡片视觉同源（无第三套风格）
  6. 验收用 `scripts/run-gui.sh` 或 npm build + cargo run；**默认仍 ui/dist**
- **明确不做什么：**
  - 不引入排行/积分/强制打卡
  - 不自动写入记忆/技能并庆祝（违反 P0 第一条）
  - 不让 micro/reflection 阻塞输入
  - 不把 devUrl:5173 写回默认；不加数据库
  - 不在根目录堆新 md；设计说明进 `docs/` + 本台账
  - P0 阶段不做音效、不做 CLI 动效、不做 Flutter 动效（可后续 1:1 语义对齐）
  - 不为「炫」引入第二套设计系统或第三主色

---

## 0c. 架构师视角（必填）

- **根因层级：** **GUI 呈现层 / 设计系统**（非引擎、非 reflection 协议）
  - 用户感知「冷」= 缺统一 motion 语言 + 空态/首次路径未产品化
  - 风险 = 各页面私写动画导致风格分裂与补丁堆叠
- **正确的长期默认路径：**
  1. **单一设计真相源：** `index.css` `@theme` tokens + `ui.tsx` / 新增 `motion` 约定 + 少量 `components/motion|ritual/*`
  2. **色彩语义：** Primary（蓝）= 工作台；Accent（紫）= **仅进化时刻**（反思/落印/micro banner/宫殿）
  3. **动效：** 优先 CSS variables + Tailwind transition；全站一个 motion 时长阶梯；`prefers-reduced-motion` 必做
  4. **状态：** 欢迎完成标记存本地（`localStorage` 或 `~/.small-rust-hermes/` 下轻量文件，0600 非必须因无密钥）；不上传
  5. **GUI 加载：** 始终 `ui/dist`；改 UI 后 `npm run build`；`scripts/run-gui.sh`
  6. **引擎边界：** 不改 reflection 默认「须批准」；仪式只绑在 **已有 accept 成功路径** 与 **UI 空态/首启**
- **与引擎/各入口边界：**
  - 仅 `crates/hermes-gui/ui`（+ 必要时极薄 Rust 读本地 flag，优先纯前端）
  - 不 fork 引擎；server/Flutter 本变更不做平行 UI（文档可标「GUI 先，移动后续」）
- **安全影响：** 无新增出站；无 token 变化；不扩大 workspace
- **如何防复发：**
  - 禁止业务页内联魔法数字时长；必须用 motion token
  - PR/台账要求：新 UI 过「统一检查清单」
  - 改完即清：替换 Welcome 静态结构时删除旧无样式/死 class
- **为何这不是补丁：**
  先立 **设计系统扩展（token + 组件族）**，再实现欢迎/首页/落印，单一默认路径，而非每页加一段 transition。

---

## 1. 方案（Plan）

### 1.1 目标

在 **不破坏** 好用/高效/可进化 与 reflection 纪律的前提下，为 GUI 建立：

1. 统一的 **工作台（蓝）+ 进化时刻（紫）** 视觉/动效语言  
2. **首次欢迎仪式**（可 Skip）  
3. **动态空首页**  
4. **批准后落印 / 会话收卷** 的克制反馈  

### 1.2 范围

| 做（P0 本变更建议切片） | 不做（本变更） |
|------------------------|----------------|
| Motion tokens + AmbientStage + MotionCard + RitualMark | 音效、积分、强制教程 |
| 首次 Onboarding 全屏层 | Flutter/CLI 动效实现 |
| WelcomeScenes 动态化（替换静态为主） | 大改聊天消息气泡布局 |
| Accept 成功落印；session-end Done 轻收卷 | 改 P0 自动写入策略 |
| i18n en-US / zh-CN | 根目录新 md |

### 1.3 用户路径变化

| 步骤 | 改前 | 改后 |
|------|------|------|
| 首次启动 | 直接进壳 + 或有 SetupBanner | 启幕（契约三句）→ Skip/开始 → 首页 |
| 空会话 | 静态四卡 | Ambient + stagger 卡 + 统一 hover |
| Accept | 列表/toast | + RitualMark 短动效 |
| 会话反思结束 | modal 关 | + 可选底部收卷摘要（不二次拦截） |

### 1.4 技术要点

- `crates/hermes-gui/ui/src/index.css` — motion/duration tokens  
- `components/common/ui.tsx` + 新 `components/motion/*` 或 `ritual/*`  
- `WelcomeScenes.tsx` / 新 `OnboardingRitual.tsx`  
- `ReflectionReview` accept 成功回调 → 落印  
- 本地 flag：`hermes.onboarding.v1.done`（localStorage 即可，无密钥）  
- 文档：`docs/gui-ritual-motion.md`（非权威）+ 本台账；**不**升 P0 除非改产品原则  

### 1.5 风险与回滚

| 风险 | 缓解 |
|------|------|
| 动效影响性能/分散注意 | 时长上限；主路径禁止长动画；reduced-motion |
| 风格再次分裂 | 组件族强制；验收检查清单 |
| 欢迎打扰老用户 | 仅首次；Skip；已有会话用户可跳过逻辑 |
| 未 build dist 以为生效 | 验收强制 run-gui.sh / npm build |

回滚：还原 ui 相关 commit；flag 不影响引擎。

### 1.6 与 P0 决策清单对齐

1. 额外安装？**否**  
2. GUI 免终端？**是**  
3. 更好用/高效/可进化？**是（引导 + 强化可审反馈）**  
4. 数据本地？**是**  
5. 非 AI 出站？**否**  
6. 引擎共享？**是（只动 GUI surface）**  
7. records？**本文件**  
8. 文档在 docs？**是**  
9–10. 两视角？**已写**  
11. 技能边界？**不改 skill**  
12. 安全？**无回归**  
13. 代码卫生？**实施阶段强制清旧 Welcome 静态冗余**

### 1.7 方案确认

- [x] 已对照 P0/P1 · **用户确认 2026-08-05：** ① 全屏启幕 ② 纯 CSS/WAAPI ③ 仅 P0 切片  

---

## 2. 实施（Implement）

- **状态：** 实施中  
- **实际改动摘要：** motion tokens；AmbientStage / MotionCard / RitualMark；OnboardingRitual；Welcome 动态化；playSeal 落印；session-end 收卷 toast；i18n；docs/gui-ritual-motion.md  
- **关键路径/文件：** `ui/src/index.css`、`components/motion/*`、`components/ritual/*`、`WelcomeScenes.tsx`、`App.tsx`、`ReflectionReview.tsx`、`chatStore`、`i18n.ts`  
- **偏离方案处：** 无；`playScroll` 暂复用落印视觉 + toast（P0 克制）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 首次打开有欢迎且可跳过 | 清 onboarding flag → run-gui | 见启幕；Skip 进主界面 | | |
| 2 | 第二次打开无强打扰 | 再开 App | 无启幕 | | |
| 3 | 空首页有动态感 | 新会话 | 背景/卡入场/hover 统一 | | |
| 4 | 批准记忆有落印 | Accept memory | ≤600ms 统一仪式，可继续点 | | |
| 5 | 不挡输入 | 对话中 | 流式与输入正常 | | |
| 6 | reduced-motion | 系统开减弱动态 | 无位移/脉冲，仅淡入 | | |
| 7 | 暗色模式 | 切 dark | token 一致、无裸硬编码色 | | |
| 8 | dist 默认路径 | run-gui.sh | 无白屏、非 5173 | | |

- **自动化：** `npm run build`（ui）；`cargo check -p hermes-gui`；全仓 clippy/test 在收工时按 P1  
- **测试结论：** 待实施  

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☐ | |
| 开箱即用未破坏 | ☐ | |
| 本地优先未破坏 | ☐ | |
| 测试通过 | ☐ | |
| 记录完整 | ☐ | |
| 产品+架构两视角齐全 | ☑ 方案阶段 | |
| 非修修补补 | ☐ | 实施须先 token/组件再页面 |
| 代码卫生 | ☐ | |

- **结论：** 方案中，**未实施、未验收**

---

## 5. 附注

### 权威约束摘要（本变更必须遵守）

| 来源 | 约束 |
|------|------|
| P0 一 | 进化候选须用户确认；仪式不得暗示未批准已写入 |
| P0 二 | 本地；欢迎 flag 可 localStorage |
| P0 三 | GUI 是 surface；不复制引擎 |
| P0 五/六 | 台账 + 说明只在 docs/ |
| P0 七/九 | 产品+架构方案；改完清旧 |
| P1 §六·附 C | GUI 默认 ui/dist；scripts/run-gui.sh |
| P1 质量门槛 | 验收前 build/测试；无噪音路径 |

### 推荐产品一句话

> 冷静工作台（蓝）+ 紫色进化时刻；首次可跳过启幕；空首页有呼吸；批准才落印。
