import {
  useCallback,
  useLayoutEffect,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ListTodo,
  Sparkles,
  X,
} from "lucide-react";
import { dayKey, speakerNameOf, useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { isDefaultTitle } from "../../utils/sessionTitle";
import {
  coalesceMessagesForDisplay,
  hasVisibleAssistantContent,
} from "../../utils/displayMessages";
import { useThrottledValue } from "../../utils/useThrottledValue";
import { Button, ui } from "../common/ui";
import { MessageBubble } from "./MessageBubble";
import { InputArea } from "./InputArea";
import { StreamingBubble } from "./StreamingBubble";

import { WelcomeScenes } from "./WelcomeScenes";
import { TeamRoster } from "./TeamRoster";
import { PendingNod } from "./PendingNod";
import { DayFold } from "./DayFold";
import { ZaibanCue } from "../zaiban/ZaibanCue";
import { WorkDrawer } from "../work/WorkDrawer";
import { useWorkDrawerStore } from "../../store/workDrawerStore";
import { useZaibanStore } from "../../store/zaibanStore";

/** Enable windowing when the transcript is long enough to matter. */
const VIRTUAL_THRESHOLD = 28;

function messageKey(
  msg: { role: string; content: unknown[]; rawStart?: number },
  index: number
): string {
  if (typeof msg.rawStart === "number") {
    return `${msg.role}-${msg.rawStart}`;
  }
  const text = msg.content
    .map((b) => {
      if (b && typeof b === "object" && "type" in b) {
        const block = b as {
          type: string;
          text?: string;
          thinking?: string;
          id?: string;
          name?: string;
        };
        if (block.type === "text") return block.text ?? "";
        if (block.type === "thinking") return block.thinking ?? "";
        if (block.type === "toolUse") return block.id ?? block.name ?? "";
      }
      return "";
    })
    .join("|")
    .slice(0, 48);
  return `${msg.role}-${index}-${text.length}-${text.slice(0, 16)}`;
}

export function ChatView() {
  const {
    activeSessionId,
    activeReadOnly,
    sessions,
    messages,
    isStreaming,
    contextCompacted,
    lastReflection,
    clearReflection,
    regenerateLast,
    editAndResend,
    personas,
    personaId,
    teams,
    teamId,
    episode,
    fetchEpisode,
  } = useChatStore();
  const t = useUiStore((s) => s.t);
  const drawerOpen = useWorkDrawerStore((s) => s.open);
  const toggleDrawer = useWorkDrawerStore((s) => s.toggle);
  const closeDrawer = useWorkDrawerStore((s) => s.close);
  const owedCount = useZaibanStore((s) => s.list?.owedCount ?? 0);
  const overdueCount = useZaibanStore((s) => s.list?.overdueCount ?? 0);


  const parentRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  /** Keys already present when session loaded / previously rendered — no re-enter. */
  const knownMsgKeys = useRef<Set<string>>(new Set());
  const seededSessionId = useRef<string | null>(null);
  const seenDays = useRef<Set<string>>(new Set());
  /** Keys currently playing enter animation (new turns only). */
  const [enteringKeys, setEnteringKeys] = useState<Record<string, true>>({});

  const [editDraft, setEditDraft] = useState<{
    rawStart: number;
    text: string;
  } | null>(null);

  const readOnly =
    activeReadOnly || !!sessions.find((s) => s.id === activeSessionId)?.readOnly;

  /**
   * 头部只回答一件事：**你现在在跟谁干活**。
   * 会话标题不上头部——用户不建会话、不挑标题，一个工位就是一段连续的对话，
   * 把某个历史标题钉在顶上只会和「换了话题」打架。
   */
  const station = useMemo(
    () => personas.find((p) => p.id === personaId) ?? null,
    [personas, personaId]
  );

  /** 项目组会话：头部写组名，写说话人的人是消息标签（规格 §2.2）。 */
  const team = useMemo(
    () => teams.find((t) => t.id === teamId) ?? null,
    [teams, teamId]
  );

  /** 组里这一轮开口的人；别的会话为 null（不标）。 */
  const speaker = useChatStore(speakerNameOf);

  /**
   * 兜底：**没有工位**的会话（persona 落地之前的老会话、别的渠道进来的会话）
   * 退回「这是哪一段」。没有正经标题时给产品名，**不回「新对话」**——
   * 那三个字按用户要求已经从首页撤掉了。
   */
  const orphanTopic = useMemo(() => {
    const s = sessions.find((x) => x.id === activeSessionId);
    return s && !isDefaultTitle(s.title) ? s.title : null;
  }, [activeSessionId, sessions]);

  /** 更早的日子 + 已经翻开的旧账（旧账只读）。 */
  const days = useChatStore((s) => s.days);
  const expandedDays = useChatStore((s) => s.expandedDays);
  const dayLoading = useChatStore((s) => s.dayLoading);
  const dayError = useChatStore((s) => s.dayError);
  const toggleDay = useChatStore((s) => s.toggleDay);

  const displayMessages = useMemo(
    () =>
      coalesceMessagesForDisplay(messages).filter(
        (m) => m.role === "user" || hasVisibleAssistantContent(m)
      ),
    [messages]
  );

  const messageKeys = useMemo(
    () => displayMessages.map((m, i) => messageKey(m, i)),
    [displayMessages]
  );

  /** Seed history silently on session switch; animate only keys that arrive later. */
  useLayoutEffect(() => {
    if (!activeSessionId) {
      seededSessionId.current = null;
      knownMsgKeys.current = new Set();
      setEnteringKeys({});
      return;
    }
    if (seededSessionId.current !== activeSessionId) {
      seededSessionId.current = activeSessionId;
      knownMsgKeys.current = new Set(messageKeys);
      seenDays.current = new Set();
      setEnteringKeys({});
      return;
    }
    const fresh: Record<string, true> = {};
    for (const k of messageKeys) {
      if (!knownMsgKeys.current.has(k)) {
        knownMsgKeys.current.add(k);
        fresh[k] = true;
      }
    }
    if (Object.keys(fresh).length > 0) {
      setEnteringKeys((prev) => ({ ...prev, ...fresh }));
    }
  }, [activeSessionId, messageKeys]);

  useLayoutEffect(() => {
    const keys = Object.keys(expandedDays);
    const fresh = keys.find((k) => !seenDays.current.has(k));
    seenDays.current = new Set(keys);
    if (!fresh) return;
    document.getElementById(`day-${fresh}`)?.scrollIntoView({ block: "start" });
  }, [expandedDays]);

  const markEntered = useCallback((key: string) => {
    setEnteringKeys((prev) => {
      if (!prev[key]) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  const showWelcome =
    !!activeSessionId && displayMessages.length === 0 && !isStreaming;

  /** 流式时不用虚拟列表：实时块在列表外会滚丢。 */
  const useVirtual =
    displayMessages.length >= VIRTUAL_THRESHOLD && !isStreaming;

  const virtualizer = useVirtualizer({
    count: useVirtual ? displayMessages.length : 0,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 120,
    overscan: 6,
  });

  useEffect(() => {
    if (!useVirtual) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
      return;
    }
    if (displayMessages.length > 0) {
      virtualizer.scrollToIndex(displayMessages.length - 1, { align: "end" });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only on new content / stream
  }, [
    displayMessages.length,
    isStreaming,
    useVirtual,
  ]);

  const lastAssistantIdx = useMemo(() => {
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      if (displayMessages[i].role === "assistant") return i;
    }
    return -1;
  }, [displayMessages]);

  /** 人物 id → 名字。引擎在消息上盖的是 id，界面要的是名字。 */
  const speakerNames = useMemo(() => {
    const map = new Map<string, string>();
    for (const t of teams) {
      for (const m of t.members) if (!map.has(m.id)) map.set(m.id, m.name);
    }
    for (const p of personas) map.set(p.id, p.name);
    return map;
  }, [teams, personas]);

  /**
   * 每一行 assistant 该标谁。**换人了就标** —— 组会话里棒跟着产物走
   * （海燕 → 吕老师 → 小宋），不标的话后一个人说的话会被并进前一个人的气泡里
   * （用户原话：「吕老师消息被埋」）。
   *
   * 没有 `speaker` 的（人物 id 落地之前的老 transcript）这里给 null，
   * 由 `speakerForRow` 走老的兜底规则，行为与过去一致。
   */
  const speakerLabels = useMemo(() => {
    const labels: (string | null)[] = [];
    let prev: string | null = null;
    for (const m of displayMessages) {
      if (m.role !== "assistant") {
        // 用户开口 = 新的一轮：下一句该重新署名。
        prev = null;
        labels.push(null);
        continue;
      }
      const id = m.speaker ?? null;
      const changed = !!id && id !== prev;
      labels.push(changed ? speakerNames.get(id) ?? id : null);
      prev = id;
    }
    return labels;
  }, [displayMessages, speakerNames]);

  const onEditUser = useCallback((rawStart: number, currentText: string) => {
    setEditDraft({ rawStart, text: currentText });
  }, []);

  const confirmEdit = () => {
    if (!editDraft) return;
    const { rawStart, text } = editDraft;
    setEditDraft(null);
    void editAndResend(rawStart, text);
  };

  /** 今天有没有待点头的选题：换桌子 / 换会话就回来读一次。 */
  useEffect(() => {
    void fetchEpisode();
  }, [fetchEpisode, teamId, activeSessionId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && drawerOpen) {
        e.preventDefault();
        closeDrawer();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawerOpen, closeDrawer]);

  const renderMessage = (msg: (typeof displayMessages)[0], i: number) => {
    const key = messageKeys[i] ?? messageKey(msg, i);
    const enter = !!enteringKeys[key];
    // 组会话才署名（工位会话头部已经写着这是谁）；老 transcript 没盖过章，
    // 退回过去那条规则：最后一条 + 已停流。
    const rowSpeaker =
      teamId === null
        ? null
        : speakerLabels[i] ??
          (i === lastAssistantIdx && !isStreaming ? speaker : null);
    return (
      <div
        key={key}
        className={enter ? "msg-enter" : undefined}
        onAnimationEnd={(e) => {
          if (e.target === e.currentTarget && enter) markEntered(key);
        }}
      >
        <MessageBubble
          message={msg}
          speaker={rowSpeaker}
          canRegenerate={i === lastAssistantIdx && !isStreaming && !readOnly}
          onRegenerate={() => void regenerateLast()}
          onEditUser={readOnly ? undefined : onEditUser}
          isStreaming={isStreaming}
        />
      </div>
    );
  };

  const headerNode = (withWork: boolean) => (
      <header className={ui.header}>
        <div className="min-w-0 flex-1 flex items-baseline gap-2">
          <h1 className="text-sm font-semibold text-app-fg dark:text-slate-100 truncate min-w-0">
            {team
              ? team.name
              : station
                ? station.name
                : (orphanTopic ?? t("chat.header"))}
          </h1>
          {team ? (
            <span className="shrink-0 text-app-sub text-app-fg-secondary dark:text-slate-400 whitespace-nowrap">
              {team.hint}
            </span>
          ) : station ? (
            <span className="shrink-0 text-app-sub text-app-fg-secondary dark:text-slate-400 whitespace-nowrap">
              {station.role}
            </span>
          ) : null}
        </div>
        {withWork && (
        <button
          type="button"
          onClick={() => void toggleDrawer()}
          className={`shrink-0 inline-flex items-center gap-1.5 pl-2.5 pr-2 py-1 rounded-full text-xs border transition-colors ${
            drawerOpen
              ? "border-app-primary/30 bg-app-primary-soft text-app-primary dark:bg-blue-950/50 dark:text-blue-300"
              : "border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800/80 text-app-fg-secondary hover:text-app-fg hover:border-app-fg/20"
          }`}
          aria-pressed={drawerOpen}
        >
          <ListTodo size={13} strokeWidth={1.75} />
          <span>{t("zaiban.title")}</span>
          {owedCount > 0 && (
            <span
              className={`min-w-[1.15rem] h-4 px-1 rounded-full text-xs font-semibold flex items-center justify-center ${
                drawerOpen
                  ? "bg-white/20 text-white dark:bg-slate-900/20 dark:text-slate-900"
                  : overdueCount > 0
                    ? "bg-amber-600 text-white"
                    : "bg-app-primary text-white"
              }`}
            >
              {owedCount > 99 ? "99+" : owedCount}
            </span>
          )}
        </button>
        )}
      </header>
  );

  if (!activeSessionId) {
    return (
      <div className={`flex h-full min-w-0 ${ui.page}`}>
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          {headerNode(false)}
          <div className="flex-1 overflow-y-auto px-4 py-4">
            <div className="max-w-3xl mx-auto">
              <WelcomeScenes />
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`flex h-full min-w-0 ${ui.page}`}>
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
      {headerNode(true)}

      {team && <TeamRoster team={team} />}

      {team && episode && <PendingNod episode={episode} />}

      <ZaibanCue />

      {/* key forces light re-enter when switching / new chat — not a blocking loader */}
      <div
        key={activeSessionId ?? "none"}
        ref={parentRef}
        className="flex-1 overflow-y-auto px-4 py-4 session-enter"
      >
        <div className="max-w-3xl mx-auto">
          {/* 时间轴：更早的日子在上，当前窗口在下。点开某一天，气泡插在这条缝上，往上翻。 */}
          {days.map((d) => {
            const key = dayKey(d.day);
            const expanded = expandedDays[key];
            if (expanded) {
              const old = coalesceMessagesForDisplay(expanded).filter(
                (m) => m.role === "user" || hasVisibleAssistantContent(m)
              );
              return (
                <div key={key} id={`day-${key}`} className="mb-6 space-y-5">
                  <button
                    type="button"
                    onClick={() => void toggleDay(d.day)}
                    className="w-full flex items-center gap-2 py-1 text-left"
                  >
                    <span className="h-px flex-1 bg-app-border dark:bg-slate-800" />
                    <span className="text-app-sub text-app-fg-tertiary whitespace-nowrap">
                      {d.label}
                    </span>
                    <span className="h-px flex-1 bg-app-border dark:bg-slate-800" />
                  </button>
                  {old.map((m, i) => (
                    <MessageBubble
                      key={`${key}-${i}`}
                      message={m}
                      isStreaming={false}
                    />
                  ))}
                </div>
              );
            }
            return (
              <DayFold
                key={key}
                label={d.label}
                turns={d.turns}
                open={false}
                loading={dayLoading === key}
                failed={dayError === key}
                onToggle={() => void toggleDay(d.day)}
              />
            );
          })}
          {showWelcome ? (
            <WelcomeScenes hint={team ? t("team.empty") : undefined} />
          ) : useVirtual ? (
            <div
              className="relative w-full"
              style={{ height: `${virtualizer.getTotalSize()}px` }}
            >
              {virtualizer.getVirtualItems().map((vr) => {
                const msg = displayMessages[vr.index];
                const key = messageKeys[vr.index] ?? messageKey(msg, vr.index);
                return (
                  <div
                    key={key}
                    data-index={vr.index}
                    ref={virtualizer.measureElement}
                    className="absolute top-0 left-0 w-full pb-5"
                    style={{ transform: `translateY(${vr.start}px)` }}
                  >
                    {renderMessage(msg, vr.index)}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="space-y-5">
              {displayMessages.map((msg, i) => renderMessage(msg, i))}
            </div>
          )}

          {contextCompacted && (
            <div
              key="context-compacted"
              className={`${useVirtual ? "pt-4" : "mt-4"} flex justify-center`}
            >
              <span className="text-xs leading-none text-app-fg-tertiary dark:text-zinc-500 px-3 py-1 rounded-full bg-app-surface/60 dark:bg-zinc-900/40">
                {t("chat.contextCompacted")}
              </span>
            </div>
          )}
          {isStreaming && (
            <div
              key="stream-turn"
              className={`${useVirtual ? "pt-5" : "mt-5"} stream-enter`}
            >
              <LiveStream scrollRef={parentRef} />
            </div>
          )}
          <div ref={bottomRef} />
        </div>
      </div>

      {lastReflection && (
        <div className="mx-4 mb-2 flex items-center gap-2.5 px-3.5 py-3 rounded-xl bg-app-accent-soft dark:bg-violet-950/40 border border-app-accent/30 dark:border-violet-600/50 text-sm shadow-[var(--shadow-app-card)] fade-up-in ring-1 ring-app-accent/10">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-app-accent/15 dark:bg-violet-800/50 text-app-accent dark:text-violet-300">
            <Sparkles size={16} />
          </div>
          <span className="flex-1 text-violet-900 dark:text-violet-100 text-xs leading-relaxed min-w-0">
            <span className="font-medium block sm:inline">{lastReflection.summary}</span>
            {(lastReflection.memoryCount > 0 || lastReflection.skillCount > 0) && (
              <span className="text-violet-600 dark:text-violet-300 ml-0 sm:ml-1.5 block sm:inline mt-0.5 sm:mt-0">
                {t("chat.reflectionCounts", {
                  memory: lastReflection.memoryCount,
                  skill: lastReflection.skillCount,
                })}
              </span>
            )}
            {lastReflection.autoAccepted > 0 && (
              <span className="text-emerald-600 dark:text-emerald-400 ml-0 sm:ml-1.5 block sm:inline">
                {t("chat.microAutoAccepted", { count: lastReflection.autoAccepted })}
              </span>
            )}
          </span>
          {(lastReflection.memoryCount > 0 || lastReflection.skillCount > 0) && (
            <button
              type="button"
              onClick={() => {
                useNavStore.getState().openPendingReview();
                clearReflection();
              }}
              className="shrink-0 px-3 py-1.5 rounded-lg text-xs font-semibold bg-app-primary text-white"
            >
              {t("chat.goConfirm")}
            </button>
          )}
          <button
            type="button"
            onClick={clearReflection}
            className="p-1 rounded-md hover:bg-violet-100 dark:hover:bg-violet-900/50"
            aria-label={t("common.dismiss")}
          >
            <X size={12} className="text-violet-400" />
          </button>
        </div>
      )}

      <InputArea />

      {editDraft && (
        <div className={`${ui.overlay} z-50 p-4`}>
          <div
            role="dialog"
            aria-labelledby="edit-msg-title"
            className="w-full max-w-lg rounded-2xl bg-app-surface dark:bg-slate-900 border border-app-border dark:border-slate-700 shadow-xl p-4 space-y-3"
          >
            <h2
              id="edit-msg-title"
              className="text-sm font-semibold text-app-fg dark:text-slate-100"
            >
              {t("message.editTitle")}
            </h2>
            <p className="text-xs text-app-fg-secondary dark:text-slate-400">
              {t("message.editHint")}
            </p>
            <textarea
              className="w-full min-h-[120px] rounded-xl border border-app-border dark:border-slate-600 bg-app-bg dark:bg-slate-950 px-3 py-2 text-sm text-app-fg dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-app-primary/40"
              value={editDraft.text}
              onChange={(e) =>
                setEditDraft((d) => (d ? { ...d, text: e.target.value } : d))
              }
              autoFocus
            />
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setEditDraft(null)}>
                {t("common.cancel")}
              </Button>
              <Button
                onClick={confirmEdit}
                disabled={!editDraft.text.trim()}
              >
                {t("message.editSubmit")}
              </Button>
            </div>
          </div>
        </div>
      )}
      </div>
      {drawerOpen && <WorkDrawer />}
    </div>
  );
}

/**
 * 实时块只订阅流式三件套：它每来一帧就重渲染，**其余对话不该跟着重渲染**
 * （以前整条 transcript 都挂在同一个全量订阅上，每个 token 重建一遍全部气泡）。
 *
 * 跟随视口也放在这里。上面 ChatView 的效应只在「消息条数 / 流式开关」变化时滚动一次，
 * 而实时流长在虚拟列表**之后**——于是 isStreaming 刚变 true 时那一次 scrollToIndex 恰好
 * 在正文出现之前执行，把正在写的字推到视口下方；token 继续长，视口却不动。用户看到的
 * 就是「卡住 → 然后一整段跳出来」。
 *
 * 判据：只有用户本来就在底部（离底 < 160px）才跟随——他上滑去看旧内容时不要把他拽
 * 回来；节流 100ms，且**先过闸再读 scrollHeight**：读它是一次强制同步布局，不能每个
 * token 都做一次。
 */
/**
 * 实时块每渲一次都要重解析整段 markdown（实测 20k 字约 9ms）。攒到 80ms 渲一次：
 * 肉眼仍是「在长字」，主线程回落到约 11%（9ms × 12/s）。详见 `useThrottledValue`。
 *
 * 不做「只渲染尾部窗口」：实测 8k 窗口只把 9ms 压到 4.1ms，却要**藏掉用户已经看到
 * 的字**——不划算（2026-09-21 量过才定）。
 */
const LIVE_RENDER_MS = 80;

function LiveStream({
  scrollRef,
}: {
  scrollRef: RefObject<HTMLDivElement>;
}) {
  const rawText = useChatStore((s) => s.streamingText);
  const rawThinking = useChatStore((s) => s.streamingThinking);
  const toolCalls = useChatStore((s) => s.activeToolCalls);
  const speaker = useChatStore(speakerNameOf);
  const text = useThrottledValue(rawText, LIVE_RENDER_MS);
  const thinking = useThrottledValue(rawThinking, LIVE_RENDER_MS);

  const lastFollowAt = useRef(0);
  const streamingLen = text.length + thinking.length + toolCalls.length;
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const now = Date.now();
    if (now - lastFollowAt.current < 100) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight > 160) return;
    lastFollowAt.current = now;
    el.scrollTop = el.scrollHeight;
  }, [streamingLen, scrollRef]);

  return (
    <StreamingBubble
      text={text}
      thinking={thinking}
      toolCalls={toolCalls}
      speaker={speaker}
    />
  );
}
