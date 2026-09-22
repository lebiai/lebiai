//! Reflection-flow: run after a chat session ends.
//!
//! Walks every candidate, asks the user, and persists accepted ones.
//! Memory candidates carrying a `supersedes` link that maps to an existing
//! `ConflictCandidate` are presented together with a five-option prompt:
//! `n` keep_new, `o` keep_old, `m` merge (→ $EDITOR), `s` scope_split,
//! `k` skip.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use hermes_core::{LlmProvider, Session};
use hermes_memory::{FsMemoryStore, LoadedMemory, MemoryStore, Scope as MemoryScope};
use hermes_reflect::{
    reflect, CandidateKind, ConflictCandidate, EnqueueMark, InboxPayload, InboxSource,
    MemoryCandidate, ReflectionOutput, SkillCandidate,
};
use hermes_skills::{FsSkillStore, Scope as SkillScope, SkillFrontmatter, SkillStore};
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};

#[derive(Debug, Clone, Copy)]
enum Action {
    Accept,
    Reject,
    Defer,
}

#[derive(Debug, Clone, Copy)]
enum ConflictAction {
    /// Write the new candidate; ensure `supersedes` includes the old id.
    KeepNew,
    /// Drop the new candidate (old wins unchanged).
    KeepOld,
    /// Open `$EDITOR`; write the edited body as the new candidate with
    /// `supersedes` pointing to the old id.
    Merge,
    /// Write the new candidate into the opposite scope (`user`↔`project`);
    /// drop the `supersedes` link so both stay active.
    ScopeSplit,
    /// Skip this candidate entirely, leaving state unchanged.
    Skip,
}

