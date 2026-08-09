# GUI 仪式感与动效

| 字段 | 内容 |
|------|------|
| **定位** | 非权威实现说明；**系统方案**见 `docs/records/20260805-gui-ritual-system.md` |
| **验收** | 系统台账 **已验收**（2026-08-06 用户「仪式通过」）；本文件仅实现备忘 |
| **技术** | 纯 CSS + 既有 React；无 motion 库 |
| **加载** | 默认 `ui/dist`；改后须 `npm run build` / `scripts/run-gui.sh` |
| **产品红线** | 新聊天首页**禁止**推销「反思 / Reflect」；仪式挂用户时刻，不挂引擎模块名 |

## 视觉语义

| 用途 | Token |
|------|--------|
| 工作台 | `app-primary`（蓝） |
| 进化时刻 | `app-accent`（紫）— 落印 / 欢迎契约 / micro 反思区 |

## Motion tokens（`index.css`）

- `--motion-fast` / `--motion-base` / `--motion-ritual`
- `--ease-out-soft`
- 工具类：`fade-up-in`、`stagger-in`、`ambient-breathe`、`motion-lift`、`ritual-seal-pop`
- `prefers-reduced-motion: reduce` 下关闭位移与循环动画

## 组件

| 路径 | 职责 |
|------|------|
| `components/motion/AmbientStage` | 首页/欢迎背景光 |
| `components/motion/MotionCard` | 统一可点卡片 |
| `components/ritual/RitualMark` | 图标方块舞台 |
| `components/ritual/OnboardingRitual` | 全屏首次启幕 |
| `components/ritual/RitualSealHost` | 批准后落印 |
| `utils/onboarding.ts` | `localStorage` flag |
| `utils/ritual.ts` | `playSeal` / `playScroll` |

## 用户路径

1. **首次打开** → 全屏启幕（Skip / 开始对话 / 去填 Key）→ 写 `hermes.onboarding.v1.done`
2. **空会话** → 动态 Welcome
3. **Accept 记忆/技能** → 落印 + toast
4. **会话反思 Done** → 轻收卷提示 toast

## 重看欢迎

- **设置 → 欢迎与仪式 → 立即显示欢迎**
- 或清除 localStorage：`hermes.onboarding.v1.done`

## 可见性尝试（部分已纠偏）

- 落印遮罩 / 首次加长：可保留为「写入成功」符号
- ~~空首页「打开反思」~~ **已删除**（用户路径错误）
- 系统落地顺序：A 骨架统一 → B 对话过程 → C 被记住 → D 启幕/首页质感 → E 收束/高级

## 重看欢迎

- 设置 → 欢迎与仪式 → 立即显示欢迎  
- 或 `localStorage`：`hermes.onboarding.v1.done`
