# 变更记录：推理可见 + 推理不再吃光输出预算 + 收尾补判据（批 1 · 治慢的根）

| 字段 | 内容 |
|------|------|
| **编号** | `20260920-reasoning-visible-and-budget` |
| **日期** | 2026-09-20 |
| **状态** | 待验收（单测全绿；实跑待人眼点头） |
| **负责人** | Agent + 用户 |
| **关联** | 台账 `20260920-empty-assistant-400`、`20260920-source-entries-method`；用户吐槽「慢」「文字输出前页面卡死」「一个字不说卡八分钟」 |

---

## 0-fp. 第一性原理

- **拒绝的类比：** 不是「别的 AI 也慢，所以慢是正常的」；也不是「换个更快的模型就好了」——
  换模型是把症状挪走，不是把账算清。
- **拆出的真：**
  1. 用户感知的「慢」= **从按下回车到看见第一个字的时间**，不是整轮总时长。
     一整轮 50 秒但第 1 秒就有字在动，和 8 分钟纹丝不动，是两种感受。
  2. 那一整轮的 `output_tokens` 里，**约四成是推理**，它和正文**共用同一个 `max_tokens`**。
     推理不是「额外开销」，是**和正文抢同一个额度**。抢光了，正文一个字都写不出来。
  3. 「零字」有两种：**没进历史**（内存里丢掉）和**进了历史**（之后每次请求都被 400）。
     前者是白等，后者是会话中毒。
- **如何从真推出：**
  1 → 推理必须**流到用户眼前**（它本来就在生成，只是我们扔了）：解析 → ThinkingDelta → 界面。
  2 → 零字且撞上限时，**同一轮加预算重来一次**；本来那一轮就是要作废的，重来只有赚。
  3 → 所有 push assistant 的地方都要过**同一个判据**（`Message::has_sendable_content`），一处不许漏。

---

## 0. 用户价值

- **谁用：** 每一个在 GUI / CLI 里发消息的人。
- **解决什么痛点：** ①「文字输出前页面卡死」——现在一按下回车就能看见它在想什么，
  一直在流；②「一个字不说卡八分钟」——推理吃光预算的那一轮不再白等，自动加预算重来；
  ③ 会话中毒（400）最后一条漏网的路被堵上。
- **用完后用户多得到什么：** 等待从**黑盒**变成**看得见**；偶发的「这轮什么都没出」变成
  「这轮重来了一次，出东西了」。
- **好用性自检：**
  - [x] 不需要额外运行时（不加依赖、不加配置）
  - [x] 步骤可感知（第一个字更早出现；重来时会明说）
  - [x] 不增加无意义确认
  - [x] 空/载/错态：重来只有一次，第二次仍零字就给一句人话
  - [x] 高频路径步骤少（常态下零额外开销：没有推理就不走这条路）

---

## 0b. 产品经理视角

- **场景：** 用户在「情报王海燕」工位说「开工」，等它采一期资讯。
- **怎么走完：** 用户按下回车 → **立刻**看到「在想这件事」下面有字在滚（那就是推理）→
  工具开始跑 → 正文一条条出来。
- **看起来怎么样：** 不再是空白气泡 + 转圈；思考区是折叠的、可展开的，和正文分开。
- **空/载/错态：** 推理把预算吃光时，先出现一行「（推理把这一轮的输出预算用完了，我加到
  N tokens 重来一次。）」，然后真的重来；第二次还是零字，才报「这一轮没能产出内容」。
- **成功标准：** 「按下回车到第一批字出现」的间隔**肉眼可见地缩短**；「一个字不说」不再是
  一种可观察的失败。
- **明确不做什么：** 不做推理折叠的视觉改版（那是批 3 的事）；不给推理单独设 API 参数
  （DeepSeek 的 OpenAI 兼容线没有这个口子，我们的做法必须对**所有** OpenAI 兼容端点成立）。

---

## 0c. 架构师视角

- **根因层级：** provider 解析层（`openai.rs` 读响应时**丢掉了 `reasoning_content`**）
  ＋ turn 循环（截断零字后**没有第二次机会**）＋ 收尾路径（**漏了**空消息判据）。
- **正确的长期默认路径：**
  - 推理是**一等内容块**（`ContentBlock::Thinking`），从 provider 到界面走同一条链，
    与 Anthropic 线一致；`has_sendable_content` 永远认它「不算内容」。
  - 撞上限且零正文 = 这一轮**作废**，所以自动重来一次是净收益；上限只加一倍、只加一次。
  - 「零字 → 换一句人话」的判据**只有一处**（`has_sendable_content`），任何 push assistant
    的地方都必须过它。
- **与引擎/各入口边界：** 只动 `hermes-llm` 与 `hermes-turn`；GUI / CLI / server / IM
  全都自动受益，因为它们走的是同一条 TurnEvent 链，无需改各自入口。