/// Run full reflection, but skip silently when the session has fewer than
/// `min_turns` user messages. Use this for the quit-driven (session-end) path.
pub async fn run_with_min_turns(
    provider: &dyn LlmProvider,
    session: &Session,
    min_turns: usize,
) -> Result<()> {
    if session.messages.is_empty() {
        return Ok(());
    }
    let user_turns = session
        .messages
        .iter()
        .filter(|m| {
            matches!(m.role, hermes_core::Role::User)
                && m.content
                    .iter()
                    .any(|b| matches!(b, hermes_core::ContentBlock::Text { .. }))
        })
        .count();
    if user_turns < min_turns {
        tracing::info!(
            user_turns,
            min_turns,
            "skipping reflection (user turns below threshold)"
        );
        return Ok(());
    }

    eprintln!();
    eprintln!("(reflecting on session...)");

    let skill_store = FsSkillStore::standard().map_err(|e| anyhow::anyhow!("skill store: {e}"))?;
    let memory_store =
        FsMemoryStore::standard().map_err(|e| anyhow::anyhow!("memory store: {e}"))?;
    let active_skills = skill_store
        .list()
        .map_err(|e| anyhow::anyhow!("listing skills: {e}"))?;
    let active_memories = memory_store
        .list_active()
        .map_err(|e| anyhow::anyhow!("listing memories: {e}"))?;

    let memory_by_id: HashMap<String, LoadedMemory> = active_memories
        .iter()
        .cloned()
        .map(|m| (m.frontmatter.id.clone(), m))
        .collect();

    // 本会话的工位（没人物 / 自带角色 → `None`）。批准候选时靠它判归属。
    let session_owner = hermes_core::persona::memory_owner_for(
        session.meta.persona.as_deref(),
        session.meta.team.as_deref(),
    );

    // Re-evaluate pending candidates from previous sessions before running
    // reflection for this one (P0 第一条: candidates must go through human
    // approval — a queue with no consumer would never be reviewed).
    absorb_legacy_deferred_queue();
    review_deferred(&session.meta.id, &skill_store, &memory_store).await?;

    let output = reflect(provider, session, &active_skills, &active_memories)
        .await
        .map_err(|e| anyhow::anyhow!("reflection: {e}"))?;

    print_summary(&output);

    if output.is_empty() {
        eprintln!("(no candidates this round.)");
        return Ok(());
    }

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    // --- skills ---
    let mut written_skills = 0;
    let n_skills = output.skill_candidates.len();
    for (i, c) in output.skill_candidates.iter().enumerate() {
        let outcome = prompt_skill(c, i + 1, n_skills, &mut reader).await?;
        match outcome {
            Some(Action::Accept) => match persist_skill(&skill_store, c) {
                Ok(path) => {
                    eprintln!("  ✓ wrote {}", path.display());
                    written_skills += 1;
                }
                Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
            },
            Some(Action::Reject) => eprintln!("  (rejected)"),
            Some(Action::Defer) => {
                defer_to_inbox(InboxPayload::Skill(c.clone()), session_owner.clone());
                eprintln!("  (deferred — will appear next session)");
            }
            None => {
                eprintln!("  (stdin closed; skipping remaining candidates)");
                log_action(
                    &session.meta.id,
                    hermes_reflect::CandidateKind::Skill,
                    &c.name,
                    hermes_reflect::ActionTaken::Cancelled,
                );
                return Ok(());
            }
        }
        if let Some(act) = outcome {
            log_action(
                &session.meta.id,
                hermes_reflect::CandidateKind::Skill,
                &c.name,
                map_action(act),
            );
        }
    }

    // --- memories + conflicts ---
    // Build a multimap of conflict-by-referenced-old-id so we can attach
    // each to the memory that supersedes it.
    let mut conflicts_by_old: HashMap<String, Vec<&ConflictCandidate>> = HashMap::new();
    for c in &output.conflicts {
        conflicts_by_old.entry(c.with.clone()).or_default().push(c);
    }

    let mut written_memories = 0;
    let n_mem = output.memory_candidates.len();
    let mut conflicts_handled: HashSet<String> = HashSet::new();

    for (i, c) in output.memory_candidates.iter().enumerate() {
        // Does this candidate's supersedes link to any active memory that
        // also has a conflict report? If so, ask via the conflict prompt.
        let linked: Vec<(&String, &ConflictCandidate)> = c
            .supersedes
            .iter()
            .filter_map(|id| {
                conflicts_by_old
                    .get(id)
                    .and_then(|v| v.first())
                    .map(|cc| (id, *cc))
            })
            .collect();

        if let Some((old_id, conflict)) = linked.first().copied() {
            let old = memory_by_id.get(old_id);
            let outcome = prompt_conflict(c, conflict, old, i + 1, n_mem, &mut reader).await?;
            match outcome {
                Some(action) => {
                    conflicts_handled.insert(old_id.clone());
                    log_action(
                        &session.meta.id,
                        hermes_reflect::CandidateKind::ConflictMemory,
                        c.fact.lines().next().unwrap_or(""),
                        map_conflict_action(action),
                    );
                    match apply_conflict_action(
                        &memory_store,
                        c,
                        old_id,
                        action,
                        session_owner.as_deref(),
                    ) {
                        Ok(Some(path)) => {
                            eprintln!("  ✓ wrote {}", path.display());
                            written_memories += 1;
                        }
                        Ok(None) => {
                            eprintln!("  (no write performed)");
                        }
                        Err(e) => eprintln!("  ✗ failed: {e:#}"),
                    }
                }
                None => {
                    eprintln!("  (stdin closed; skipping remaining candidates)");
                    log_action(
                        &session.meta.id,
                        hermes_reflect::CandidateKind::ConflictMemory,
                        c.fact.lines().next().unwrap_or(""),
                        hermes_reflect::ActionTaken::Cancelled,
                    );
                    return Ok(());
                }
            }
            continue;
        }

        // No conflict — classic a/r/d flow.
        let outcome = prompt_memory(c, i + 1, n_mem, &mut reader).await?;
        match outcome {
            Some(Action::Accept) => {
                match persist_memory(&memory_store, c, session_owner.as_deref()) {
                    Ok(Some(path)) => {
                        eprintln!("  ✓ wrote {}", path.display());
                        written_memories += 1;
                    }
                    Ok(None) => eprintln!("  ↩ already remembered — nothing new written"),
                    Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
                }
            }
            Some(Action::Reject) => eprintln!("  (rejected)"),
            Some(Action::Defer) => {
                defer_to_inbox(InboxPayload::Memory(c.clone()), session_owner.clone());
                eprintln!("  (deferred — will appear next session)");
            }
            None => {
                eprintln!("  (stdin closed; skipping remaining candidates)");
                log_action(
                    &session.meta.id,
                    hermes_reflect::CandidateKind::Memory,
                    c.fact.lines().next().unwrap_or(""),
                    hermes_reflect::ActionTaken::Cancelled,
                );
                return Ok(());
            }
        }
        if let Some(act) = outcome {
            log_action(
                &session.meta.id,
                hermes_reflect::CandidateKind::Memory,
                c.fact.lines().next().unwrap_or(""),
                map_action(act),
            );
        }
    }

    // --- unresolved conflicts (no memory candidate references them) ---
    let leftover: Vec<&ConflictCandidate> = output
        .conflicts
        .iter()
        .filter(|c| !conflicts_handled.contains(&c.with))
        .collect();
    if !leftover.is_empty() {
        eprintln!();
        eprintln!(
            "== Unresolved conflicts (no linked memory candidate): {} ==",
            leftover.len()
        );
        for c in &leftover {
            eprintln!("  with {}: {} — {}", c.with, c.kind, c.explain);
            if let Some(old) = memory_by_id.get(&c.with) {
                eprintln!("    OLD: {}", old.body.trim());
            }
            loop {
                eprint!("    [d]elete old / [k]eep / [m]anual edit ▸ ");
                std::io::stderr().flush().ok();
                let line = reader.next_line().await.context("reading stdin")?;
                let Some(line) = line else { break };
                match line.trim().to_lowercase().as_str() {
                    "d" | "delete" => {
                        if let Some(old) = memory_by_id.get(&c.with) {
                            match memory_store.delete(old.scope, &c.with) {
                                Ok(true) => eprintln!("    ✓ deleted {}", c.with),
                                Ok(false) => eprintln!("    (not found on disk)"),
                                Err(e) => eprintln!("    ✗ delete failed: {e:#}"),
                            }
                        } else {
                            eprintln!(
                                "    (memory {} not found — may have been hallucinated)",
                                c.with
                            );
                        }
                        break;
                    }
                    "k" | "keep" | "" => {
                        eprintln!("    (kept — no changes)");
                        break;
                    }
                    "m" | "manual" => {
                        if let Some(old) = memory_by_id.get(&c.with) {
                            let initial = format!(
                                "# Edit the memory body. Blank = cancel.\n\n{}\n",
                                old.body.trim()
                            );
                            match super::editor::edit(&initial, "memory-edit") {
                                Ok(Some(edited)) => {
                                    let body: String = edited
                                        .lines()
                                        .filter(|l| !l.trim_start().starts_with('#'))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                        .trim()
                                        .to_string();
                                    if body.is_empty() {
                                        eprintln!("    (empty body; cancelled)");
                                    } else {
                                        let mut fm = old.frontmatter.clone();
                                        fm.supersedes.push(c.with.clone());
                                        match memory_store.put(old.scope, fm, &body) {
                                            Ok(p) => eprintln!("    ✓ wrote {}", p.display()),
                                            Err(e) => eprintln!("    ✗ write failed: {e:#}"),
                                        }
                                    }
                                }
                                Ok(None) => eprintln!("    (editor cancelled)"),
                                Err(e) => eprintln!("    ✗ editor failed: {e:#}"),
                            }
                        } else {
                            eprintln!("    (memory {} not found — cannot edit)", c.with);
                        }
                        break;
                    }
                    other => eprintln!("    (unknown action {other:?} — try d / k / m)"),
                }
            }
            log_action(
                &session.meta.id,
                hermes_reflect::CandidateKind::OrphanConflict,
                &c.explain,
                hermes_reflect::ActionTaken::Defer,
            );
        }
    }

    eprintln!();
    eprintln!(
        "reflection done — skills: {written_skills} / {n_skills},  memories: {written_memories} / {n_mem}"
    );
    Ok(())
}

