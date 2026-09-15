//! `subagent` tool: spawn a fresh child turn with isolated context.
//!
//! This is the runtime primitive the bundled `skill-creator` meta-skill relies
//! on to do real evaluations: the parent agent calls `subagent` once per test
//! case with a clean `system` + `prompt` and the child runs the task in a
//! fresh context (no parent reasoning leakage). The parent then grades the
//! returned outputs — also via `subagent` calls if it wants a grader subagent
//! with a different system prompt.
//!
//! Why fresh context: an agent that runs both the test AND the grade in the
//! same conversation has read the answer key; blind grading needs separation.
//!
//! Safety:
//! - Recursion guard: an [`AtomicUsize`] depth counter shared via Arc; tool
//!   refuses if `depth >= max_depth` (default 1 — parent spawns subagents,
//!   subagents don't spawn subagents → no fork bomb).
//! - Tool whitelist: the subagent only sees tools the caller named in
//!   `allow_tools`. The `subagent` tool itself is always excluded.
//! - Fresh tool host: built per-call from the same workspace and memory/skill
//!   stores, with NO `propose_ctx` and NO subagent context — and, when the parent
//!   session is scoped, wrapped in the **same** `PersonaToolHost` the parent wears
//!   (so a child's memory writes carry the session's owner too; Task 1.10f B-1).
//!   That structural choice — not a check — is what prevents recursion.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use hermes_core::{
    ContentBlock, LlmProvider, Message, Result, Role, ToolCallOutcome, ToolHost, ToolSpec,
};
use hermes_memory::MemoryStore;
use hermes_skills::SkillStore;
use hermes_turn::{run_turn, PermissionChecker, TurnConfig, TurnEvent};
use serde::Deserialize;

/// Wiring needed by the `subagent` tool. Construct one at startup and inject
/// into [`BuiltinToolHost::with_subagent_ctx`].
///
/// child 的记忆视野 = **父会话的视野**。三种处境，`Option<String>` 只能表达两种：
///
/// - [`MemoryView::Unscoped`]：父**没被收窄**——`hermes agent`（引擎批处理）与 IM
///   渠道今天都不套 `PersonaToolHost`。child 与父一致：看得到全库。
/// - `MemoryView::Scoped(None)`：父被收窄、但这次会话**没有人物**（`hermes chat`
///   不带 `--persona`；或会话文件里的人物 id 本版本不认识，归属落全局）。child 与
///   父一致：只看得到全局那一份。
/// - `MemoryView::Scoped(Some(id))`：父被收窄且有具体人物。child 只看得到
///   「全局 + 该人物」。
///
/// 为什么要一个显式三态：`None` 曾经同时表示前两种（相反的）处境，无人物 chat 会话
/// 里就成了「父查不到、它开的 child 查得到」的不对称泄漏（Task 1.10d）。
#[derive(Debug, Clone)]
pub enum MemoryView {
    /// 父未被收窄：child 也不收窄。
    Unscoped,
    /// 父被收窄：child 穿父会话同一件 `PersonaToolHost`（内部收窄 + 写面归属）；
    /// `None` = 只看全局。
    Scoped(Option<String>),
}

pub struct SubagentContext {
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
    pub max_tokens: u32,
    pub max_tool_rounds: usize,
    pub permissions: PermissionChecker,
    pub workspace: PathBuf,
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    /// child 该继承的**父视野**（见 [`MemoryView`]）。child 是同一个会话伸出去的
    /// 一只手：读面不该比那只手看得更多，写下的话也不该从这只手漏出去。收窄与归属
    /// 都交给父会话同一件 `PersonaToolHost`（见 [`build_child_host`]），判定仍旧只有
    /// `visible_to` / `resolve_owner` 各一处。
    pub memory_view: MemoryView,
    pub skill_store: Option<Arc<dyn SkillStore>>,
    /// Recursion depth tracker; shared via Arc so increments from parallel
    /// subagent invocations are coherent. Increment on entry, decrement on
    /// drop. Refuse to enter when depth >= max_depth.
    pub depth: Arc<AtomicUsize>,
    pub max_depth: usize,
}