- **安全影响：** 无新增权限、无新增网络出口。推理块**不上线**（`translate_outbound` 丢弃），
  所以不会把上一轮的思考当成上下文再喂回去。
- **如何防复发：** 三个新单测各盯一条：`reasoning_content` 必须解成 ThinkingDelta；
  零字截断只重来一次；收尾零字不许留空 assistant。
- **为何这不是补丁：** 不是给某一次报错加特判，而是把推理**接回主链**、
  把「作废的一轮」变成「可重来的一轮」、把最后一条漏判据统一到唯一判据上。

---

## 实施

- [x] `crates/hermes-llm/src/openai.rs`
  - `ChatMessage` 新增只读字段 `reasoning_content`（`skip_serializing_if` + 四个出站构造点
    全填 `None`，**永不上线**）。
  - 非流式 `into_completion()`：推理解成 `ContentBlock::Thinking`，**插在文本之前**。
  - 流式 `handle_line()`：`delta.reasoning_content` → `StreamEvent::ThinkingDelta`（0 号块）；
    文本块下标在其后顺延（`usize::from(!thinking_buf.is_empty())`）。
  - 流式 `finalise()`：块下标改成按顺序数出来（思考 → 文本 → 工具），
    `BlockStop` 顺序与 `content` 顺序一致。
- [x] `crates/hermes-turn/src/lib.rs`
  - 新增 `TRUNCATION_RETRY_CEILING = 32_768`。
  - 轮次主体包进 `'attempt` 循环：`stop_reason == MaxTokens` **且**零可发内容 **且** 还没重来过
    **且** 预算未到顶 → 预算翻倍重来一次；重来前明说一行，花掉的 token **照实报**。
  - 收尾那一次（工具轮次打满后的无工具收尾）补上同一条判据：零字换成一句人话，
    不再把空 assistant 推进历史。

## 测试

- [x] `cargo test -p hermes-llm` → **28 passed**，含 4 条新增：
  - `reasoning_content_decodes_to_a_thinking_block_ahead_of_the_text`
  - `a_reasoning_only_response_is_not_sendable_content`
  - `streaming_reasoning_becomes_thinking_deltas`
  - `a_stream_without_reasoning_keeps_its_old_numbering`（防回归：没推理的流编号不许变）
- [x] `cargo test -p hermes-turn` → **30 passed**，含 3 条新增：
  - `a_reasoning_only_truncated_round_retries_once_with_a_bigger_budget`
  - `the_truncation_retry_happens_at_most_once`
  - `an_empty_closing_synthesis_never_leaves_an_empty_assistant`
- [x] 下游入口不受影响：`cargo test -p hermes-channel -p hermes-gui -p hermes-cli`
  → **43 / 10 / 48 + 2 + 2 passed，0 failed**。
- [x] `cargo clippy --workspace --all-targets -- -D warnings` → 全绿。
- [x] `cargo fmt` 本次改动文件 → 无 diff。

## 验收

- [x] 实跑一轮：推理在**第一个字**那一秒就流出来（不再有「卡住」的黑盒段）。
- [x] 实跑一轮后确认：会话历史里没有空 assistant。
- [ ] 用户人眼点头。

### 实跑证据（2026-09-20 12:50，隔离数据根 `/tmp/lebi-stream-test2`）

命令：`hermes chat`，回车后输入「用三句话说明什么是通货膨胀。」

| 时刻 | 发生了什么 |
|---|---|
| 3.00s | 用户按下回车 |
| **3.59s** | **第一行推理出现在屏幕上**（`💭 The…`）——距回车 **约 0.6 秒** |
| 4.37s | 推理滚完（310 字），正文开始流 |
| 5.05s | 整轮结束，回到提示符 |

落盘会话逐块核对：`user [text]` → `assistant [thinking(310), text(163)]`。
即**推理真的进了内容块**（不再凭空丢掉），且这条 assistant **有可发内容**，
不会在下一轮把会话打成 400。

修复前的对照：同一条命令实测**一个字都不吐**，界面纹丝不动；`reasoning_content`
在 provider 层就被丢掉了，用户看到的只有等待。

## 遗留

- 推理与正文仍共用 `max_tokens`（API 没有分开的口子）——本轮的做法是「抢光了就重来」，
  不是「不让它抢」。真正的分离要等 provider 提供独立推理额度。
- `parse_usage()` 仍把 `cache_read_tokens` 写死 0（DeepSeek 会返回
  `prompt_cache_hit_tokens`）→ 现在**量不到**缓存命中率。批 2（治轮次与重发）需要这把尺子，
  到时候一并接上。
- GUI/CLI 对「推理」的视觉呈现本轮未动（沿用现成的折叠「思考中」）。