fn print_summary(out: &ReflectionOutput) {
    if !out.summary.is_empty() {
        eprintln!("summary: {}", out.summary);
    }
    eprintln!(
        "candidates: {} skill / {} memory / {} conflict",
        out.skill_candidates.len(),
        out.memory_candidates.len(),
        out.conflicts.len()
    );
    eprintln!();
}

async fn prompt_skill(
    c: &SkillCandidate,
    idx: usize,
    total: usize,
    reader: &mut Lines<BufReader<Stdin>>,
) -> Result<Option<Action>> {
    eprintln!("[Skill {idx}/{total}]  name: {}", c.name);
    eprintln!("  description: {}", c.description);
    if !c.triggers.is_empty() {
        eprintln!("  triggers:    {}", c.triggers.join(", "));
    }
    eprintln!("  rationale:   {}", c.rationale);
    eprintln!("  confidence:  {:?}", c.confidence);
    let preview: String = c.body.lines().take(3).collect::<Vec<_>>().join("\n  | ");
    eprintln!("  body (preview):");
    eprintln!("  | {preview}");
    let body_lines = c.body.lines().count();
    if body_lines > 3 {
        eprintln!("  | ... ({} more lines)", body_lines - 3);
    }
    prompt_ard(reader).await
}

async fn prompt_memory(
    c: &MemoryCandidate,
    idx: usize,
    total: usize,
    reader: &mut Lines<BufReader<Stdin>>,
) -> Result<Option<Action>> {
    eprintln!("[Memory {idx}/{total}]  fact: {}", c.fact);
    eprintln!("  scope:       {:?}", c.scope);
    if !c.tags.is_empty() {
        eprintln!("  tags:        {}", c.tags.join(", "));
    }
    eprintln!("  confidence:  {:?}", c.confidence);
    eprintln!("  rationale:   {}", c.rationale);
    if !c.supersedes.is_empty() {
        eprintln!("  supersedes:  {}", c.supersedes.join(", "));
    }
    prompt_ard(reader).await
}