impl SubagentContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        model: String,
        max_tokens: u32,
        max_tool_rounds: usize,
        permissions: PermissionChecker,
        workspace: PathBuf,
        memory_store: Option<Arc<dyn MemoryStore>>,
        memory_view: MemoryView,
        skill_store: Option<Arc<dyn SkillStore>>,
    ) -> Self {
        Self {
            provider,
            model,
            max_tokens,
            max_tool_rounds,
            permissions,
            workspace,
            memory_store,
            memory_view,
            skill_store,
            depth: Arc::new(AtomicUsize::new(0)),
            max_depth: 1,
        }
    }
}

/// 建 child 的工具面：同一份 workspace / skill store，记忆面按 `ctx.memory_view`
/// 与父会话**共用同一件外套**——`Scoped(owner)` 时套一层
/// [`crate::persona_scope::PersonaToolHost`]，`Unscoped` 保持裸 host。
///
/// 为什么是同一件外套而不是新机制：父会话（`hermes chat`）的记忆读写已经由
/// `PersonaToolHost` 一处管住（读收窄走 `ScopedMemoryStore`，写归属走
/// `resolve_owner`）。child 缺的从来不是第二套归属，而是**这一件**——裸
/// `BuiltinToolHost` 的记忆分发把 `session_owner` 写死 `None`，于是人物会话派出的
/// child 一存记忆就落全局（父私有、子广播，Task 1.10f 的 B-1）。
///
/// 为什么不给 `BuiltinToolHost` 再加一个归属字段：那会和 `PersonaToolHost` 并行成
/// 两处必须同时改对的口（P0 第九条：判定点与收窄点都不许散）。为什么不把
/// `subagent` 特判进 `PersonaToolHost`：child 是**另一个** host 实例，父的包装套不
/// 到它身上。
fn build_child_host(ctx: &SubagentContext) -> Arc<dyn ToolHost> {
    let mut child = crate::BuiltinToolHost::new(ctx.workspace.clone());
    if let Some(m) = &ctx.memory_store {
        // 内层仍是**未收窄**的同一份 store：收窄与归属由外层 `PersonaToolHost`
        // 一处做（与 `chat/mod.rs` 给父包的那一层同构）。在这里再塞一层
        // `ScopedMemoryStore` 会让「收窄」变成两处，两处必须同时改对。
        child = child.with_memory_store(m.clone());
    }
    if let Some(s) = &ctx.skill_store {
        child = child.with_skill_store(s.clone());
    }
    let child: Arc<dyn ToolHost> = Arc::new(child);
    match &ctx.memory_view {
        MemoryView::Scoped(owner) => Arc::new(crate::persona_scope::PersonaToolHost::new(
            child,
            ctx.memory_store.clone(),
            owner.clone(),
        )),
        // 父没被收窄（`hermes agent` 引擎批处理 / IM 渠道）→ child 也不收窄，与今天
        // 行为一致。
        MemoryView::Unscoped => child,
    }
}

