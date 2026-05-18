# 我用 Rust 写了一个会自我进化的 Agent，它可能是目前架构最优雅的开源实现

> 不是套壳，不是 Demo，是一个真正能跑、能记忆、能反思、能进化的 Agent 系统。11 个 crate，4000 行核心代码，每一行都在回答同一个问题：**Agent 的本质到底是什么？**

## 从一个灵魂拷问开始

市面上的 Agent 框架，99% 在做同一件事——把 LLM 的输出解析成 JSON，调用一下工具，再把结果塞回去。循环几轮，收工。

这不叫 Agent。这叫**带工具的聊天机器人**。

真正的 Agent 应该是什么样的？

- 它应该有**记忆**，而不是每次对话都从零开始
- 它应该会**反思**，从自己的行为中提取经验
- 它应该能**进化**，下一次比上一次做得更好
- 它应该有**纪律**，知道什么能做、什么要先问人
- 它应该**高效**，不会傻傻地串行等待每一个工具返回

这就是 Hermes —— 一个用纯 Rust 构建的自进化 Agent 系统。

## 架构：11 个 crate 的交响乐

```
hermes-core        — 类型系统：Message、ContentBlock、ToolHost trait、上下文压缩
hermes-llm         — LLM 适配层：Anthropic / OpenAI / DeepSeek，流式 SSE
hermes-turn        — 核心执行引擎：单轮 tool loop + 权限 + 并行
hermes-tools       — 13 个内置工具：read/write/edit/bash/grep/glob/git/think/todo/...
hermes-mcp         — MCP 协议层：stdio + Streamable HTTP，无限扩展工具生态
hermes-memory      — 记忆宫殿：zone 分区 + 超越链 + 效果追踪 + 遗忘衰减
hermes-skills      — 技能系统：触发匹配 + 效果追踪 + BM25 混合检索
hermes-reflect     — 自省引擎：micro 反思 + 全量反思 + profile 编译
hermes-store       — 会话持久化：JSONL append-only log
hermes-cli         — 命令行前端：REPL + slash commands
hermes-gui         — 桌面前端：Tauri + 确认弹窗 + 流式渲染
```

不是所有 crate 都一样重要，但每一个都只做一件事。这是 Rust workspace 的哲学——**编译器帮你守住边界**。`hermes-memory` 不知道 `hermes-tools` 的存在，`hermes-turn` 不关心你用的是哪个 LLM。依赖是单向的，职责是清晰的。

这不是过度设计。这是**活下去的设计**。当你要换一个 LLM provider，当你要加一个新的前端，当你要给记忆系统加一个新的存储后端——你改一个 crate，其他的纹丝不动。

## 记忆宫殿：不是数据库，是认知架构

大多数 Agent 的"记忆"是什么？一个 vector database，存一堆 embedding，检索的时候做个余弦相似度。

这很蠢。

人类的记忆不是这样工作的。你不会把"老板喜欢简洁的代码风格"和"昨天的会议纪要"存在同一个抽屉里。你的大脑有**分区**，有**层次**，有**遗忘曲线**。

Hermes 的记忆系统就是这样设计的：

```
记忆宫殿（Memory Palace）
├── core/          — 用户身份、偏好、原则（几乎不变）
├── work/          — 当前焦点、近期决策（中频更新）
├── project:xxx/   — 项目级约定和上下文（按项目隔离）
├── episode/       — 会话摘要（高频写入）
└── general/       — 未分类（兜底）
```

每条记忆不只是一段文本。它有完整的元数据：

```yaml
id: a3f7b2c1           # UUID，全局唯一
source: reflection      # 来源：用户说的 / 反思生成的 / 导入的
confidence: medium      # 置信度：low / medium / high
zone: core              # 所在区域
tags: [rust, style]     # 标签
pinned: false           # 是否钉选（钉选的永远注入 system prompt）
supersedes: [older-id]  # 超越链：这条记忆替代了哪条旧记忆
```

**超越链（Supersedes Chain）** 是最精妙的设计之一。当 Agent 学到新东西，它不会删除旧记忆，而是创建一条新记忆并标记"我替代了那条"。`list_active()` 会自动过滤掉被传递性超越的记忆。这意味着：

- 记忆的**历史**被保留了（你可以追溯 Agent 的认知变化）
- 更新是**原子的**（写入新记忆和标记旧记忆是一个操作）
- 不会丢数据（最坏情况只是多了一条冗余记忆）

### 效果追踪：记忆也有 KPI

这是我最得意的设计。

大多数系统只关心"记忆有没有被检索到"。但被检索到不代表有用。你把一条记忆塞进了 context，LLM 可能完全没理它。

Hermes 追踪两个事件：

- **Loaded** — 记忆被注入到 context 中
- **Referenced** — LLM 的回复中实际引用了这条记忆的内容