async fn prompt_conflict(
    new: &MemoryCandidate,
    conflict: &ConflictCandidate,
    old: Option<&LoadedMemory>,
    idx: usize,
    total: usize,
    reader: &mut Lines<BufReader<Stdin>>,
) -> Result<Option<ConflictAction>> {
    eprintln!("[Memory {idx}/{total}]  ⚠ CONFLICT");
    eprintln!("  explain: {}", conflict.explain);
    eprintln!("  kind:    {}", conflict.kind);
    eprintln!();
    if let Some(o) = old {
        eprintln!(
            "  OLD [{}] ({:?}): {}",
            o.frontmatter.id,
            o.scope,
            o.body.trim()
        );
    } else {
        eprintln!(
            "  OLD [{}]: (memory not found on disk — LLM may have hallucinated the id)",
            conflict.with
        );
    }
    eprintln!("  NEW fact: {}", new.fact);
    eprintln!(
        "           scope: {:?}, tags: {}",
        new.scope,
        new.tags.join(", ")
    );
    eprintln!();
    loop {
        eprint!(
            "  Resolve: [n]ew-supersedes-old / [o]ld-keep / [m]erge (edit) / [s]cope-split / s[k]ip ▸ "
        );
        std::io::stderr().flush().ok();
        let line = reader.next_line().await.context("reading stdin")?;
        let Some(line) = line else { return Ok(None) };
        match line.trim().to_lowercase().as_str() {
            "n" | "new" => return Ok(Some(ConflictAction::KeepNew)),
            "o" | "old" => return Ok(Some(ConflictAction::KeepOld)),
            "m" | "merge" => return Ok(Some(ConflictAction::Merge)),
            "s" | "split" | "scope-split" => return Ok(Some(ConflictAction::ScopeSplit)),
            "k" | "skip" | "" => return Ok(Some(ConflictAction::Skip)),
            other => eprintln!("  (unknown action {other:?} — try n/o/m/s/k)"),
        }
    }
}

async fn prompt_ard(reader: &mut Lines<BufReader<Stdin>>) -> Result<Option<Action>> {
    loop {
        eprint!("  Action: [a]ccept / [r]eject / [d]efer ▸ ");
        std::io::stderr().flush().ok();
        let line = reader.next_line().await.context("reading stdin")?;
        let Some(line) = line else { return Ok(None) };
        match line.trim().to_lowercase().as_str() {
            "a" | "accept" | "y" | "yes" => return Ok(Some(Action::Accept)),
            "r" | "reject" | "n" | "no" => return Ok(Some(Action::Reject)),
            "d" | "defer" | "" => return Ok(Some(Action::Defer)),
            other => eprintln!("  (unknown action {other:?} — try a / r / d)"),
        }
    }
}

