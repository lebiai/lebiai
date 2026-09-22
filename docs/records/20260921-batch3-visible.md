# 变更记录：批 3 —— 让你看得见（谁在说 · 字在长 · 链接点得开 · 派了子代理）

| 字段 | 内容 |
|------|------|
| **编号** | `20260921-batch3-visible` |
| **日期** | 2026-09-21 |
| **状态** | 待验收 |
| **负责人** | 引擎/桌面（Codex 会话） |
| **关联** | 前序 [`20260920-collection-critical-path`](./20260920-collection-critical-path.md)（批 2 治慢）；本批治「看不见」 |

---

## 0-fp. 第一性原理（必填 · P0 第零条）

- **拒绝的类比：** 不因为「别的聊天软件也是这么长的」就把卡顿归给模型速度；不因为「浏览器里 `<a>` 能跳」就认为 WebView 里也能跳；不把「派了 7 个子代理」写成「做了些事」凑合。
- **拆出的真：**
  1. 用户看的不是 token，是**画面在动**。画面不动 = 用户认为你在卡（哪怕引擎一直在推）。
  2. 项目组会话里**棒会换人**（海燕 → 吕老师 → 小宋）。落盘的消息如果没写「谁说的」，界面只能把两轮并成一块 —— 于是吕老师说过话，用户却看不见。
  3. 桌面壳（Tauri/WebView）**不是浏览器**：`<a target=_blank>` 什么都不会发生；只有系统 open 能力能开浏览器。
  4. 一条真实动作（派子代理并发采集）必须**有自己的说法**，否则用户看到的是无信息量的「做了事」。
- **如何从真推出：**
  - 真 1 → 把「每个 token 重解析整条 markdown + 重建全部气泡」改成**按帧合并**，并把实时块的重渲染**圈在一个组件里**；
  - 真 2 → 消息加 `speaker` 字段 + 引擎统一盖章 + 前端**换人就不并**、并署名；
  - 真 3 → 接官方 `tauri-plugin-opener`（不自建任意 URL 打开面），且把链接渲染成**来源小标签**（既点得开，又看得出是来源）；
  - 真 4 → `processKindForTool` 给 `subagent` 一个正式标签。

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** 桌面 GUI 用户（工位会话 + 项目组会话）。
- **解决什么痛点：** ① 组会话里「谁在说」看不出来；② 输出顶到一大段才蹦出来，像死机；③ 来源链接点了没反应；④ 派人干活显示「做了些事」。
- **用完后用户多得到什么：** 同一轮里**看见人换手**；文字**一行行长出来**；**点来源标签就开浏览器**；过程行直接写「派了子代理」。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（链接有外链图标 + 小标签外观）
  - [x] 不增加无意义确认或噪音
  - [x] 主操作一眼能找；空/载/错态完整
  - [x] 高频路径步骤少、界面干净

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在项目组会话里发一句话；随后看见过程行、正文、条目、来源。
- **怎么走完：** 输入 → 过程行（可展开）→ 正文**逐行**出现 → 每条末尾一枚「原文链接」标签 → 点它 → 系统浏览器打开原文页。
- **看起来怎么样：**
  - 换人时**气泡上方出现小字名字**（如「情报王海燕」），同一轮内换人才标；
  - 链接 = 圆角小标签（外链图标 + 「原文链接」），浅蓝底 / 深色下深蓝底，hover 加深；**不再是下划线裸链**；
  - 过程行的人话动词：新增「派了子代理 / 派子代理」。
- **好走 / 好看：** 不新增任何点击步骤；链接从「看不出能点」变成「一看就是能点的标签」。
- **成功标准：** 组会话里能一眼看出这一句是谁说的；正文肉眼可见在长；来源标签点开浏览器。
- **明确不做什么：** 不做「来源标签自动回链到本地文件」；不做链接预览卡片。

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：**
  1. **协议层（说话人）**：`Message` 不存说话人 → 前端 `mergeAssistantWorkSpans` **无条件**合并相邻 assistant → 吕老师被并进海燕的气泡。
  2. **渲染层（卡顿）**：`textDelta` 每个 token 一次 `set()` → 全量订阅的 `ChatView` 重渲染整条 transcript；`MarkdownContent` 非 memo 且 `components` 内联 → 每次渲染都是新组件类型 → 整棵子树卸载重建；跟随视口在**节流之前**读 `scrollHeight`（强制同步布局）。
  3. **权限/壳层（链接）**：WebView 不会自己开外链，需要 `tauri-plugin-opener` + capability。
  4. **映射层（标签）**：`processKindForTool` 没有 `subagent` 分支 → 落 `other`。