```rust
// 检查 LLM 回复是否真的引用了注入的记忆
let body_fragments = mem.body.split_whitespace().collect::<Vec<_>>();
let fragment_len = body_fragments.len().min(5);
if fragment_len >= 3 {
    let probe: String = body_fragments[..fragment_len].join(" ");
    if assistant_text.contains(&probe) {
        record_memory_stat(MemoryStatEntry {
            at: Utc::now(),
            memory_id: id.clone(),
            event: MemoryEvent::Referenced,
        });
    }
}
```

基于 Referenced / Loaded 比率，每条记忆会得到一个 **effectiveness factor**（0.5 ~ 1.0）。低效记忆在后续检索中会被降权——但永远不会降到 0，因为它可能只是暂时不相关。

这就像一个**自然选择**系统。有用的记忆越来越容易被召回，无用的记忆逐渐沉默。但都还活着，等待被重新激活的那一天。

## 自省引擎：Agent 的元认知

如果记忆宫殿是 Agent 的海马体，那自省引擎就是它的前额叶皮层。

Hermes 有两种反思模式：

### Micro 反思：每一轮对话后的快速复盘

```rust
// 每轮对话后，后台异步执行
if should_micro_reflect(&turn_messages, turns_since_last_reflect) {
    tokio::spawn(async move {
        match micro_reflect(provider, &turn_messages, &skills, &memories).await {
            Ok(output) if !output.is_empty() => {
                // 产出：记忆候选、技能候选、冲突检测
                for candidate in &output.memory_candidates {
                    if eligible_for_auto_accept(candidate) {
                        memory_store.put(candidate.scope, fm, &candidate.fact);
                    } else {
                        deferred_save(DeferredCandidate::Memory(candidate));
                    }
                }
            }
        }
    });
}
```

注意几个关键点：

1. **异步后台执行** — `tokio::spawn`，不阻塞用户的下一轮输入
2. **产出结构化** — 不是一段模糊的"反思文本"，而是具体的候选记忆、候选技能、冲突检测
3. **审慎持久化** — 只有满足条件的候选才会自动保存，其余**延迟保存**等用户审核

延迟保存（Deferred Curation）是安全设计的核心。Agent 不会偷偷修改自己的记忆库，每一次持久化都有审计日志：

```rust
log_append(ReflectLogEntry {
    at: Utc::now(),
    session_id: session_id.clone(),
    kind: CandidateKind::Memory,
    action: ActionTaken::AutoAccept,
    label: candidate.fact.lines().next().unwrap_or("").to_string(),
});
```

### 全量反思：会话级的深度复盘

用户可以随时触发 `/reflect`，对整个会话进行深度分析。这时候 LLM 会审视完整的对话历史，提取：

- 值得记住的**事实和偏好**
- 可以固化为**技能**的操作模式
- 与现有记忆的**冲突**

### Profile 编译：从碎片到画像

记忆多了以后，每条都注入 system prompt 太浪费 token。Hermes 的解决方案是 **profile 编译**——用 LLM 把所有活跃记忆合成一份简洁的用户画像：

```
/compile  →  所有活跃记忆  →  LLM 合成  →  profile.md（~200 tokens）
```

编译后的 profile 替代了逐条记忆注入，token 消耗骤降，但关键信息一条不丢。

## 技能系统：可进化的行为模式

记忆是"知道什么"，技能是"怎么做"。

```yaml
# ~/.small-rust-hermes/skills/user/code-review.md
---
name: code-review
description: 代码审查的标准流程
triggers: [review, 审查, PR, code review]
always_active: false
---
1. 先看 git diff，理解变更范围
2. 检查是否有测试覆盖
3. 关注安全相关的变更（auth, input validation）
4. 检查命名和代码风格一致性
5. 最后给出 Changed / Verified / Risks 总结
```

技能通过 **BM25 + 触发词** 混合匹配。用户说"帮我 review 这个 PR"，`code-review` 技能就会被激活，它的 body 会被注入到当轮的 system prompt 中。

和记忆一样，技能也有效果追踪：

- **Matched** — 技能被触发匹配
- **Used** — LLM 的回复中实际使用了技能内容

低效技能会被降权。这意味着 Agent 的技能库会**自动优化**——用过的、有效的技能排名上升，从未被 LLM 采纳的技能逐渐退居幕后。

## 工具并行：不是优化，是正确

当 LLM 在一轮回复中请求 3 个 `grep` 调用时，为什么要等第一个完成再开始第二个？

Hermes 的工具执行是三阶段的：

