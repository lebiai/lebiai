use hermes_core::{Message, Role, Session, SessionEvent, SessionMeta};

use hermes_memory::MemoryStore;
use hermes_store::SessionWriter;
use hermes_turn::{TurnConfig, TurnEvent};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use crate::commands::micro;
use crate::context::ContextSources;
use crate::error::GuiError;
use crate::events::ChatStreamEvent;
use crate::state::{session_path_for, ActiveSession, AppState};

/// Last non-empty human user text (skips tool-result-only user rows).
fn last_human_user_text(messages: &[Message]) -> Option<String> {
    for m in messages.iter().rev() {
        if m.role != Role::User || m.is_tool_result_only() || m.is_internal_instruction_only() {
            continue;
        }
        let text: String = m
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}

/// 这一轮**最后一段** assistant 正文（界面那块实时文本就是这一段）。
///
/// 给「纠正文」用：界面上的实时文本在每次工具调用开始时清空重来，所以纠正的粒度
/// 必须是最后一段。取全部 assistant 文本会把过程旁白灌进回答气泡，变成一堵之后
/// 才「塌掉」的墙（2026-09-20 用户原话：整段蹦出来、看不出流式）。
fn last_assistant_text(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| {
            m.content
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn strip_attachments_header(text: &str) -> String {
    match text.find("[attachments]") {
        Some(i) => text[..i].trim().to_string(),
        None => text.trim().to_string(),
    }
}

/// Index after the last human user message (exclusive end for truncate-to-user-keep).
fn end_after_last_human_user(messages: &[Message]) -> Option<usize> {
    for (i, m) in messages.iter().enumerate().rev() {
        if m.role == Role::User && !m.is_tool_result_only() && !m.is_internal_instruction_only() {
            let text: String = m
                .content
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return Some(i + 1);
            }
        }
    }
    None
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    begin_turn(app, state, session_id, Some(content), on_event).await
}

/// Re-run the agent on current history **without** pushing a new user message.
/// Caller must truncate trailing assistant/tool turns first (see `truncate_session`).
#[tauri::command]
pub async fn regenerate_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    begin_turn(app, state, session_id, None, on_event).await
}

async fn begin_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mut new_user: Option<String>,
    on_event: Channel<ChatStreamEvent>,
) -> Result<(), GuiError> {
    // License / trial gate (docs/spec/license-ux.md).
    if !hermes_core::can_use_main_readonly() {
        return Err(GuiError::Config("license_locked".into()));
    }

    // No API key configured → refuse before any request leaves the machine.
    // Frontend already gates on hasApiKey; this is defense for direct invoke.
    let active_key = state
        .config
        .read()
        .unwrap()
        .active_provider()
        .map_err(|e| GuiError::Config(e.to_string()))?
        .api_key
        .clone();
    if active_key.trim().is_empty() {
        return Err(GuiError::Config(
            "no API key configured — add one in Settings first".into(),
        ));
    }

    let provider = state.provider.read().unwrap().clone();
    let host = state.host.clone();
    let model = state.model();
    let max_tokens = state.max_tokens();
    let skills = state.refresh_skills().await;
    // Refresh memory cache from disk so mid-session memory_save is visible next turn.
    let active = {
        let all = match state.memory_store.list_active() {
            Ok(v) => v,
            Err(_) => state.active_memories.lock().await.clone(),
        };
        let pinned: Vec<_> = all
            .iter()
            .filter(|m| m.frontmatter.pinned)
            .cloned()
            .collect();
        *state.active_memories.lock().await = all.clone();
        *state.pinned_memories.lock().await = pinned;
        all
    };
    let workspace_root = state.workspace_root();
    let (limits, default_provider, compaction_policy) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.limits,
            cfg.default_provider.clone(),
            hermes_core::compaction::CompactionPolicy {
                model_limit: cfg.context.model_limit,
                headroom: cfg.context.headroom,
                keep_recent_turns: cfg.context.keep_recent_turns,
            },
        )
    };

    let allow_rules: Vec<String> = state.config.read().unwrap().permissions.allow.clone();
    let deny_rules: Vec<String> = state.config.read().unwrap().permissions.deny.clone();
    let permissions = hermes_turn::PermissionChecker::new(&allow_rules, &deny_rules);

    let mut sessions = state.sessions.lock().await;
    if let Some(s) = sessions.get(&session_id) {
        if hermes_store::channel_of_session_path(&s.path).is_some() {
            return Err(GuiError::Session(
                "channel records are view-only in the desktop app".into(),
            ));
        }
    }
    let active_session = if let Some(s) = sessions.get_mut(&session_id) {
        s
    } else {
        if new_user.is_none() {
            return Err(GuiError::Session("no active session for regenerate".into()));
        }
        let meta = SessionMeta::new(model.clone(), default_provider);
        let path = session_path_for(&meta).map_err(|e| GuiError::Session(e.to_string()))?;
        sessions.insert(
            session_id.clone(),
            ActiveSession {
                session: Session {
                    meta,
                    messages: Vec::new(),
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    flow: Default::default(),
                },
                writer: None,
                path,
            },
        );
        sessions.get_mut(&session_id).unwrap()
    };

    let prompt_for_system = if let Some(ref content) = new_user {
        content.clone()
    } else {
        last_human_user_text(&active_session.session.messages).ok_or_else(|| {
            GuiError::Session("cannot regenerate: no user message in history".into())
        })?
    };

    let material_query = strip_attachments_header(&prompt_for_system);
    if hermes_sources::is_topic_reset(&material_query)
        || hermes_sources::wants_other_file(&material_query)
    {
        state.source_focus.lock().await.remove(&session_id);
    }
    let stored_focus = state
        .source_focus
        .lock()
        .await
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let focus: &[String] = if hermes_sources::is_topic_reset(&material_query) {
        &[]
    } else {
        &stored_focus
    };
    let material_hits: Vec<hermes_core::MaterialHit> = state
        .source_store
        .search(&material_query, focus)
        .into_iter()
        .map(|h| hermes_core::MaterialHit {
            id: h.id,
            title: h.title,
            excerpt: h.excerpt,
        })
        .collect();
    if hermes_sources::is_topic_reset(&material_query) {
        // stay cleared
    } else if !material_hits.is_empty() {
        let ids: Vec<String> = material_hits.iter().map(|h| h.id.clone()).collect();
        state
            .source_focus
            .lock()
            .await
            .insert(session_id.clone(), ids);
    } else if !hermes_sources::is_followup_query(&material_query) {
        state.source_focus.lock().await.remove(&session_id);
    }

    if hermes_sources::wants_on_hand(&material_query) {
        state.source_store.set_read_allowlist(None);
    } else if material_hits.is_empty() {
        state.source_store.set_read_allowlist(Some(Vec::new()));
    } else {
        let mut allow: Vec<String> = material_hits
            .iter()
            .flat_map(|h| [h.id.clone(), h.title.clone()])
            .collect();
        allow.sort();
        allow.dedup();
        state.source_store.set_read_allowlist(Some(allow));
    }
    if hermes_sources::wants_on_hand(&material_query) {
        let rows = state.source_store.list_active();
        if let Some(ref mut content) = new_user {
            if !content.contains("[on-hand]") {
                content.push_str("\n\n[on-hand]\n");
                if rows.is_empty() {
                    content.push_str("(none)\n");
                } else {
                    for r in &rows {
                        content.push_str(&format!("- 《{}》\n", r.title));
                    }
                }
            }
        }
    }

    if hermes_sources::wants_remember_standard(&material_query) {
        let title = material_hits
            .first()
            .map(|h| h.title.clone())
            .unwrap_or_else(|| "这份材料".into());
        let fact = format!(
            "做同类事时，好的标准是：{}。出处：《{}》。",
            material_query.chars().take(240).collect::<String>().trim(),
            title
        );
        let output = hermes_reflect::ReflectionOutput {
            summary: "from kept material".into(),
            memory_candidates: vec![hermes_reflect::MemoryCandidate {
                fact,
                tags: vec!["standard".into(), "from-material".into()],
                zone: "standards".into(),
                scope: hermes_memory::Scope::User,
                confidence: hermes_memory::Confidence::Medium,
                rationale: "user asked to remember a standard".into(),
                supersedes: vec![],
                owner: None,
            }],
            ..Default::default()
        };
        // 用户当场点「记住」是一个片段、不是整场 distill 的结果，所以只记工位、
        // 不带 `session_id`（`append_only`）——否则它会被同会话下一批 micro 候选
        // 或 session-end 那份挤掉。
        let mark =
            hermes_reflect::EnqueueMark::append_only(hermes_core::persona::memory_owner_for(
                active_session.session.meta.persona.as_deref(),
                active_session.session.meta.team.as_deref(),
            ));
        if let Ok(n) = hermes_reflect::enqueue_from_reflection_marked(
            &output,
            hermes_reflect::InboxSource::Micro,
            mark,
        ) {
            if n > 0 {
                let _ = on_event.send(crate::events::ChatStreamEvent::RememberQueued);
            }
        }
    }

    let open_work = state.commitment_store.list_live().unwrap_or_default();
    let first_human_today = {
        let (seq, at) = active_session.session.last_human_send();
        let today = chrono::Utc::now().date_naive();
        seq == 0 || at.map(|t| t.date_naive() < today).unwrap_or(true)
    };
    // 指路名册 = 侧栏上真正有的那几个工位（授权 ∩ 勾选 + 自带）。
    let roster = hermes_core::persona::open();
    if active_session.session.meta.team.is_some() {
        let _ = crate::commands::episode::sync_baton(
            active_session,
            std::path::Path::new(&workspace_root),
            Some(prompt_for_system.as_str()),
        );
    }
    // 这一轮谁在说：工位是他本人；组里是**接棒的人**（没交过棒 = 第一棒采）。
    // 判据只此一处（`speaker_for`），界面上的名字与提示词里的人物块读的是同一个答案。
    let speaker = hermes_core::persona::speaker_for(
        active_session.session.meta.persona.as_deref(),
        active_session.session.meta.team.as_deref(),
        active_session.session.flow.holder(),
    );
    // 这个会话看得见的那几个归属（§4.2）：工位 / 无人物是单值；**组是两个**——
    // 本项目组 + 这一轮说话的人（他的手艺跟着上桌）。会话 → 归属的转换只此一处。
    let owners = hermes_core::persona::memory_owners_for(
        active_session.session.meta.persona.as_deref(),
        active_session.session.meta.team.as_deref(),
        speaker.map(|p| p.id.as_str()),
    );
    let owner_refs: Vec<&str> = owners.iter().map(String::as_str).collect();
    // 注入面与卡面**同轴**：只看「全局 + 这几个归属」，别人的一条都不给
    // （`docs/spec/personas.md` §5.2）。装配只走 `views_for_any` 一处——CLI / server
    // 用的是同一个函数，谁再自己写一份判据，就会有一天只改一处、某个人物开始看见
    // 别人的记忆。注意：上面写进 `state.active_memories` 缓存的是**全量**，不受影响。
    let (active, pinned) = visible_memories(&active, &owner_refs);
    let topic_cards = hermes_memory::topics::render_from_disk_any(&active, &owner_refs);
    // 编译好的 `profile.md`：CLI 与 IM 一直在注入，桌面端以前**从不**注入（P1-2）——
    // 同一条记忆，命令行里记得、GUI 里不记得。每轮现读：档案是反思之后重编的，
    // 启动时那一份到晚上就旧了。
    let compiled_profile = hermes_memory::load_profile().unwrap_or(None);
    // 记忆工具面（`memory_search` / `memory_save` / palace 三件套）也走同一个视图：
    // 读只给「全局 + 这几个归属」，写入按 `resolve_owner` 落归属。**没有这一层，
    // 桌面端教什么都落全局**（归属判定等于没接上）。
    let host: Arc<dyn hermes_core::ToolHost> =
        Arc::new(hermes_tools::persona_scope::PersonaToolHost::with_owners(
            host,
            Some(state.memory_store.clone() as Arc<dyn hermes_memory::MemoryStore>),
            owners.clone(),
        ));
    // 翻旧账：让模型能翻回**本会话**更早的对话（含被压缩换掉的原文）。路径由引擎绑，
    // 模型给不了——它翻不到别人的会话。慢也是看得见的慢（工具卡上写着「翻旧账」）。
    let host: Arc<dyn hermes_core::ToolHost> = Arc::new(
        hermes_tools::session_recall::SessionRecallHost::new(host, active_session.path.clone()),
    );
    // 组会话：这张桌子上今天还开着谁（同一份「开着」的判据），以及这一轮谁接。
    // 表外的会话（工位 / 无人物）`team` 为 `None`，提示词与没有这一层时逐字节相同。
    // 「谁定的 / 哪一期 / 哪个组」由**引擎**盖到 `decision` 上，模型说了不算。
    // 自由对话（没有人物）不盖：那份决定会缺掉四样里的一样，工具会明说记不了。
    let host: Arc<dyn hermes_core::ToolHost> = match speaker {
        Some(p) => Arc::new(hermes_tools::StampedHost::new(
            host,
            hermes_tools::Stamp {
                by: p.id.to_string(),
                team: active_session.session.meta.team.clone(),
                episode: chrono::Local::now().date_naive().to_string(),
            },
        )),
        None => host,
    };
    // 子代理：child 是同一个会话伸出去的一只手。**在这一层挂而不是启动时挂**——
    // child 的记忆视野要跟着本轮人物走（`Scoped(owner)`），而工具宿主是启动时建的、
    // 所有会话共用。网页能力**现算**：它握着 provider（含 API Key），启动时那一份在
    // 用户改 Key 之后就旧了。
    let subagent_ctx = {
        let cfg = state.config.read().unwrap();
        Arc::new(
            hermes_tools::SubagentContext::new(
                provider.clone(),
                model.clone(),
                max_tokens,
                cfg.limits.max_tool_rounds,
                permissions.clone(),
                std::path::PathBuf::from(&workspace_root),
                Some(state.memory_store.clone() as Arc<dyn hermes_memory::MemoryStore>),
                hermes_tools::subagent::MemoryView::Scoped(owners.first().cloned()),
                Some(state.skill_store.clone() as Arc<dyn hermes_skills::SkillStore>),
            )
            .with_web_ctx(crate::state::build_web_ctx(&cfg, provider.clone())),
        )
    };
    let host: Arc<dyn hermes_core::ToolHost> =
        Arc::new(hermes_tools::SubagentHost::new(host, subagent_ctx));
    // 工具面 = **这一轮真正用的宿主**说了算。以前这里读的是启动时那份缓存
    // （`state.tools`），于是 per-turn 壳挂上去的工具（翻旧账 / 子代理）
    // 模型根本看不见——宿主支持、surface 没有，两边各说各话。
    let tools = host.list_tools().await.unwrap_or_default();
    let team_present: Vec<&str> = active_session
        .session
        .meta
        .team
        .as_deref()
        .and_then(hermes_core::team::get)
        .map(|t| {
            t.members
                .iter()
                .filter(|m| roster.iter().any(|p| p.id == m.id))
                .map(|m| m.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    let team_ctx = active_session
        .session
        .meta
        .team
        .as_deref()
        .and_then(hermes_core::team::get)
        .map(|t| crate::context::TeamContext {
            team: t,
            present: &team_present,
            // 组块里写「这一轮由谁接」：接棒的人，没交过棒就是第一棒。
            speaker: speaker
                .map(|p| p.id.as_str())
                .unwrap_or_else(|| hermes_core::start_id(t)),
        });
    let sources = turn_sources(
        speaker,
        &roster,
        team_ctx,
        topic_cards.as_deref(),
        compiled_profile.as_deref(),
        &pinned,
        &active,
        &skills,
        &open_work,
        &material_hits,
        first_human_today,
        &workspace_root,
        limits,
    );
    let turn_system = sources.build_turn_system(&prompt_for_system);

    if let Some(content) = new_user {
        let user_msg = active_session.session.push_user(&content).clone();
        active_session
            .ensure_writer()
            .map_err(|e| GuiError::Session(e.to_string()))?
            .append(&SessionEvent::Message(user_msg))
            .map_err(|e| GuiError::Session(e.to_string()))?;

        {
            use hermes_core::{
                derive_title_from_messages, is_trivial_user_text, DEFAULT_SESSION_TITLE,
            };
            let derived = derive_title_from_messages(&active_session.session.messages);
            let current = active_session
                .session
                .meta
                .title
                .as_deref()
                .unwrap_or(DEFAULT_SESSION_TITLE);
            let should_write = !is_trivial_user_text(&content)
                && (current == DEFAULT_SESSION_TITLE
                    || active_session.session.meta.title.is_none()
                    || hermes_core::is_trivial_user_text(current));
            if should_write && derived != DEFAULT_SESSION_TITLE {
                active_session.session.meta.title = Some(derived.clone());
                let path = active_session.path.clone();
                let _ = hermes_store::update_session_title(&path, &derived);
                if let Ok(w) = SessionWriter::open_append(&path) {
                    active_session.writer = Some(w);
                }
            }
        }
    }

    // Repair incomplete tool_use / tool_result pairs so OpenAI-compatible
    // providers (DeepSeek etc.) do not 400 on resume.
    let history = hermes_core::sanitize_history_for_provider(&active_session.session.messages);
    // Keep in-memory session aligned so subsequent turns stay clean.
    active_session.session.messages = history.clone();
    let session_meta = active_session.session.meta.clone();
    drop(sessions);

    if let Ok(mut guard) = state.propose_messages.write() {
        *guard = history.clone();
    }

    let allowed_titles: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(
        material_hits.iter().map(|h| h.title.clone()).collect(),
    ));

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state
        .cancel_tokens
        .lock()
        .await
        .insert(session_id.clone(), cancel_tx);

    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel::<hermes_turn::ConfirmRequest>(8);

    let sid = session_id.clone();
    let sessions_arc = state.sessions.clone();
    let cancel_tokens_arc = state.cancel_tokens.clone();
    let propose_queue = state.propose_queue.clone();
    let provider_for_micro = state.provider.read().unwrap().clone();
    let memory_store = state.memory_store.clone();
    let skills_for_micro = skills.clone();
    let memories_for_micro = active.clone();
    let active_memories_arc = state.active_memories.clone();
    let pinned_memories_arc = state.pinned_memories.clone();
    let micro_cooldown = state.micro_turns_since.clone();
    let (auto_accept, min_confidence) = micro::reflect_policy(&state);
    let persist_thinking = state.config.read().unwrap().ui.persist_thinking;
    let ui_lang = state
        .config
        .read()
        .map(|c| c.ui.language.clone())
        .unwrap_or_else(|_| "zh-CN".into());
    let live_llm = state.live_llm.clone();
    let workspace_for_baton = workspace_root.clone();

    tokio::spawn(async move {
        let _live = live_llm.enter();

        // 开口不挡在摘要上：这一轮请求体由 `fold_for_request` 封顶（更早的条整段省略）。
        // 持久压缩挪到回答之后，只服务下一轮，失败也不回滚这一轮。
        let history = history;

        let config = TurnConfig {
            model,
            system: if turn_system.is_empty() {
                None
            } else {
                Some(turn_system.clone())
            },
            max_tokens,
            max_tool_rounds: limits.max_tool_rounds,
            permissions,
        };

        let evt = on_event.clone();
        // `Done` 是界面的「可以抬手了」。它必须**晚于**所有内容事件：run_turn 一结束就
        // 发的话，随后 sanitize 出来的权威全文（TextCorrected）会落到已经收摊的界面上，
        // 等于白纠。所以这里先扣住，等正文落地再放。
        let done_held = Arc::new(AtomicBool::new(false));
        let done_held_ev = done_held.clone();
        let ui_lang_ev = ui_lang.clone();
        let app_ev = app.clone();
        let tool_names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let tool_names_ev = tool_names.clone();
        let allowed_ev = allowed_titles.clone();
        let on_turn_event = move |event: TurnEvent| match event {
            TurnEvent::TextDelta(text) => {
                let _ = evt.send(ChatStreamEvent::TextDelta { text });
            }
            TurnEvent::ThinkingDelta(text) => {
                let _ = evt.send(ChatStreamEvent::ThinkingDelta { text });
            }
            TurnEvent::ToolUseStart { id, name } => {
                let _ = evt.send(ChatStreamEvent::ToolUseStart { id, name });
            }
            TurnEvent::ToolExecStart {
                id, name, summary, ..
            } => {
                if let Ok(mut m) = tool_names_ev.lock() {
                    m.insert(id.clone(), name.clone());
                }
                let _ = evt.send(ChatStreamEvent::ToolExecStart { id, name, summary });
            }
            TurnEvent::ToolConfirmPending { .. } => {
                // 桌面这一轮用户已经点了发送，不再弹确认。放行在 confirm_bridge。
            }
            TurnEvent::ToolUseResult {
                id,
                content,
                is_error,
            } => {
                let name = tool_names_ev
                    .lock()
                    .ok()
                    .and_then(|m| m.get(&id).cloned())
                    .unwrap_or_default();
                if !is_error {
                    if let Some(title) = hermes_core::companion::parse_source_read_title(&content) {
                        if let Ok(mut a) = allowed_ev.lock() {
                            if !a.iter().any(|t| t == &title) {
                                a.push(title);
                            }
                        }
                    }
                }
                let _ = evt.send(ChatStreamEvent::ToolUseResult {
                    id,
                    content: content.clone(),
                    is_error,
                });
                if name.starts_with("commitment_") {
                    let _ = app_ev.emit("hermes://zaiban-changed", ());
                    if let Some(z) = parse_zaiban_tool(&name, &content, is_error) {
                        let _ = evt.send(z);
                    }
                }
            }
            TurnEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            } => {
                let _ = evt.send(ChatStreamEvent::UsageUpdate {
                    input_tokens,
                    output_tokens,
                });
            }
            TurnEvent::Error(message) => {
                let _ = evt.send(ChatStreamEvent::Error {
                    message: hermes_llm::humanize_error_lang(&message, &ui_lang_ev),
                });
            }
            TurnEvent::Cancelled => {
                let _ = evt.send(ChatStreamEvent::Cancelled);
            }
            TurnEvent::Done => {
                done_held_ev.store(true, Ordering::SeqCst);
            }
        };

        let confirm_bridge = tokio::spawn(async move {
            while let Some(req) = confirm_rx.recv().await {
                let _ = req.reply.send(hermes_turn::ConfirmAction::Allow);
            }
        });

        let history_for_turn = hermes_channel::inject_time_header(history.clone());
        let result = hermes_turn::run_turn(
            provider.as_ref(),
            host.as_ref(),
            &tools,
            &history_for_turn,
            &config,
            Some(confirm_tx),
            on_turn_event,
            cancel_rx,
        )
        .await;

        confirm_bridge.abort();

        let mut turn_messages: Vec<hermes_core::Message> = Vec::new();
        let mut session_id_for_log = sid.clone();
        let mut session_persona_for_log: Option<String> = None;
        let mut session_team_for_log: Option<String> = None;
        let mut session_holder_for_log: Option<String> = None;

        match result {
            Ok(output) => {
                let mut msgs = output.new_messages;
                let allowed = allowed_titles.lock().map(|a| a.clone()).unwrap_or_default();
                for m in &mut msgs {
                    m.sanitize_material_cites(&allowed);
                }
                // 盖上这一轮开口的人。**不盖的代价是看得见的**：组会话里棒会换人
                // （海燕 → 吕老师 → 小宋），落盘之后分不出谁说的，界面会把两轮的
                // 气泡并成一块（用户原话：「吕老师消息被埋」）。
                hermes_core::message::stamp_speaker(&mut msgs, speaker.map(|p| p.id.as_str()));
                let corrected = last_assistant_text(&msgs);
                if !corrected.is_empty() {
                    let _ = on_event.send(ChatStreamEvent::TextCorrected { text: corrected });
                }
                // 正文已经全部发完，这才轮到「这一轮结束」。
                if done_held.swap(false, Ordering::SeqCst) {
                    let _ = on_event.send(ChatStreamEvent::Done);
                }
                turn_messages = msgs.clone();
                if let Some(s) = sessions_arc.lock().await.get_mut(&sid) {
                    session_id_for_log = s.session.meta.id.clone();
                    session_persona_for_log = s.session.meta.persona.clone();
                    session_team_for_log = s.session.meta.team.clone();
                    for msg in &msgs {
                        s.session.messages.push(msg.clone());
                        let to_disk = msg.for_persist(persist_thinking);
                        if to_disk.content.is_empty() {
                            continue;
                        }
                        if let Ok(w) = s.ensure_writer() {
                            let _ = w.append(&SessionEvent::Message(to_disk));
                        }
                    }
                    s.session.total_input_tokens += output.usage.input_tokens;
                    s.session.total_output_tokens += output.usage.output_tokens;
                    if let Ok(w) = s.ensure_writer() {
                        let _ = w.append(&SessionEvent::Usage(hermes_core::Usage {
                            input_tokens: output.usage.input_tokens,
                            output_tokens: output.usage.output_tokens,
                            cache_read_tokens: output.usage.cache_read_tokens,
                            cache_creation_tokens: output.usage.cache_creation_tokens,
                        }));
                    }
                    if s.session.meta.team.is_some() {
                        let _ = crate::commands::episode::sync_baton(
                            s,
                            std::path::Path::new(&workspace_for_baton),
                            None,
                        );
                    }
                    session_holder_for_log = s.session.flow.holder().map(str::to_string);
                }

                // 回答已经给用户。摘要只为下一轮瘦身，失败就留全文。
                let (snapshot, snapshot_len) = {
                    let guard = sessions_arc.lock().await;
                    match guard.get(&sid) {
                        Some(s) => (s.session.messages.clone(), s.session.messages.len()),
                        None => (Vec::new(), 0),
                    }
                };
                if snapshot_len > 0 {
                    let mut compactable = hermes_core::Session {
                        meta: session_meta.clone(),
                        messages: snapshot,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        flow: Default::default(),
                    };
                    let tools_json = serde_json::to_string(&tools).unwrap_or_default();
                    match hermes_core::compaction::maybe_compact(
                        provider.as_ref(),
                        &mut compactable,
                        &turn_system,
                        &tools_json,
                        compaction_policy,
                    )
                    .await
                    {
                        Ok(Some(done)) => {
                            if let Some(s) = sessions_arc.lock().await.get_mut(&sid) {
                                if s.session.messages.len() == snapshot_len {
                                    s.session.messages = compactable.messages;
                                    if let Ok(w) = s.ensure_writer() {
                                        let _ = w.append(&SessionEvent::Compaction(
                                            hermes_core::CompactionRecord {
                                                replaced: done.replaced,
                                                summary: done.summary.clone(),
                                                at: chrono::Utc::now(),
                                            },
                                        ));
                                    }
                                }
                            }
                            let _ = on_event.send(ChatStreamEvent::ContextCompacted {
                                replaced: done.replaced,
                                before_tokens: done.before_tokens,
                                after_tokens: done.after_tokens,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!(
                            error = %e,
                            "post-turn compaction failed; next turn still uses request-fold"
                        ),
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "turn failed");
                let _ = on_event.send(ChatStreamEvent::Error {
                    message: hermes_llm::humanize_error_lang(&format!("{e:#}"), &ui_lang),
                });
                // 失败也要收尾：少了 Done，界面上的流式开关会一直亮着。
                done_held.store(false, Ordering::SeqCst);
                let _ = on_event.send(ChatStreamEvent::Done);
            }
        }

        let drained: Vec<hermes_reflect::SkillCandidate> = {
            match propose_queue.lock() {
                Ok(mut q) => q.drain(..).collect(),
                Err(_) => Vec::new(),
            }
        };
        if !drained.is_empty() {
            let pending = hermes_reflect::ReflectionOutput {
                summary: String::new(),
                skill_candidates: drained,
                memory_candidates: vec![],
                conflicts: vec![],
            };
            match hermes_reflect::enqueue_from_reflection(
                &pending,
                hermes_reflect::InboxSource::ManualReflect,
            ) {
                Ok(n) if n > 0 => {
                    tracing::info!(added = n, "propose_skill enqueued to inbox");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error=%e, "enqueue proposed skill failed"),
            }
        }

        // Yield the model before housekeeping so the next send is not queued.
        drop(_live);

        // Micro-reflection: separate lifecycle from the turn stream.
        // Notifies UI via Tauri event `hermes://micro-reflection` (not Channel).
        micro::spawn_after_turn(
            app,
            provider_for_micro,
            memory_store,
            active_memories_arc,
            pinned_memories_arc,
            micro_cooldown,
            skills_for_micro,
            memories_for_micro,
            turn_messages,
            sid.clone(),
            session_id_for_log,
            session_persona_for_log,
            session_team_for_log,
            session_holder_for_log,
            auto_accept,
            min_confidence,
            live_llm,
        );

        cancel_tokens_arc.lock().await.remove(&sid);
    });

    Ok(())
}

/// 这一轮系统提示词的来源。人物只从 `session.meta.persona` 取——没绑定或不认识的
/// id → `None`（容错与 `persona::memory_owner_for` 一致，见 `personas::bound_persona`）。
///
/// 单独成函数是为了让「会话身份 → 人物块」这条接线被测试直接盯住：把 `persona`
/// 写死回 `None`，`a_bound_persona_reaches_the_turn_prompt` 必须红。
#[allow(clippy::too_many_arguments)]
fn turn_sources<'a>(
    speaker: Option<&'a hermes_core::persona::Persona>,
    roster: &'a [&'a hermes_core::persona::Persona],
    team: Option<crate::context::TeamContext<'a>>,
    topic_cards: Option<&'a str>,
    compiled_profile: Option<&'a str>,
    pinned: &'a [hermes_memory::LoadedMemory],
    active: &'a [hermes_memory::LoadedMemory],
    all_skills: &'a [hermes_skills::LoadedSkill],
    open_work: &'a [hermes_commitments::Commitment],
    material_hits: &'a [hermes_core::MaterialHit],
    first_human_today: bool,
    workspace_root: &'a str,
    limits: hermes_llm::ContextLimits,
) -> ContextSources<'a> {
    ContextSources {
        base: None,
        persona: speaker,
        team,
        roster,
        topic_cards,
        compiled_profile,
        pinned,
        active,
        all_skills,
        open_work,
        material_hits,
        first_human_today,
        workspace_root,
        limits,
    }
}

/// 这一轮**看得见**的记忆：`(active, 其中的 pinned)`。
///
/// 装配只走 `hermes_memory::views_for_any` 一处（与 CLI / server 同一个函数）——
/// GUI 曾经在这里直接把全量记忆喂进提示词，于是某个工位的会话看得见别人的记忆，
/// 而主题卡那边却是按工位切过的：同一轮里两条轴不一致，是明确的隔离失效。
/// 单独成函数是为了让「按归属切」这条接线可测：归属传空必须只剩全局。
fn visible_memories(
    all: &[hermes_memory::LoadedMemory],
    owners: &[&str],
) -> (
    Vec<hermes_memory::LoadedMemory>,
    Vec<hermes_memory::LoadedMemory>,
) {
    hermes_memory::views_for_any(all, owners)
}

fn parse_zaiban_tool(name: &str, content: &str, is_error: bool) -> Option<ChatStreamEvent> {
    if is_error || !name.starts_with("commitment_") {
        return None;
    }
    if content.starts_with("Near existing") {
        let existing_id = bracket_id(content)?;
        let existing_title = content
            .split('「')
            .nth(1)
            .and_then(|s| s.split('」').next())
            .map(|s| s.to_string());
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: "near".into(),
            id: None,
            title: None,
            existing_id: Some(existing_id),
            existing_title,
        });
    }
    if content.starts_with("Recorded open work") || content.starts_with("Folded into") {
        let id = bracket_id(content);
        let title = content
            .split("]: ")
            .nth(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let action = if content.starts_with("Folded") {
            "folded"
        } else {
            "saved"
        };
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: action.into(),
            id,
            title,
            existing_id: None,
            existing_title: None,
        });
    }
    if content.starts_with("Closed")
        || content.starts_with("Dropped")
        || content.starts_with("Updated")
        || content.starts_with("Split")
    {
        return Some(ChatStreamEvent::ZaibanUpdated {
            action: "changed".into(),
            id: bracket_id(content),
            title: None,
            existing_id: None,
            existing_title: None,
        });
    }
    Some(ChatStreamEvent::ZaibanUpdated {
        action: "changed".into(),
        id: None,
        title: None,
        existing_id: None,
        existing_title: None,
    })
}