- **正确的长期默认路径：** 说话人由**引擎盖章**（`hermes_core::message::stamp_speaker`，GUI/CLI/server 三入口同一份判定），界面只负责显示；实时渲染的代价**必须和实时内容绑在一起**（谁按 token 变，谁才重渲染）。
- **与引擎/各入口边界：** `speaker` 落在 `hermes-core::Message`（共享）；三个入口各自盖章，IM 渠道/引擎批处理无人物 → `None` → 行为不变。
- **安全影响：** opener 只放开 `https/http/mailto`（`opener:default` scope），**不新增任意 URL 打开面**；`safeHref` 仍在（挡 `javascript:` / `data:` / `file:`）。
- **如何防复发：** 说话人是**一个函数**（谁少盖一处，那个入口的组会话就看不见人）；流式渲染收进 `LiveStream` 单一订阅点；链接权限进 `tests/window_capabilities.rs` 的映射表（漏授权、映射表过期都会红）。
- **为何这不是补丁：** 改动落在「消息协议 + 渲染订阅边界」，不是给某一处加特判；`Done` 事件顺序也被修正为「所有内容事件先于 Done」，从协议上保证纠正文不再作废。

---

## 1. 方案（Plan）

- **目标：** 批 3 四件事一次做完 —— 看得见谁在说、看得见字在长、链接点得开、派活有说法。
- **范围：**
  - 做：`Message.speaker` + 三入口盖章 + 前端换人不并 + 署名；按帧合并流式文本 + `MarkdownContent` memo/常量提升 + `LiveStream` 收窄订阅 + 跟随视口节流修正；`Done` 顺序修正；`tauri-plugin-opener` + 来源小标签；`subagent` 过程标签。
  - **不做**：虚拟列表在流式期间开启；直播正文只渲染尾部窗口（会「吃掉」用户已经看到的字）。
- **用户路径变化：** 改前「一轮下来只有一个气泡、最后一次性蹦字、链接点不动」→ 改后「气泡上署名、字在长、标签点得开」。
- **技术要点：** `hermes-core`（message）/ `hermes-gui`（commands::chat、main.rs、capabilities、Cargo.toml）/ `hermes-server`（routes::chat）/ 前端（chatStore、ChatView、MarkdownContent、processLabel、i18n、index.css）。
- **风险与回滚：** rAF 合并若在窗口隐藏时不触发 → **每个非文本事件与回合收尾都先 flush**，不会丢字；opener 若权限写错 → `tauri-build` 编译期即报错。
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-09-21 · Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `Message` 新增 `speaker`（serde default / skip_none），新增**唯一**盖章函数 `stamp_speaker`（只盖 assistant；`None` 不盖）；GUI / CLI / server 三入口盖章；GUI 序列化 `MessageData.speaker`。
  2. 前端 `mergeAssistantWorkSpans` 加 `prev.speaker === row.speaker` 条件 —— **换人不并**；`ChatView` 算出逐行署名（换人才标）。
  3. `chatStore` 流式文本**按帧合并**（`textDelta`/`thinkingDelta` 进缓冲，rAF flush；`toolUseStart`/`error`/`cancelled`/`textCorrected`/`done` 先 flush）。
  4. `MarkdownContent`：`remarkPlugins` 与 `components` 提为模块级常量 + `memo()`。
  5. `ChatView` 不再订阅流式三件套；跟随视口逻辑移入 `LiveStream`（先过节流再读 `scrollHeight`）。
  6. `commands::chat`：`TurnEvent::Done` 先扣住，正文（sanitize 后的 `TextCorrected`）发完再放 —— 修正「纠正文落到已收摊的界面」。纠正文的粒度抽成 `last_assistant_text`（**只取最后一段** assistant 正文）：取全部会把过程旁白灌进回答气泡。
  7. 新增 `tauri-plugin-opener`（`Cargo.toml` / `main.rs` / `capabilities/default.json` 的 `opener:default` / 前端依赖），`a` 渲染器改成**来源小标签**并 `openUrl` 打开系统浏览器（浏览器预览时退回 `window.open`）。
  8. `processKindForTool` 新增 `agent` 分支（`subagent`）+ i18n（「派了子代理 / 派子代理」「Sent a helper / Sending a helper」）。
  9. **看得见产出**：落盘的消息只留工具入参、不留 `toolExecStart` 的 summary，折叠行于是只能说「写了文件」。新增 `summaryFromToolInput`（按引擎 `hermes_turn::tool_call_summary` 的同一条键位规则从 `input` 还原），装配 `ToolCallView` 时补上；`processHeadline` 新增 `finishedObject`，干完后折叠行直接写**产出文件名**。