```
Phase 1: 分类
  ├── Denied（权限拒绝）→ 立即返回错误
  ├── Safe（允许 / 非危险的 Prompt）→ 收集到 safe_calls
  └── Dangerous（危险的 Prompt）→ 收集到 confirm_calls

Phase 2: 安全工具并行执行
  futures::future::join_all(safe_calls)  // 并发，不是并行
  
Phase 3: 危险工具串行确认
  for call in confirm_calls {
      ask_user_confirmation();
      if approved { execute(); }
  }
```

核心实现只有 30 行：

```rust
let futs = safe_calls.into_iter().map(|(id, name, input)| {
    let on_event = &on_event;
    async move {
        let outcome = match host.call(&name, input).await {
            Ok(o) => o,
            Err(e) => ToolCallOutcome {
                content: format!("tool call failed: {e}"),
                is_error: true,
            },
        };
        on_event(TurnEvent::ToolUseResult { id: id.clone(), .. });
        ContentBlock::ToolResult { tool_use_id: id, .. }
    }
});
let parallel = futures::future::join_all(futs);
tokio::select! {
    biased;
    _ = &mut cancel => { return Ok(..); }  // 取消支持
    results = parallel => { tool_results.extend(results); }
}
```

为什么这能工作？因为 Rust 的类型系统**在编译期就保证了安全性**：

- `ToolHost: Send + Sync` — trait bound 保证 `host.call()` 可以并发调用
- `on_event: Fn + Send + Sync` — `Fn`（不是 `FnMut`）保证事件回调可以并发触发
- 不需要 `tokio::spawn` — `join_all` 在同一个 task 上并发，避免了 `'static` 生命周期要求

没有锁，没有 Arc，没有 unsafe。**编译通过就是正确的。** 这就是 Rust 写 Agent 的核心优势。

## 上下文管理：和遗忘做朋友

LLM 的 context window 是有限的。128K tokens 听起来很多，但几轮工具调用就能吃掉一大半。

Hermes 的策略：

```rust
// 每轮对话前检查
if should_compact(&system, &session, tools_approx, model_limit, headroom) {
    // 保留最近 4 轮原文，老消息发 LLM 生成摘要
    compact_session(provider, &mut session, keep_recent_turns).await;
}
```

压缩不是简单的截断。它是一次 **LLM 驱动的摘要**：

```
COMPACTION_SYSTEM = "Summarize the following conversation preserving: 
key decisions made, tool results and their outcomes, user preferences 
stated, and any facts the user asked to remember. Be concise — aim 
for 1/5 the original length."
```

旧消息被替换为一条 `[Context Summary]` 消息，最近 N 轮保持原文。这样 Agent 不会丢失关键上下文，但也不会被历史淹没。

阈值计算考虑了所有因素：

```rust
fn should_compact(system, session, tools_json_approx, model_limit, headroom) -> bool {
    if session.messages.len() <= 8 { return false; }
    let threshold = (model_limit as f64 * (1.0 - headroom)) as usize;
    estimate_session_tokens(system, session, tools_json_approx) > threshold
}
```

system prompt + 历史消息 + 工具定义，全部算进去。headroom 默认 18%，留给 LLM 生成回复。

## 权限系统：信任但验证

```rust
pub fn is_dangerous_tool(name: &str) -> bool {
    matches!(name, "bash" | "write" | "edit" | "memory_save" | "memory_delete")
        || name.contains("__") // MCP 工具一律视为危险
}
```

5 行代码，但意味深长：

- `bash` — 可以执行任意命令
- `write` / `edit` — 可以修改任何文件
- `memory_save` / `memory_delete` — 可以修改 Agent 的"大脑"
- MCP 工具（`server__tool` 格式）— 第三方，不可信

非危险工具（`read` / `grep` / `glob` / `git` / `think`）直接执行，零延迟。危险工具必须经过用户确认：

```
⚠ confirm bash: rm -rf /tmp/old-cache  [y/a/N/...]
```

`y` 允许这次，`a` 永久允许这个工具，`N` 拒绝，或者输入文字告诉 Agent 为什么不行（反馈会作为 ToolResult 返回给 LLM）。

配置文件也可以预设规则：

```toml
[permissions]
allow = ["read", "grep", "glob"]
deny = ["memory_delete"]
```

## Agent 循环：PLAN → EXECUTE → VERIFY

单轮对话是 `run_turn()`。多步任务是 `run_agent()`——一个最多 20 次迭代的自主循环：

```rust
const AGENT_SYSTEM_SUFFIX: &str = "
PHASE 1 — PLAN (first turn):
- Use `think` to analyze the goal
- Use `todo_add` to create a step-by-step plan
- Do NOT write files in this phase

PHASE 2 — EXECUTE (subsequent turns):
- Work through todo list one step at a time
- Use `todo_update` to mark progress

PHASE 3 — VERIFY:
- Test or review before declaring completion
";
```

Agent 通过两个标记来表达完成状态：