fn apply_conflict_action(
    store: &FsMemoryStore,
    new: &MemoryCandidate,
    old_id: &str,
    action: ConflictAction,
    session_owner: Option<&str>,
) -> Result<Option<PathBuf>> {
    match action {
        ConflictAction::KeepNew => {
            let mut c = new.clone();
            if !c.supersedes.iter().any(|id| id == old_id) {
                c.supersedes.push(old_id.to_string());
            }
            persist_memory(store, &c, session_owner)
        }
        ConflictAction::KeepOld => {
            eprintln!("  (old kept; new candidate discarded)");
            Ok(None)
        }
        ConflictAction::Merge => {
            let initial = format!(
                "# Edit the merged memory body. Blank = cancel.\n\n{}\n",
                new.fact
            );
            match super::editor::edit(&initial, "memory-merge")? {
                Some(edited) => {
                    // Strip leading comment lines that begin with `#`.
                    let body: String = edited
                        .lines()
                        .filter(|l| !l.trim_start().starts_with('#'))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim()
                        .to_string();
                    if body.is_empty() {
                        eprintln!("  (empty body after editing; cancelled)");
                        return Ok(None);
                    }
                    let mut c = new.clone();
                    c.fact = body;
                    if !c.supersedes.iter().any(|id| id == old_id) {
                        c.supersedes.push(old_id.to_string());
                    }
                    persist_memory(store, &c, session_owner)
                }
                None => {
                    eprintln!("  (editor cancelled)");
                    Ok(None)
                }
            }
        }
        ConflictAction::ScopeSplit => {
            // Put the new candidate in the opposite scope and drop the
            // supersedes link so the old memory remains active.
            let new_scope = match new.scope {
                MemoryScope::User => MemoryScope::Project,
                MemoryScope::Project => MemoryScope::User,
            };
            let mut c = new.clone();
            c.scope = new_scope;
            c.supersedes.retain(|id| id != old_id);
            persist_memory(store, &c, session_owner)
        }
        ConflictAction::Skip => {
            eprintln!("  (skipped — no changes)");
            Ok(None)
        }
    }
}