- **关键路径/文件：**
  - `crates/hermes-core/src/message.rs`（字段 + `stamp_speaker` + 3 条断言）
  - `crates/hermes-gui/src/commands/chat.rs`（盖章 + Done 顺序）
  - `crates/hermes-gui/src/main.rs` · `capabilities/default.json` · `Cargo.toml`
  - `crates/hermes-server/src/routes/chat.rs`（盖章）
  - `crates/hermes-gui/ui/src/store/chatStore.ts` · `components/chat/ChatView.tsx` · `components/chat/MessageBubble.tsx` · `components/common/MarkdownContent.tsx` · `utils/processLabel.ts` · `utils/displayMessages.ts` · `i18n.ts` · `index.css`
- **偏离方案处：** 无（来源小标签是用户明确要过的形态，本批一并做掉）。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 组会话里换人说话，看得出换了人 | 两条相邻 assistant，`speaker` 不同 → `coalesceMessagesForDisplay` | **3 行**（user / 海燕 / 吕老师），不并 | 通过 | esbuild 打包真实模块 + node 跑，见附注 |
| 2 | 同一个人相邻两条仍然并成一块 | 同上但 `speaker` 相同 | 2 行（并） | 通过 | 老行为不变 |
| 3 | 老会话（没盖过章）行为不变 | `speaker` 都缺省 | 2 行（并） | 通过 | 兼容 |
| 4 | 只给 assistant 署名 | `stamp_speaker(Some)` | user 不变、assistant 盖章 | 通过 | `cargo test -p hermes-core` |
| 5 | 没人物的会话不凭空署名 | `stamp_speaker(None)` | 原样 | 通过 | 同上 |
| 6 | 署名活过落盘与上下文瘦身 | `for_persist` / `without_thinking` | 仍带 speaker | 通过 | 同上 |
| 7 | 过程行说得出「派了子代理」 | 渲染含 `subagent` 的工具卡 | 过程行写「派了子代理，在工作区做了事」，**不再**是「做了些事」 | 通过 | 真机截图（附注） |
| 8 | 来源链接是可点的小标签 | 渲染资讯格式正文 | 每条末尾一枚外链图标小标签，居中留白正常、不换行断裂 | 通过 | 真机截图 |
| 9 | 点标签开系统浏览器 | Tauri 内点标签 | 系统浏览器打开该 URL | **待用户目视**（能力已由 `tauri-build` 校验：`opener:default` 已解析进 `capabilities.json`，manifest 含 `open_url`） | 需重启 GUI |
| 10 | 字一行行长出来 | 桌面发一句长回答 | 逐行长出，不再「卡住 → 整段蹦出」 | **待用户目视** | 需重启 GUI |
| 13 | 链接权限漏配会红（防复发） | `cargo test -p hermes-gui --test window_capabilities` | 指纹 `openUrl(` 与 `opener:allow-open-url` 双向校验 | 通过 | 做过**变异验证**：把指纹改成 `openUrlX(` → 测试如约失败（「UI 源码里已找不到」），改回即绿 |
| 12 | 纠正文不把过程旁白灌进回答 | 一轮里两条 assistant（前一条是旁白 + 工具），取纠正文 | 只取后一条正文，多块拼成一段 | 通过 | `cargo test -p hermes-gui` |
| 11 | 看得出这次落了哪个文件 | 折叠的过程行（含 `write`/`edit`） | 折叠行写「写了文件：outputs/2026-09-20/资讯-2026-09-20.md」 | 通过 | 真机截图（附注）；`input` 缺省时退回旧文案 |