#[derive(Deserialize)]
struct SubagentArgs {
    /// System prompt installed at the top of the subagent's context. This is
    /// the role/contract the subagent operates under (e.g. "You are a grader.
    /// Read the transcript at <path> and the output at <path>, then write
    /// grading.json matching the schema in references/schemas.md.")
    system: String,
    /// Initial user message. The subagent treats this as its single user
    /// turn and runs tool loops until it produces a final text response.
    prompt: String,
    /// Whitelist of tool names the subagent may use. Default: empty (text-only
    /// reasoning, no tool access). Common picks:
    /// - executor subagent: `["read", "write", "edit", "bash", "glob", "grep",
    ///   "skill_read", "skill_read_file"]`
    /// - grader subagent: `["read", "glob", "write"]` (read transcript +
    ///   outputs, write grading.json)
    /// - comparator subagent: `["read", "glob", "write"]` (read both outputs,
    ///   write comparison.json)
    ///
    /// The `subagent` tool itself is always excluded from the list (no nesting).
    #[serde(default)]
    allow_tools: Vec<String>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "subagent".into(),
        description: "Spawn a child agent in a fresh context to run a sub-task. \
            Use for: (a) executing each test-case prompt during skill evaluation \
            (clean context per run, no leakage from parent reasoning), \
            (b) grading transcripts with a dedicated grader prompt, \
            (c) blind A/B comparison between two outputs, \
            (d) description-optimization loop. \
            The child has its own context: it sees only the `system` and `prompt` \
            you pass — none of your conversation history. Returns the child's \
            final text reply plus a summary of tool calls it made. \
            Multiple `subagent` calls in the same response run in parallel (the model \
            batches them). Subagents cannot themselves call `subagent` (depth=1 hard cap)."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "system": {
                    "type": "string",
                    "description": "System prompt for the child. Sets its role and contract. For evaluator runs, include the skill body (read via skill_read first) or the path to the skill workspace."
                },
                "prompt": {
                    "type": "string",
                    "description": "The single user message the child receives. Be self-contained — no parent context leaks through."
                },
                "allow_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tool names the child may call. Default empty (text-only). Pick the minimum set: e.g. ['read','write','bash','glob','grep'] for an executor; ['read','glob','write'] for a grader.",
                    "default": []
                }
            },
            "required": ["system", "prompt"]
        }),
        requires_confirmation: true,
    }
}

/// Guard that decrements the depth counter on drop — keeps the counter
/// consistent even if `run_turn` panics or the host fails mid-call.
struct DepthGuard {
    depth: Arc<AtomicUsize>,
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        self.depth.fetch_sub(1, Ordering::SeqCst);
    }
}