```
[GOAL_COMPLETE] 任务完成，附上摘要
[GOAL_FAILED] 任务失败，附上原因
```

每轮之间，系统会注入 **progress check**：

```
[Agent Progress Check]
Goal: 重构认证模块
Iterations completed: 3/20
Review your todo list with `todo_list`. If all tasks are done 
and verified, respond with [GOAL_COMPLETE].
```

这防止 Agent 陷入无限循环或忘记自己在干什么。

## 为什么是 Rust？

不是为了炫技。是因为 Agent 系统有几个特性，**恰好是 Rust 最擅长的**：

**1. 并发安全**

工具并行执行需要并发访问 `ToolHost` 和事件回调。在 Python/TypeScript 里，你需要自己保证没有竞态条件。在 Rust 里，编译器帮你做：

```rust
pub trait ToolHost: Send + Sync { ... }
F: Fn(TurnEvent) + Send + Sync
```

这两行 trait bound 就够了。如果你的实现不是线程安全的，代码不会编译通过。

**2. 零成本抽象**

11 个 crate 的 trait 边界、泛型、异步——运行时开销为零。`CompositeToolHost` 包装了内置工具和 MCP 工具，但 dispatch 只是一次 `match`，没有虚表查找、没有反射、没有运行时类型检查。

```rust
async fn call(&self, name: &str, args: Value) -> Result<ToolCallOutcome> {
    if self.builtin.handles(name) {
        return self.builtin.call(name, args).await;
    }
    if name.contains("__") {
        if let Some(mcp) = &self.mcp {
            return mcp.call(name, args).await;
        }
    }
    Err(Error::ToolHost(format!("unknown tool: {name}")))
}
```

**3. 可靠性**

Agent 是长时间运行的程序。Python Agent 跑几个小时内存泄漏、TypeScript Agent 一个未处理的 Promise rejection 就崩了。Rust 的所有权系统保证了：

- 没有内存泄漏（RAII + Drop）
- 没有空指针（Option/Result）
- 没有数据竞争（Send + Sync）
- 错误必须被处理（Result 是必须解包的）

## 全景对比

| 能力 | Hermes | 典型 Python Agent | 典型 TS Agent |
|------|--------|-------------------|---------------|
| 工具并行 | join_all，编译期安全 | asyncio.gather，运行时祈祷 | Promise.all，运行时祈祷 |
| 记忆系统 | 记忆宫殿 + 超越链 + 效果追踪 | Vector DB 暴力检索 | 通常没有 |
| 自省 | 双模式反思 + 审计日志 | 通常没有 | 通常没有 |
| 技能进化 | 效果追踪 + 自动降权 | 通常没有 | 通常没有 |
| 上下文管理 | LLM 驱动的智能压缩 | 粗暴截断 | 粗暴截断 |
| 权限系统 | 三级分类 + 用户确认 | 全部允许或全部拒绝 | 全部允许 |
| 多前端 | CLI + GUI（Tauri） | 通常只有 CLI | 通常只有 Web |
| 扩展性 | MCP 协议（stdio + HTTP） | 自定义插件 | 自定义插件 |
| 崩溃恢复 | 会话 JSONL 持久化 + resume | 崩了就没了 | 崩了就没了 |
| 内存安全 | 编译期保证 | 运行时 GC | 运行时 GC |

## 它到底能做什么？

```bash
# 直接问问题
hermes ask "解释一下这个函数的作用" 

# 交互式对话（带记忆、技能、反思）
hermes chat

# 自主完成任务（PLAN → EXECUTE → VERIFY 循环）
hermes agent "给这个模块加上单元测试"
```

在 `chat` 模式下，Agent 会：

1. 根据你的输入，从记忆宫殿检索相关记忆
2. 根据触发词，激活相关技能
3. 把记忆 + 技能 + 用户画像编织进 system prompt
4. 流式生成回复，按需调用工具
5. 安全工具并行执行，危险工具逐个确认
6. 对话后台进行微反思，提取新知识
7. 自动压缩上下文，防止 token 溢出

**每一轮对话，Agent 都在变得更好。**

## 写在最后

Hermes 不是一个实验项目。它是一个回答："如果我们认真对待 Agent 的每一个维度，会得到什么？"的工程实践。

记忆不是可选的装饰，是 Agent 的核心能力。反思不是花哨的功能，是进化的引擎。权限不是事后补丁，是信任的基础。并行不是优化，是对"正确"的追求。

而 Rust，是把这一切粘合在一起的那个"正确性胶水"。

市面上有太多"能跑就行"的 Agent 实现。Hermes 想证明的是：**Agent 值得被认真地工程化**。

---

*项目地址：[small-rust-hermes](https://github.com/brzhang/small-rust-hermes)*

*纯 Rust 实现，MIT 协议，欢迎 Star 和 PR。*