- **自动化：** `cargo test --workspace` → **全绿**（历史欠账，本批首次补跑：hermes-core 128、hermes-gui 49+2+2、hermes-tools 125、hermes-channel 43、hermes-reflect 76、hermes-memory 81、hermes-skills 39、hermes-store 33、hermes-turn 31、hermes-llm 29、hermes-commitments 24、hermes-sources 19、hermes-cli 10、hermes-server 8+8、hermes-feishu 3、hermes-weixin 3、hermes-mcp 2、doc-tests 若干 0；`live_do_path` 的 2 条 live 用例按其 `#[ignore]` 标注为 ignored）；`cargo clippy --workspace --all-targets -- -D warnings` → **0 warning**；`cargo fmt --all --check` → 仅剩**用户 WIP** 的 `crates/hermes-gui/src/commands/episode.rs:165`（本批未碰，未顺手改）；前端 `npm run build`（`tsc && vite build`）→ 通过；`cargo build -p hermes-gui` → 通过。
- **手工（防复发测试变异验证）：** `window_capabilities` 的映射表加一条 `openUrl(` → `opener:allow-open-url`；故意把指纹写错跑一次 → **失败**（不是假绿）；改回 → 通过。
- **手工（引擎侧顺序）：** 逐个 `Ok(TurnOutput)` 出口核对 `hermes-turn` 的 `TurnEvent::Done`（830 正常收尾 / 304、580、643 三处取消 / 其它 `Err` 出口在 GUI 手动补发）→ 一轮**恰好**一次 `Done`，扣住再放不会漏。
- **手工：** 用备用 vite harness（`.harness/`，**验收后已删除**）渲染真实 `MessageBubble`，拿 headless Chrome 截图目视：署名、来源标签、过程行动词。
- **测试结论：** [x] 全部通过 · [ ] 有已知问题 → 见遗留项

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 「谁在说 / 字在长 / 点得开 / 派了人」四件事各自有可见证据 |
| 开箱即用未破坏 | ☑ | 只加插件与前端依赖；无新中间件 |
| 本地优先未破坏 | ☑ | 未改数据路径；speaker 与消息同文件落盘 |
| 测试通过 | ☑ | `cargo test --workspace` 全绿 + clippy 0 warning + 前端 build + `cargo build -p hermes-gui` |
| 记录完整 | ☑ | 本文件 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补（默认路径正确） | ☑ | 盖章唯一函数 + 渲染订阅边界 |
| 代码卫生：高效无冗余、旧代码/注释/入口已清理 | ☑ | 删掉 `ChatView` 里旧的全量订阅与旧滚动效应；`MarkdownContent` 内联 `components` 已清 |
| 操作与视觉：走查完成，好看、好走 | ☑ | 截图目视（署名 + 来源标签 + 过程行） |
| 第一性原理：三步写全 | ☑ | §0-fp |

- **验收人：** 用户
- **验收日期：**
- **结论：** ☐ 通过 · ☐ 驳回（原因：）
- **遗留项：**
  - 第 9 / 10 项要**重启桌面 GUI**后由用户目视确认；
  - `cargo test --workspace` 里那条 `live_do_path` 二进制在 macOS 上会**静默停几十秒到十几分钟**：不是代码问题，是 XProtect/Gatekeeper 在扫新编出来的测试二进制（`XprotectService`/`syspolicyd` 各占 ~30% CPU，而它 0 CPU 挂在那儿）。第二次跑就快。
  - 流式「看得见」的量化只到「机制层」（帧合并 + 订阅收窄），**未**在 WebView 里测过真实帧率；
  - 「来源小标签」目前对**所有**安全链接生效（含正文中段链接），若用户觉得过重，可收窄为「仅在行尾 / 词面为『链接/来源』时」。

---

## 5. 附注

- 换人不并的实测输出（`esbuild` 打包真实 `displayMessages.ts` 后在 node 跑）：
  - `换人 → rows: 3 ["user(干活)","assistant:wang-hai-yan(海燕收完了)","assistant:lv-lao-shi(我来定调)"]`
  - `同人 → rows: 2 ["user(干活)","assistant:wang-hai-yan(结论如下)"]`
  - `无署名 → rows: 2 ["user(干活)","assistant(结论如下)"]`
- 视觉截图：headless Chrome 打开 harness（`http://127.0.0.1:8792/` / `:8793/`）→ 见本轮会话附图；截图文件 `/tmp/harness-shot.png`、`/tmp/harness-shot3.png`（临时）。
  - 第 3 张：折叠行写「写了文件：outputs/2026-09-20/资讯-2026-09-20.md」；同一张里「派了子代理，在工作区做了事」与「情报王海燕 / 吕老师」两个署名也在。
- 事件顺序修正的依据：`TextCorrected` 由 `commands::chat` 在 `run_turn` **返回后**发（要等 sanitize），而 `Done` 原本在 `run_turn` **内部**发 → 界面已 `isStreaming=false`，直播块不再渲染，纠正文只留在 state 里 = 白纠。

---

## 6. 追加（同日第二轮 · 用户点单「2 → 1 → 4 → 3」）

### 6.1 改了什么（4 + 1 件）

1. **产出小标签可点开**（省一步）：折叠行右侧多一枚小标签（`📄 2026-09-20/资讯-2026-09-20.md`），点它**直接打开**这份文件——复用「我的材料」那条 `commands::outputs::open_output`（越界 / 软链都在引擎侧挡过、有测试、**不需要新权限**）。标题里不再重复写路径；最多摊三枚，多的写「还有 N 个」。
   - 两个来源都认：刚落盘的消息带 `input`、正在流的工具只有 `toolExecStart` 的 summary（引擎拼的 `write: <path>`）——`artifactPathsOf` 统一。