pub async fn run(ctx: &SubagentContext, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: SubagentArgs = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("subagent: bad args: {e}")))?;

    // Recursion guard — atomic CAS-style increment.
    let prev = ctx.depth.fetch_add(1, Ordering::SeqCst);
    if prev >= ctx.max_depth {
        // We over-incremented; roll back and refuse.
        ctx.depth.fetch_sub(1, Ordering::SeqCst);
        return Ok(ToolCallOutcome {
            content: format!(
                "subagent: refused — recursion depth {prev} already at max {} (subagents cannot themselves spawn subagents).",
                ctx.max_depth
            ),
            is_error: true,
        });
    }
    let _guard = DepthGuard {
        depth: ctx.depth.clone(),
    };

    // Build a fresh tool host for the child (bare `BuiltinToolHost`, or it wrapped
    // in the session's `PersonaToolHost` — see `build_child_host`). No
    // propose_ctx and no subagent_ctx → child literally cannot call
    // `propose_skill` or `subagent`, regardless of what allow_tools says.
    let child = build_child_host(ctx);
    let all_specs = child.list_tools().await?;
    let allowed: std::collections::HashSet<&str> =
        a.allow_tools.iter().map(|s| s.as_str()).collect();
    // Always exclude `subagent` from the child's tool list (depth guard would
    // catch a sneaky call anyway, but filter at the API surface so the model
    // doesn't see it advertised).
    let filtered: Vec<ToolSpec> = all_specs
        .into_iter()
        .filter(|t| t.name != "subagent" && allowed.contains(t.name.as_str()))
        .collect();

    // Report which tools the child can use — gives the parent a way to debug
    // when allow_tools contained a typo that silently dropped a tool.
    let advertised: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();

    let history = vec![Message::user_text(a.prompt.clone())];

    let turn_cfg = TurnConfig {
        model: ctx.model.clone(),
        system: Some(a.system),
        max_tokens: ctx.max_tokens,
        max_tool_rounds: ctx.max_tool_rounds,
        permissions: ctx.permissions.clone(),
    };

    // Collect tool-call summaries as events stream by — parent sees a digest
    // of what the child did instead of having to re-spawn the same conversation
    // to find out.
    let tool_calls: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let on_event = |ev: TurnEvent| {
        if let TurnEvent::ToolExecStart { summary, .. } = ev {
            if let Ok(mut v) = tool_calls.lock() {
                v.push(summary);
            }
        }
    };

    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let started = Instant::now();
    let output = run_turn(
        ctx.provider.as_ref(),
        child.as_ref(),
        &filtered,
        &history,
        &turn_cfg,
        None, // confirm_tx: None → fail-closed (no approval UI)
        on_event,
        cancel_rx,
    )
    .await?;
    let duration_ms = started.elapsed().as_millis() as u64;

    // Extract the child's final assistant text.
    let final_text: String = output
        .new_messages
        .iter()
        .filter(|m| matches!(m.role, Role::Assistant))
        .flat_map(|m| {
            m.content.iter().filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    let calls = tool_calls.into_inner().unwrap_or_default();
    let input_tokens = output.usage.input_tokens;
    let output_tokens = output.usage.output_tokens;

    let mut content = String::new();
    content.push_str("--- subagent reply ---\n");
    if final_text.is_empty() {
        content.push_str("(no text reply)\n");
    } else {
        content.push_str(&final_text);
        content.push('\n');
    }
    content.push_str("--- subagent telemetry ---\n");
    content.push_str(&format!("tools_advertised: [{}]\n", advertised.join(", ")));
    content.push_str(&format!("tool_calls: {}\n", calls.len()));
    for c in &calls {
        content.push_str(&format!("  - {c}\n"));
    }
    content.push_str(&format!(
        "duration_ms: {duration_ms}\ninput_tokens: {input_tokens}\noutput_tokens: {output_tokens}\n"
    ));

    Ok(ToolCallOutcome {
        content,
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{
        Confidence, FsMemoryStore, LoadedMemory, MemoryFrontmatter, Scope, Source,
    };

    /// child host 的可测路径**不碰** provider（`build_child_host` 只用 workspace 与
    /// 两个 store）。真被调到就是测试写错了，直接报错比静默回一句正文好。
    struct NeverCalledProvider;

    #[async_trait::async_trait]
    impl LlmProvider for NeverCalledProvider {
        async fn complete(
            &self,
            _req: hermes_core::CompletionRequest,
        ) -> hermes_core::Result<hermes_core::CompletionResponse> {
            Err(hermes_core::Error::Provider(
                "test provider: complete() must never be called".into(),
            ))
        }

        fn capabilities(&self) -> hermes_core::Capabilities {
            hermes_core::Capabilities {
                tool_use: true,
                prompt_caching: false,
                streaming: false,
            }
        }

        fn name(&self) -> &str {
            "never-called"
        }
    }

    fn ctx_with(
        view: MemoryView,
        store: Arc<dyn MemoryStore>,
        dir: &std::path::Path,
    ) -> SubagentContext {
        SubagentContext::new(
            Arc::new(NeverCalledProvider),
            "test-model".into(),
            1024,
            4,
            PermissionChecker::new(&[], &[]),
            dir.to_path_buf(),
            Some(store),
            view,
            None,
        )
    }

    fn seed(store: &dyn MemoryStore, owner: Option<&str>, body: &str) {
        let fm = MemoryFrontmatter::new(Source::User, Confidence::High, vec![], "general".into())
            .owned(owner.map(str::to_string));
        store.put(Scope::User, fm, body).unwrap();
    }

    /// 同一份 store：全局 1 条、wang-hai-yan 1 条、xiao-xie 1 条。
    fn seeded_store(dir: &std::path::Path) -> Arc<dyn MemoryStore> {
        let store: Arc<dyn MemoryStore> = Arc::new(FsMemoryStore::new(dir.to_path_buf(), None));
        seed(store.as_ref(), None, "交付一律给 Word 放桌面");
        seed(
            store.as_ref(),
            Some("wang-hai-yan"),
            "标题不夸张，别用夸张形容词",
        );
        seed(
            store.as_ref(),
            Some("xiao-xie"),
            "林碳报告只引 IEA 的公开口径",
        );
        store
    }

    fn find(store: &dyn MemoryStore, needle: &str) -> Option<LoadedMemory> {
        store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|m| m.body.contains(needle))
    }

    /// 归属钉（Task 1.10f 的 B-1）：child 不只是一只读手，它写下的话也得归这个会话
    /// 的人物。裸 `BuiltinToolHost` 把 `session_owner` 写死 `None`，一存就落全局
    /// （父私有、子广播）；修法是让 child 穿父会话同一件 `PersonaToolHost`。
    #[tokio::test]
    async fn a_scoped_childs_write_is_attributed_to_its_persona() {
        let _root = crate::test_env::temp_data_root();
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store(dir.path());
        let child = build_child_host(&ctx_with(
            MemoryView::Scoped(Some("xiao-xie".into())),
            store.clone(),
            dir.path(),
        ));

        let saved = child
            .call(
                "memory_save",
                serde_json::json!({"content": "林碳配额口径一律用 2026 版"}),
            )
            .await
            .unwrap();
        assert!(!saved.is_error, "{}", saved.content);

        let landed = find(store.as_ref(), "林碳配额口径一律用").expect("child 存下的那条必须落盘");
        assert_eq!(
            landed.frontmatter.owner.as_deref(),
            Some("xiao-xie"),
            "child 存下的记忆必须归这个会话的人物，不能落全局"
        );
    }

    /// 同一条归属轴的读面半边：child 写下的那条，**另一个工位的 child 读不到**。
    /// 少了这半，「写对了但谁都能看见」也能让上面那条绿。
    #[tokio::test]
    async fn a_scoped_childs_write_stays_private_to_its_persona() {
        let _root = crate::test_env::temp_data_root();
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store(dir.path());
        let xie = build_child_host(&ctx_with(
            MemoryView::Scoped(Some("xiao-xie".into())),
            store.clone(),
            dir.path(),
        ));
        let saved = xie
            .call(
                "memory_save",
                serde_json::json!({"content": "林碳配额口径一律用 2026 版"}),
            )
            .await
            .unwrap();
        assert!(!saved.is_error, "{}", saved.content);

        let wang = build_child_host(&ctx_with(
            MemoryView::Scoped(Some("wang-hai-yan".into())),
            store.clone(),
            dir.path(),
        ));
        // 断言正文短语而不是 query：查空时工具会把 query 原样回显进
        // "no memories matching: …"，拿 query 当判据会让这条永远绿。
        let others = wang
            .call(
                "memory_search",
                serde_json::json!({"query": "配额", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(
            !others.content.contains("林碳配额口径一律用"),
            "别的工位的 child 读到了这条: {}",
            others.content
        );

        // 正面控制：写它的人自己看得见（少了这半，「谁都读不到」也能让上面绿）。
        let mine = xie
            .call(
                "memory_search",
                serde_json::json!({"query": "配额", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(
            mine.content.contains("林碳配额口径一律用"),
            "{}",
            mine.content
        );
    }

    /// 泄漏钉（Task 1.10c）：child 是父会话伸出去的一只手，读面必须与父会话一致。
    #[tokio::test]
    async fn a_scoped_child_reads_only_its_own_persona() {
        // 命中 palace 读会往数据根写 `memory-stats.jsonl`（`palace.rs`）——整条
        // 用例套临时数据根，红了也不碰用户的盘（见 `crate::test_env`）。
        let _root = crate::test_env::temp_data_root();
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store(dir.path());
        let child = build_child_host(&ctx_with(
            MemoryView::Scoped(Some("xiao-xie".into())),
            store,
            dir.path(),
        ));

        let others = child
            .call(
                "memory_search",
                serde_json::json!({"query": "标题", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(
            !others.content.contains("标题不夸张"),
            "child 读到了别的人物（wang-hai-yan）的记忆: {}",
            others.content
        );
        let palace = child
            .call("palace_recall", serde_json::json!({"topic": "标题"}))
            .await
            .unwrap();
        assert!(
            !palace.content.contains("标题不夸张"),
            "palace 也是同一个读面，不能漏: {}",
            palace.content
        );

        // 正面控制：自己的 + 全局的仍在（少了这一半，「谁都读不到」也能让上面绿）。
        let mine = child
            .call(
                "memory_search",
                serde_json::json!({"query": "IEA", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(
            mine.content.contains("林碳报告只引 IEA"),
            "本人物自己的记忆必须还在: {}",
            mine.content
        );
        let global = child
            .call(
                "memory_search",
                serde_json::json!({"query": "交付", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(global.content.contains("Word"), "{}", global.content);
    }

    /// 父被收窄、但会话**没有人物**（`hermes chat` 不带 `--persona`）：child 与父一致
    /// ——只看得到全局那一份。这条挡的正是「父查不到、它开的 child 查得到」的不对称
    /// 泄漏（Task 1.10d）。
    #[tokio::test]
    async fn a_scoped_child_without_a_persona_sees_globals_only() {
        let _root = crate::test_env::temp_data_root();
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store(dir.path());
        let child = build_child_host(&ctx_with(MemoryView::Scoped(None), store, dir.path()));

        // 两条人物私有记忆都读不到：检索与 palace 两条路各试一次。
        for (name, args) in [
            (
                "memory_search",
                serde_json::json!({"query": "标题 夸张", "limit": 5}),
            ),
            (
                "memory_search",
                serde_json::json!({"query": "林碳 IEA 口径", "limit": 5}),
            ),
            ("palace_recall", serde_json::json!({"topic": "标题 夸张"})),
            (
                "palace_recall",
                serde_json::json!({"topic": "林碳 IEA 口径"}),
            ),
        ] {
            let out = child.call(name, args).await.unwrap();
            for body in ["标题不夸张", "林碳报告只引 IEA"] {
                assert!(
                    !out.content.contains(body),
                    "{name} 读到了人物私有记忆 {body:?}: {}",
                    out.content
                );
            }
        }

        // 正面控制：全局那条仍在（少了这一半，「谁都读不到」也能让上面绿）。
        let global = child
            .call(
                "memory_search",
                serde_json::json!({"query": "交付 Word 桌面", "limit": 5}),
            )
            .await
            .unwrap();
        assert!(global.content.contains("Word"), "{}", global.content);
        let zone = child
            .call("palace_read_zone", serde_json::json!({"zone": "general"}))
            .await
            .unwrap();
        assert!(
            zone.content.contains("交付一律给 Word 放桌面"),
            "{}",
            zone.content
        );
        assert!(!zone.content.contains("标题不夸张"), "{}", zone.content);
    }

    /// 回归钉：父**没被收窄**（`hermes agent` / IM 渠道今天没接人物）→ child 也不
    /// 收窄，仍看得到全部。别为了堵上面那条把这条路径也收窄了。
    #[tokio::test]
    async fn an_unscoped_child_still_sees_everything() {
        let _root = crate::test_env::temp_data_root();
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store(dir.path());
        let child = build_child_host(&ctx_with(MemoryView::Unscoped, store, dir.path()));

        // 检索是**排序**取 top-k，用一个词的 query 只保证命中那一条；所以每条
        // 用「能找到它」的 query，别拿相关性当「看得见」。
        for (name, args) in [
            (
                "memory_search",
                serde_json::json!({"query": "标题 夸张", "limit": 5}),
            ),
            ("palace_recall", serde_json::json!({"topic": "标题 夸张"})),
        ] {
            let out = child.call(name, args).await.unwrap();
            assert!(
                out.content.contains("标题不夸张"),
                "无人物 child 必须仍看得见别的人物的记忆（{name}）: {}",
                out.content
            );
        }
        // `palace_read_zone` 不排序，整区列出来——三条一条都不能少。
        let zone = child
            .call("palace_read_zone", serde_json::json!({"zone": "general"}))
            .await
            .unwrap();
        for body in [
            "交付一律给 Word 放桌面",
            "标题不夸张，别用夸张形容词",
            "林碳报告只引 IEA 的公开口径",
        ] {
            assert!(
                zone.content.contains(body),
                "无人物 child 的 zone 里少了 {body:?}: {}",
                zone.content
            );
        }
    }
}
