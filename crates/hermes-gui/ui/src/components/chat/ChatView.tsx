import {
  useCallback,
  useLayoutEffect,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Sparkles, X, ListTodo } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { isDefaultTitle } from "../../utils/sessionTitle";
import {
  coalesceMessagesForDisplay,
  hasVisibleAssistantContent,
} from "../../utils/displayMessages";
import { Button, ui } from "../common/ui";
import { MessageBubble } from "./MessageBubble";
import { InputArea } from "./InputArea";
import { StreamingBubble } from "./StreamingBubble";
import { ConfirmModal } from "./ConfirmModal";
import { ProposedSkillModal } from "./ProposedSkillModal";
import { WelcomeScenes } from "./WelcomeScenes";
import { MicroReviewModal } from "../reflect/MicroReviewModal";
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
    streamingText,
    streamingThinking,
    activeToolCalls,
    lastReflection,
    clearReflection,
    openMicroReview,
    microReview,
    regenerateLast,
    editAndResend,
  } = useChatStore();
  const t = useUiStore((s) => s.t);
  const setComposerPrefill = useUiStore((s) => s.setComposerPrefill);
  const drawerOpen = useWorkDrawerStore((s) => s.open);
  const toggleDrawer = useWorkDrawerStore((s) => s.toggle);
  const closeDrawer = useWorkDrawerStore((s) => s.close);
  const owedCount = useZaibanStore((s) => s.list?.owedCount ?? 0);
  const overdueCount = useZaibanStore((s) => s.list?.overdueCount ?? 0);
  const pendingConfirm = useChatStore((s) => s.pendingConfirm);

  const parentRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  /** Keys already present when session loaded / previously rendered — no re-enter. */
  const knownMsgKeys = useRef<Set<string>>(new Set());
  const seededSessionId = useRef<string | null>(null);
  /** Keys currently playing enter animation (new turns only). */
  const [enteringKeys, setEnteringKeys] = useState<Record<string, true>>({});

  const [editDraft, setEditDraft] = useState<{
    rawStart: number;
    text: string;
  } | null>(null);

  const readOnly =
    activeReadOnly || !!sessions.find((s) => s.id === activeSessionId)?.readOnly;

  const sessionTitle = useMemo(() => {
    if (!activeSessionId) return t("chat.header");
    const s = sessions.find((x) => x.id === activeSessionId);
    if (!s) return t("chat.defaultTitle");
    return isDefaultTitle(s.title) ? t("chat.defaultTitle") : s.title;
  }, [activeSessionId, sessions, t]);

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

  const useVirtual = displayMessages.length >= VIRTUAL_THRESHOLD;

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
    streamingText,
    streamingThinking,
    activeToolCalls.length,
    useVirtual,
  ]);

  const handlePickPrompt = (prompt: string) => {
    setComposerPrefill(prompt);
  };

  const lastAssistantIdx = useMemo(() => {
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      if (displayMessages[i].role === "assistant") return i;
    }
    return -1;
  }, [displayMessages]);

  const onEditUser = useCallback((rawStart: number, currentText: string) => {
    setEditDraft({ rawStart, text: currentText });
  }, []);

  const confirmEdit = () => {
    if (!editDraft) return;
    const { rawStart, text } = editDraft;
    setEditDraft(null);
    void editAndResend(rawStart, text);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && drawerOpen && !pendingConfirm) {
        e.preventDefault();
        closeDrawer();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawerOpen, closeDrawer, pendingConfirm]);

  if (!activeSessionId) {
    return (
      <div className={`flex flex-col h-full ${ui.page}`}>
        <div className="flex-1 flex items-center justify-center">
          <p className="text-sm text-app-fg-tertiary">{t("chat.opening")}</p>
        </div>
      </div>
    );
  }

  const renderMessage = (msg: (typeof displayMessages)[0], i: number) => {
    const key = messageKeys[i] ?? messageKey(msg, i);
    const enter = !!enteringKeys[key];
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
          canRegenerate={i === lastAssistantIdx && !isStreaming && !readOnly}
          onRegenerate={() => void regenerateLast()}
          onEditUser={readOnly ? undefined : onEditUser}
          isStreaming={isStreaming}
        />
      </div>
    );
  };

  return (
    <div className={`flex h-full min-w-0 ${ui.page}`}>
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
      <header className={ui.header}>
        <div className="min-w-0 flex-1 flex items-baseline gap-2">
          <h1 className="text-sm font-semibold text-app-fg dark:text-slate-100 truncate min-w-0">
            {sessionTitle}
          </h1>
          <span className="shrink-0 text-[11px] text-app-fg-tertiary dark:text-slate-500 whitespace-nowrap">
            {t("chat.headerSubShort")}
          </span>
        </div>
        <button
          type="button"
          onClick={() => void toggleDrawer()}
          className={`shrink-0 inline-flex items-center gap-1.5 pl-2.5 pr-2 py-1 rounded-full text-[12px] border transition-colors ${
            drawerOpen
              ? "border-app-fg/20 bg-app-fg text-white dark:bg-slate-100 dark:text-slate-900"
              : "border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800/80 text-app-fg-secondary hover:text-app-fg hover:border-app-fg/20"
          }`}
          aria-pressed={drawerOpen}
        >
          <ListTodo size={13} strokeWidth={1.75} />
          <span>{t("zaiban.title")}</span>
          {owedCount > 0 && (
            <span
              className={`min-w-[1.15rem] h-4 px-1 rounded-full text-[10px] font-semibold flex items-center justify-center ${
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
      </header>

      <ZaibanCue />

      {/* key forces light re-enter when switching / new chat — not a blocking loader */}
      <div
        key={activeSessionId ?? "none"}
        ref={parentRef}
        className="flex-1 overflow-y-auto px-4 py-4 session-enter"
      >
        <div className="max-w-3xl mx-auto">
          {showWelcome ? (
            <WelcomeScenes onPick={handlePickPrompt} disabled={isStreaming} />
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

          {isStreaming && (
            <div
              key="stream-turn"
              className={`${useVirtual ? "pt-5" : "mt-5"} stream-enter`}
            >
              <StreamingBubble
                text={streamingText}
                thinking={streamingThinking}
                toolCalls={activeToolCalls}
              />
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
          {microReview &&
            (lastReflection.memoryCount > 0 || lastReflection.skillCount > 0) && (
              <button
                type="button"
                onClick={() => openMicroReview()}
                className="shrink-0 px-3 py-1.5 rounded-lg text-xs font-semibold bg-app-accent text-white hover:bg-violet-700 shadow-sm motion-safe-only"
                style={{ animation: "ambient-breathe 2.2s ease-in-out 2" }}
              >
                {t("chat.microReview")}
              </button>
            )}
          {(lastReflection.memoryCount > 0 || lastReflection.skillCount > 0) && (
            <button
              type="button"
              onClick={() => {
                useNavStore.getState().openKnow("you");
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
      <ConfirmModal />
      <ProposedSkillModal />
      <MicroReviewModal />

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