2. **来源小标签收窄**：只有「短标签」（原文链接 / 来源 / 36氪）和**裸网址**才做成小标签；包着一整句话的链接保持普通链接的样子（通篇标签反而吵）。裸网址只显示**域名**（`pbc.gov.cn`），完整地址进 `title`——真实生成物里来源是长 URL，整条塞进标签会把行撑爆。
3. **流式渲染节流（80ms）** —— 原计划的「只渲染尾部窗口」被**实测否掉**：
   - 客户端实测（同一份资讯卷切片、`flushSync` 计时）：1k 字 **2.6ms**、2k 2.05ms、4k 2.4ms、8k **4.1ms**、12k 5.67ms、20k **9.0ms**（第二次跑 9.7ms）。
   - 尾部窗口只把 9ms 压到 4.1ms，却要**藏掉用户已经看到的字** → 不划算。改成把**渲染**节流到 80ms：主线程从「62 次/秒 × 9ms ≈ 56%」降到「14 次/秒 × 9ms ≈ 13%」，且一个字都不藏（真值在 store 里，落盘/定格仍是全文）。
   - 用真实 `useThrottledValue` 跑探针：**1 秒内 62 → 14 次渲染**（实测，两次一致）。
4. **暗色正文配色**（量暗色时顺手发现并修）：`.prose-chat` 在同一元素上覆盖了 `dark:prose-invert` 设的 `--tw-prose-*`（同优先级 → 按源码顺序浅色值赢），暗色下正文是 `#0f172a` 深藏青压在近黑底上——**几乎看不见字**。补 `html.dark .prose-chat`（0,2,0）一份暗色 token；截图前后对比确认可读。

### 6.2 文件
- `crates/hermes-gui/ui/src/utils/processLabel.ts`（`artifactPathsOf` / `artifactLabel` / `toolKeyValue`）
- `crates/hermes-gui/ui/src/utils/useThrottledValue.ts`（新增）
- `crates/hermes-gui/ui/src/components/chat/MessageBubble.tsx`（产出标签 + 节流接入）
- `crates/hermes-gui/ui/src/components/chat/ChatView.tsx`（`LiveStream` 用节流值）
- `crates/hermes-gui/ui/src/components/common/MarkdownContent.tsx`（`chipLabelOf` 收窄 + 域名）
- `crates/hermes-gui/ui/src/index.css`（来源标签 label 截断 · 暗色 prose token）
- `crates/hermes-gui/ui/src/i18n.ts`（`message.artifactOpen/Failed/More`）

### 6.3 测试

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 14 | 折叠行看得出落了哪个文件，并且点得开 | 真机渲染含 `write` 的一轮 | 折叠行右侧一枚文件小标签；点击走 `open_output` | 通过（截图；点击链路复用已有命令与既有测试） |
| 15 | 裸长 URL 不再撑爆版面 | 渲染真实资讯卷（来源是长 URL） | 显示 `pbc.gov.cn` / `gov.cn` / `ndrc.gov.cn`；`title` 里是完整地址 | 通过（截图） |
| 16 | 一整句话的链接不被改成标签 | 正文中段链接 | 保持普通链接样式 | 通过（代码路径 + 截图未见标签） |
| 17 | 字不再「卡住然后整段蹦」 | 真实 `useThrottledValue` 探针 1 秒 | 渲染次数 62 → **14** | 通过 |
| 18 | 暗色下正文看得见 | 暗色截图 | 正文浅色可读、标签蓝底可辨 | 通过（前后各一张截图） |
| 19 | 构建不破 | `npm run build`（`tsc && vite build`） | 通过 | 通过 |

- **自动化：** 本轮只动前端（无 Rust 改动），`npm run build` 通过；Rust 侧沿用本轮已全绿的 `cargo test --workspace` / `clippy -D warnings`。
- **手工：** 备用 vite harness 渲染真 `MessageBubble` + 真实资讯卷，headless Chrome 出亮/暗两张图；临时目录已删。
- **测试结论：** [x] 全部通过（第 14 条的**点击效果**仍需用户在重启后的 GUI 里点一下确认——我无法在无头环境点 WebView）。

### 6.4 遗留（更新）
- 节流是**浏览器引擎实测**（Chromium）；桌面壳是 WebKit，量级应接近但**没在真机量过**。
- 产出小标签最多 3 枚；同一轮写 4 个以上文件时靠展开逐条看。
- 上一轮的遗留照旧：链接「点开浏览器」、字「逐行长出」两条要重启 GUI 后目视确认。