fn persist_skill(store: &FsSkillStore, c: &SkillCandidate) -> Result<PathBuf> {
    use serde_yaml::{Mapping, Value};
    let mut extra = Mapping::new();
    extra.insert(
        Value::String("source".into()),
        Value::String("reflection".into()),
    );
    extra.insert(
        Value::String("confidence".into()),
        Value::String(format!("{:?}", c.confidence).to_lowercase()),
    );

    let fm = SkillFrontmatter {
        name: c.name.clone(),
        description: c.description.clone(),
        triggers: c.triggers.clone(),
        version: Some("0.1.0".into()),
        license: None,
        always_active: false,
        extra,
    };
    store
        .put(SkillScope::User, fm, &c.body)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Interactively review a single skill candidate proposed mid-session (e.g.
/// by the `propose_skill` tool). Accept → write to skill store; Reject /
/// Defer → just log the decision. Used by the chat loop's drain step.
pub(crate) async fn review_proposed_skill(
    c: &SkillCandidate,
    session_id: &str,
    skill_store: &FsSkillStore,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    eprintln!(
        "\n{}",
        crate::commands::style::paint("1;36", "== Proposed skill (from agent) ==")
    );
    let outcome = prompt_skill(c, 1, 1, &mut reader).await?;
    match outcome {
        Some(Action::Accept) => match persist_skill(skill_store, c) {
            Ok(path) => eprintln!("  ✓ wrote {}", path.display()),
            Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
        },
        Some(Action::Reject) => eprintln!("  (rejected)"),
        Some(Action::Defer) => {
            defer_to_inbox(InboxPayload::Skill(c.clone()), None);
            eprintln!("  (deferred — will appear next session)");
        }
        None => eprintln!("  (stdin closed; skipping)"),
    }
    if let Some(act) = outcome {
        log_action(
            session_id,
            hermes_reflect::CandidateKind::Skill,
            &c.name,
            map_action(act),
        );
    } else {
        // stdin closed — still log as cancelled.
        log_action(
            session_id,
            hermes_reflect::CandidateKind::Skill,
            &c.name,
            hermes_reflect::ActionTaken::Cancelled,
        );
    }
    Ok(())
}

/// Persist a memory candidate: a fresh id, `supersedes` set to whatever the
/// candidate carries. Reused by `hermes distill` to write the survivor of a
/// cluster (its `supersedes` lists the other members' ids).
///
/// `session_owner` 是候选来源会话的工位（`SessionMeta.persona` →
/// `memory_owner_for`）。没有来源会话（跨会话的 distill、取不到会话）传 `None`。
/// 归属判定只有 `hermes_memory::resolve_owner` 一个地方，这里只传参。
///
/// 返回 `Ok(None)` = **库里已经有一条同样的**（查重闸门在 `put` 里，P1-4）：
/// 没重复写，但这不算失败 —— 用户点头的是「记住这件事」，那件事已经在里面了。
pub(crate) fn persist_memory(
    store: &FsMemoryStore,
    c: &MemoryCandidate,
    session_owner: Option<&str>,
) -> Result<Option<PathBuf>> {
    let fm = hermes_reflect::candidate::frontmatter_for(c, session_owner);
    match hermes_reflect::candidate::put_with_fallback(store, c.scope, fm, &c.fact) {
        Ok(path) => Ok(Some(path)),
        Err(hermes_memory::MemoryStoreError::Conflict { .. }) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}

/// Review the **single** pending queue (`pending-review.json`, P1-5).
///
/// 以前 CLI 读的是自己那一份 `deferred.jsonl`，而 GUI/server 的待审长在
/// `pending-review.json`：同机同一用户两条队列各自长、互相看不见。现在只有一条。
///
/// 点头 = 落盘 + 从队列里移除；摇头 = 只移除；推迟 = 留在队列里等下次。
/// stdin 断了就整体不动 —— 队列原封不动留给下一次。
async fn review_deferred(
    session_id: &str,
    skill_store: &FsSkillStore,
    memory_store: &FsMemoryStore,
) -> Result<()> {
    let pending = match hermes_reflect::inbox_list() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, "loading the pending queue failed");
            return Ok(());
        }
    };
    if pending.is_empty() {
        return Ok(());
    }

    eprintln!();
    eprintln!(
        "{}",
        crate::commands::style::paint("1;36", "== Pending candidates from previous sessions ==")
    );
    eprintln!("({} item(s) awaiting your decision)", pending.len());

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    let mut skills_done = 0usize;
    let mut memories_done = 0usize;
    let total = pending.len();
    for (i, item) in pending.iter().enumerate() {
        let (kind, label, action) = match &item.payload {
            InboxPayload::Skill(c) => {
                let outcome = prompt_skill(c, i + 1, total, &mut reader).await?;
                let label = c.name.clone();
                let act = match outcome {
                    Some(Action::Accept) => {
                        match persist_skill(skill_store, c) {
                            Ok(path) => {
                                eprintln!("  ✓ wrote {}", path.display());
                                skills_done += 1;
                            }
                            Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
                        }
                        Some(Action::Accept)
                    }
                    Some(Action::Reject) => {
                        eprintln!("  (rejected)");
                        Some(Action::Reject)
                    }
                    Some(Action::Defer) => {
                        eprintln!("  (kept pending)");
                        Some(Action::Defer)
                    }
                    None => {
                        eprintln!("  (stdin closed; queue kept for next session)");
                        return Ok(());
                    }
                };
                (CandidateKind::Skill, label, act)
            }
            InboxPayload::Memory(c) => {
                let outcome = prompt_memory(c, i + 1, total, &mut reader).await?;
                let label = c.fact.lines().next().unwrap_or("").to_string();
                let act = match outcome {
                    Some(Action::Accept) => {
                        match persist_memory(memory_store, c, item.session_owner.as_deref()) {
                            Ok(Some(path)) => {
                                eprintln!("  ✓ wrote {}", path.display());
                                memories_done += 1;
                            }
                            Ok(None) => {
                                eprintln!("  ↩ already remembered — nothing new written")
                            }
                            Err(e) => eprintln!("  ✗ failed to persist: {e:#}"),
                        }
                        Some(Action::Accept)
                    }
                    Some(Action::Reject) => {
                        eprintln!("  (rejected)");
                        Some(Action::Reject)
                    }
                    Some(Action::Defer) => {
                        eprintln!("  (kept pending)");
                        Some(Action::Defer)
                    }
                    None => {
                        eprintln!("  (stdin closed; queue kept for next session)");
                        return Ok(());
                    }
                };
                (CandidateKind::Memory, label, act)
            }
        };
        // 点头/摇头都离开队列；推迟留着。
        if matches!(action, Some(Action::Accept) | Some(Action::Reject)) {
            if let Err(e) = hermes_reflect::inbox_remove(&item.id) {
                tracing::warn!(error=%e, id=%item.id, "removing a reviewed item failed");
            }
        }
        if let Some(a) = action {
            log_action(session_id, kind, &label, map_action(a));
        }
    }

    eprintln!("pending review done — skills: {skills_done}, memories: {memories_done}");
    Ok(())
}