fn bracket_id(s: &str) -> Option<String> {
    let start = s.find('[')? + 1;
    let rest = s.get(start..)?;
    let end = rest.find(']')?;
    let id = rest[..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Drop messages after the last human user turn (keeps that user message).
/// Used before `regenerate_turn`.
#[tauri::command]
pub async fn truncate_after_last_user(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<usize, GuiError> {
    let mut sessions = state.sessions.lock().await;
    let active = sessions
        .get_mut(&session_id)
        .ok_or_else(|| GuiError::Session("session not found".into()))?;

    let keep = end_after_last_human_user(&active.session.messages)
        .ok_or_else(|| GuiError::Session("no user message to regenerate from".into()))?;

    if keep >= active.session.messages.len() {
        // Nothing after last user — still ok (empty assistant).
        return Ok(keep);
    }

    active.session.messages.truncate(keep);
    active.writer = None;
    hermes_store::rewrite_session(&active.path, &active.session)
        .map_err(|e| GuiError::Session(e.to_string()))?;
    active.writer = Some(
        SessionWriter::open_append(&active.path).map_err(|e| GuiError::Session(e.to_string()))?,
    );
    Ok(keep)
}

/// Keep only the first `keep_count` raw messages (0..=len). Used for edit-resend.
#[tauri::command]
pub async fn truncate_session(
    state: State<'_, AppState>,
    session_id: String,
    keep_count: usize,
) -> Result<(), GuiError> {
    let mut sessions = state.sessions.lock().await;
    let active = sessions
        .get_mut(&session_id)
        .ok_or_else(|| GuiError::Session("session not found".into()))?;

    if keep_count > active.session.messages.len() {
        return Err(GuiError::Session(format!(
            "keep_count {keep_count} > message len {}",
            active.session.messages.len()
        )));
    }

    // No disk file yet (empty draft) — memory only.
    if active.writer.is_none() && !active.path.exists() {
        active.session.messages.truncate(keep_count);
        return Ok(());
    }

    active.session.messages.truncate(keep_count);
    active.writer = None;
    hermes_store::rewrite_session(&active.path, &active.session)
        .map_err(|e| GuiError::Session(e.to_string()))?;
    if active.path.exists() {
        active.writer = Some(
            SessionWriter::open_append(&active.path)
                .map_err(|e| GuiError::Session(e.to_string()))?,
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_stream(state: State<'_, AppState>, session_id: String) -> Result<(), GuiError> {
    if let Some(tx) = state.cancel_tokens.lock().await.remove(&session_id) {
        let _ = tx.send(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_with(persona: Option<&str>, team: Option<&str>) -> ActiveSession {
        let mut meta = SessionMeta::new("test-model", "test-provider");
        meta.persona = persona.map(str::to_string);
        meta.team = team.map(str::to_string);
        ActiveSession {
            session: Session::new(meta),
            writer: None,
            path: std::path::PathBuf::from("/tmp/lebi-gui-turn-sources.jsonl"),
        }
    }

    fn draft(persona: Option<&str>) -> ActiveSession {
        draft_with(persona, None)
    }

    fn mem(owner: Option<&str>, body: &str, pinned: bool) -> hermes_memory::LoadedMemory {
        let mut fm = hermes_memory::MemoryFrontmatter::new(
            hermes_memory::Source::User,
            hermes_memory::Confidence::High,
            vec![],
            "general".into(),
        )
        .owned(owner.map(str::to_string));
        fm.pinned = pinned;
        hermes_memory::LoadedMemory {
            frontmatter: fm,
            body: body.into(),
            source_path: std::path::PathBuf::from("/nowhere"),
            scope: hermes_memory::Scope::User,
        }
    }

    /// 提示词里那份记忆必须**按归属切过**：只留「全局 + 本人物」。
    /// 归属传空（GUI 曾经就是这样直接把全量喂进去），这条必须红。
    #[test]
    fn visible_memories_keeps_globals_and_own_only() {
        let all = vec![
            mem(None, "全局：交付一律给 Word 放桌面", true),
            mem(Some("sao-di-seng"), "自己的：林碳报告只引 IEA", false),
            mem(Some("wang-hai-yan"), "别人的：标题不夸张", true),
        ];

        let (view, pinned) = visible_memories(&all, &["sao-di-seng"]);
        assert_eq!(
            view.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            vec!["全局：交付一律给 Word 放桌面", "自己的：林碳报告只引 IEA"],
            "人物会话只拿「全局 + 本人物」"
        );
        assert!(
            !view
                .iter()
                .any(|m| m.frontmatter.owner.as_deref() == Some("wang-hai-yan")),
            "别的人物的记忆一条都不许进提示词"
        );
        assert_eq!(pinned.len(), 1, "别人的 pinned 不许借「置顶」溜进来");
        assert_eq!(pinned[0].frontmatter.owner, None);

        let (globals, _) = visible_memories(&all, &[]);
        assert_eq!(globals.len(), 1, "无人物 / 自带角色 → 只看全局");

        // 组会话（§4.2）：组规 + 这一轮说话的人的手艺，两样都要；别人的一条不给。
        let team = vec![
            mem(None, "全局：交付一律给 Word 放桌面", true),
            mem(Some("caifu-zaozhidao"), "组规：往期公告都吃 1/3", true),
            mem(Some("wang-hai-yan"), "手艺：采料先按出处分级", false),
            mem(Some("xiao-yu"), "别人的手艺：数字怎么念先标出来", true),
        ];
        let (view, _) = visible_memories(&team, &["caifu-zaozhidao", "wang-hai-yan"]);
        assert_eq!(
            view.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            vec![
                "全局：交付一律给 Word 放桌面",
                "组规：往期公告都吃 1/3",
                "手艺：采料先按出处分级",
            ],
            "组会话 = 全局 + 组 + 这一轮说话的人"
        );
        // 接了棒换人：这一轮换成小雨，海燕的手艺就不在提示词里了
        let (view, _) = visible_memories(&team, &["caifu-zaozhidao", "xiao-yu"]);
        assert!(
            !view
                .iter()
                .any(|m| m.frontmatter.owner.as_deref() == Some("wang-hai-yan")),
            "换了人，前一个人的手艺不许留在桌上"
        );
    }

    fn system_prompt(persona: Option<&str>) -> String {
        turn_prompt(draft(persona), None)
    }

    fn turn_prompt(active: ActiveSession, team: Option<&str>) -> String {
        // 名册给两个人，其中一个是「我」——「我」不许出现在自己的名单里。
        let roster = [
            hermes_core::persona::get("sao-di-seng").unwrap(),
            hermes_core::persona::get("yu-tian").unwrap(),
        ];
        let present: Vec<&str> = roster.iter().map(|p| p.id.as_str()).collect();
        let team_ctx = team
            .and_then(hermes_core::team::get)
            .map(|t| crate::context::TeamContext {
                team: t,
                present: &present,
                speaker: t.interface.as_str(),
            });
        let speaker = hermes_core::persona::speaker_for(
            active.session.meta.persona.as_deref(),
            active.session.meta.team.as_deref(),
            active.session.flow.holder(),
        );
        turn_sources(
            speaker,
            &roster,
            team_ctx,
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
            "/tmp/ws",
            hermes_llm::ContextLimits::default(),
        )
        .build_turn_system("你好")
    }

    /// 组会话：提示词里既要有「你是谁」（这一轮开口的人），也要有「你在哪张桌子上」。
    /// 这条接线断了，组会话会退化成一条**无人物会话**——而且界面上看不出来。
    #[test]
    fn a_team_session_reaches_the_turn_prompt_with_its_table() {
        let s = turn_prompt(
            draft_with(None, Some("caifu-zaozhidao")),
            Some("caifu-zaozhidao"),
        );
        assert!(s.contains("## 你现在是谁"), "说话人的人物块要在：{s}");
        assert!(s.contains("## 你在哪张桌子上"), "组块要在：{s}");
        assert!(s.contains("财富早知道"), "{s}");

        // 不认识的组：不 panic、也不凭空长出一个人格或一张桌子。
        let unknown = turn_prompt(draft_with(None, Some("nobody")), None);
        assert!(!unknown.contains("## 你现在是谁"), "{unknown}");
        assert!(!unknown.contains("## 你在哪张桌子上"), "{unknown}");
    }

    #[test]
    fn a_bound_persona_reaches_the_turn_prompt() {
        let bound = system_prompt(Some("sao-di-seng"));
        assert!(bound.contains("## 你现在是谁"), "{bound}");
        assert!(
            bound.contains("产业专家扫地僧"),
            "人物块必须带上这位人物的身份：{bound}"
        );
        assert!(
            bound.contains("- 编辑雨天 · 错在哪，我指给你"),
            "指路名册必须进提示词，且写成侧栏那行的样子：{bound}"
        );
        assert!(
            !bound.contains("- 产业专家扫地僧 · "),
            "名册是「其他工位」，不该把自己也列进去：{bound}"
        );

        let free = system_prompt(None);
        assert!(!free.contains("## 你现在是谁"), "{free}");

        let unknown = system_prompt(Some("who-is-this"));
        assert!(
            !unknown.contains("## 你现在是谁"),
            "不认识的 id 不该凭空长出一个人物块：{unknown}"
        );
    }

    /// 纠正文只认**最后一段**：过程旁白（更早那条 assistant）不能被灌进回答气泡。
    #[test]
    fn the_correction_takes_the_last_segment_only() {
        let msgs = vec![
            Message::user_text("干活"),
            Message {
                role: Role::Assistant,
                content: vec![
                    hermes_core::ContentBlock::Text {
                        text: "我先去查。".into(),
                    },
                    hermes_core::ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "bash".into(),
                        input: serde_json::json!({}),
                    },
                ],
                at: None,
                speaker: None,
            },
            Message {
                role: Role::Assistant,
                content: vec![
                    hermes_core::ContentBlock::Thinking {
                        thinking: "想一下怎么说".into(),
                        signature: None,
                    },
                    hermes_core::ContentBlock::Text {
                        text: "正文一".into(),
                    },
                    hermes_core::ContentBlock::Text {
                        text: "，正文二".into(),
                    },
                ],
                at: None,
                speaker: None,
            },
        ];
        assert_eq!(last_assistant_text(&msgs), "正文一，正文二");
        assert_eq!(last_assistant_text(&[]), "");
    }
}