/// 旧的 CLI 私有队列 `deferred.jsonl`：读到就并进唯一的待审队列，然后删掉。
/// 只做一次；文件不在就是无事发生。
fn absorb_legacy_deferred_queue() {
    let root = hermes_core::data_root();
    let legacy = root.join("deferred.jsonl");
    if !legacy.exists() {
        return;
    }
    let raw = match std::fs::read_to_string(&legacy) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, "reading the legacy deferred queue failed");
            return;
        }
    };
    let mut moved = 0usize;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // 老格式（`DeferredCandidate`，内部 tag = `kind`，内容都是**平铺**的）：
        //   {"kind":"skill",  ...SkillCandidate 字段...}
        //   {"kind":"memory", ...MemoryCandidate 字段..., "session_owner":"..."}
        let payload = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(mut v) => {
                let kind = v
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string();
                let owner = v
                    .get("session_owner")
                    .and_then(|o| o.as_str())
                    .map(str::to_string);
                if let Some(obj) = v.as_object_mut() {
                    // 这两个键只属于队列外壳，不属于候选本身。
                    obj.remove("kind");
                    obj.remove("session_owner");
                }
                let parsed = if kind == "memory" {
                    serde_json::from_value::<MemoryCandidate>(v).map(InboxPayload::Memory)
                } else {
                    serde_json::from_value::<SkillCandidate>(v).map(InboxPayload::Skill)
                };
                match parsed {
                    Ok(p) => Some((p, owner)),
                    Err(e) => {
                        tracing::warn!(error=%e, "skipping an unreadable legacy deferred line");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "skipping a malformed legacy deferred line");
                None
            }
        };
        if let Some((p, owner)) = payload {
            let mark = EnqueueMark::append_only(owner);
            if let Ok(n) = hermes_reflect::enqueue_candidate(p, InboxSource::ManualReflect, mark) {
                moved += n;
            }
        }
    }
    match std::fs::remove_file(&legacy) {
        Ok(()) => eprintln!("(merged {moved} item(s) from the old deferred queue)"),
        Err(e) => tracing::warn!(error=%e, "removing the legacy deferred queue failed"),
    }
}

/// 把一条候选放进**唯一**的待审队列（P1-5：以前 CLI 另写 `deferred.jsonl`）。
///
/// 用 `append_only`：CLI 是在候选**当场**被推迟时入队，队列里已有的条目
/// （含 GUI 那边排的）不该被这一条顶掉。
fn defer_to_inbox(payload: InboxPayload, session_owner: Option<String>) {
    let mark = EnqueueMark::append_only(session_owner);
    match hermes_reflect::enqueue_candidate(payload, InboxSource::ManualReflect, mark) {
        Ok(n) if n > 0 => {}
        // 0 条 = 质量门槛没过（噪声 / 太短 / 重复）。不打扰用户，也不算失败。
        Ok(_) => eprintln!("  (deferred — 没过待审门槛，未入队)"),
        Err(e) => eprintln!("  (deferred — 写入待审队列失败: {e:#})"),
    }
}

fn map_action(a: Action) -> hermes_reflect::ActionTaken {
    match a {
        Action::Accept => hermes_reflect::ActionTaken::Accept,
        Action::Reject => hermes_reflect::ActionTaken::Reject,
        Action::Defer => hermes_reflect::ActionTaken::Defer,
    }
}

fn map_conflict_action(a: ConflictAction) -> hermes_reflect::ActionTaken {
    match a {
        ConflictAction::KeepNew => hermes_reflect::ActionTaken::Accept,
        ConflictAction::KeepOld | ConflictAction::Skip => hermes_reflect::ActionTaken::Reject,
        ConflictAction::Merge => hermes_reflect::ActionTaken::Merge,
        ConflictAction::ScopeSplit => hermes_reflect::ActionTaken::ScopeSplit,
    }
}

fn log_action(
    session_id: &str,
    kind: hermes_reflect::CandidateKind,
    label: &str,
    action: hermes_reflect::ActionTaken,
) {
    hermes_reflect::log_append(hermes_reflect::ReflectLogEntry {
        at: chrono::Utc::now(),
        session_id: session_id.to_string(),
        kind,
        action,
        label: label.to_string(),
    });
}
