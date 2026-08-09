import { create } from "zustand";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChatStreamEvent,
  ConfirmAction,
  ContentBlock,
  LoadedSessionData,
  MessageData,
  MicroReflectionEvent,
  PendingConfirm,
  ReflectionResult,
  SessionEndReflectionOutcome,
  SessionSummary,
} from "../types";
import {
  MICRO_REFLECTION_EVENT,
  reflectionHasCandidates,
} from "../types";
import { useUiStore } from "./uiStore";
import { useNavStore } from "./navStore";
import {
  deriveSessionTitle,
  isDefaultTitle,
  isTrivialUserText,
} from "../utils/sessionTitle";
import { toast } from "../utils/toast";
import { playScroll, playSeal } from "../utils/ritual";
import {
  notifyRemembered,
  parseSavedMemoryId,
  sealRememberedAuto,
} from "../utils/remembered";

interface ToolCall {
  id: string;
  name: string;
  /** One-line summary of what the tool is doing (toolExecStart). */
  summary?: string;
  result?: string;
  isError?: boolean;
}

/**
 * Leave-session reflection is **non-blocking**:
 * - navigation runs immediately
 * - LLM runs in the background
 * - review modal only appears when candidates exist
 */
export type SessionEndState =
  | {
      status: "background";
      /** Session being reflected (already left). */
      sessionId: string;
    }
  | {
      status: "review";
      sessionId: string;
      result: ReflectionResult;
    };

interface ChatState {
  sessions: SessionSummary[];
  /** Empty draft (active, not yet in history). Promoted on first user message. */
  draftSession: SessionSummary | null;
  sessionsLoading: boolean;
  sessionsError: string | null;
  activeSessionId: string | null;
  messages: MessageData[];
  streamingText: string;
  streamingThinking: string;
  activeToolCalls: ToolCall[];
  isStreaming: boolean;
  inputTokens: number;
  outputTokens: number;
  lastReflection: {
    summary: string;
    memoryCount: number;
    skillCount: number;
    autoAccepted: number;
  } | null;
  /** Pending micro-reflection candidates (for Review button). */
  microReview: ReflectionResult | null;
  /** Whether the in-chat micro review modal is open. */
  microReviewOpen: boolean;
  pendingConfirm: PendingConfirm | null;
  proposedSkills: { name: string; description: string; body: string; triggers: string[] }[];
  /** Background / review reflection after leaving a session. */
  sessionEnd: SessionEndState | null;
  /** Monotonic id so stale background jobs cannot clobber newer UI state. */
  reflectJobId: number;

  fetchSessions: () => Promise<void>;
  clearSessionsError: () => void;
  newSession: () => Promise<void>;
  loadSession: (path: string) => Promise<void>;
  deleteSession: (path: string) => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  /** Re-run after last human user message (drops trailing assistant/tool turns). */
  regenerateLast: () => Promise<void>;
  /**
   * Truncate raw transcript at `rawStart` (drop that user message and after),
   * then send `content` as a new user turn.
   */
  editAndResend: (rawStart: number, content: string) => Promise<void>;
  cancelStream: () => void;
  clearReflection: () => void;
  openMicroReview: () => void;
  updateMicroReview: (result: ReflectionResult | null) => void;
  dismissMicroReview: () => void;
  respondConfirm: (action: ConfirmAction, reason?: string) => Promise<void>;
  acceptProposedSkill: (name: string) => Promise<void>;
  dismissProposedSkill: (name: string) => void;
  /**
   * Leave immediately, then run session-end reflection in the background.
   * Does **not** wait for the LLM (fixes multi-second hang on New Chat).
   */
  runAfterSessionEnd: (action: () => Promise<void>) => Promise<void>;
  updateSessionEndResult: (result: ReflectionResult | null) => void;
  completeSessionEnd: () => Promise<void>;
  dismissSessionEnd: () => void;
  /** Apply a micro-reflection event (from global Tauri listen). */
  applyMicroReflection: (ev: MicroReflectionEvent) => void;
}

/** Module-level turn clock / usage for the in-flight stream. */
let turnStartedAt = 0;
let turnInputTokens = 0;
let turnOutputTokens = 0;

function indexAfterLastHumanUser(messages: MessageData[]): number | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m.role !== "user") continue;
    const text = m.content
      .filter((b) => b.type === "text")
      .map((b) => (b.type === "text" ? b.text : ""))
      .join("\n")
      .trim();
    if (text.length > 0) return i + 1;
  }
  return null;
}

type SetFn = (
  partial: Partial<ChatState> | ((s: ChatState) => Partial<ChatState>)
) => void;
type GetFn = () => ChatState;

function bindStreamChannel(set: SetFn, get: GetFn): Channel<ChatStreamEvent> {
  turnStartedAt = Date.now();
  turnInputTokens = 0;
  turnOutputTokens = 0;

  const onEvent = new Channel<ChatStreamEvent>();
  onEvent.onmessage = (event) => {
    switch (event.event) {
      case "textDelta":
        set((s) => ({ streamingText: s.streamingText + event.data.text }));
        break;
      case "thinkingDelta":
        set((s) => ({
          streamingThinking: s.streamingThinking + event.data.text,
        }));
        break;
      case "toolUseStart":
        set((s) => ({
          activeToolCalls: [
            ...s.activeToolCalls,
            { id: event.data.id, name: event.data.name },
          ],
        }));
        break;
      case "toolExecStart":
        set((s) => ({
          activeToolCalls: s.activeToolCalls.map((tc) =>
            tc.id === event.data.id
              ? { ...tc, summary: event.data.summary }
              : tc
          ),
        }));
        break;
      case "toolUseResult": {
        const tool = get().activeToolCalls.find((tc) => tc.id === event.data.id);
        set((s) => ({
          activeToolCalls: s.activeToolCalls.map((tc) =>
            tc.id === event.data.id
              ? {
                  ...tc,
                  result: event.data.content,
                  isError: event.data.isError,
                }
              : tc
          ),
        }));
        // Natural “remembered” path — seal + toast; no Reflect CTA.
        if (
          tool?.name === "memory_save" &&
          !event.data.isError &&
          event.data.content
        ) {
          notifyRemembered(parseSavedMemoryId(event.data.content));
        }
        break;
      }
      case "confirmRequired":
        set({
          pendingConfirm: {
            id: event.data.id,
            toolName: event.data.toolName,
            summary: event.data.summary,
            reason: event.data.reason,
          },
        });
        break;
      case "usageUpdate":
        turnInputTokens += event.data.inputTokens;
        turnOutputTokens += event.data.outputTokens;
        set((s) => ({
          inputTokens: s.inputTokens + event.data.inputTokens,
          outputTokens: s.outputTokens + event.data.outputTokens,
        }));
        break;
      case "error": {
        const msg = event.data.message ?? "";
        if (msg === "cancelled" || msg.toLowerCase().includes("cancelled")) {
          set((s) => ({
            streamingText:
              s.streamingText.trim().length > 0
                ? `${s.streamingText}\n\n*${useUiStore.getState().t("chat.stopped")}*`
                : `*${useUiStore.getState().t("chat.stopped")}*`,
          }));
          toast.info(useUiStore.getState().t("toast.generationStopped"));
          break;
        }
        set((s) => ({
          streamingText:
            s.streamingText +
            `\n\n**${useUiStore.getState().t("chat.errorPrefix")}:** ${msg}`,
        }));
        break;
      }
      case "cancelled":
        set((s) => ({
          streamingText:
            s.streamingText.trim().length > 0
              ? `${s.streamingText}\n\n*${useUiStore.getState().t("chat.stopped")}*`
              : `*${useUiStore.getState().t("chat.stopped")}*`,
        }));
        toast.info(useUiStore.getState().t("toast.generationStopped"));
        break;
      case "skillCandidateProposed":
        set((s) => {
          if (s.proposedSkills.some((p) => p.name === event.data.name)) {
            return {};
          }
          return { proposedSkills: [...s.proposedSkills, event.data] };
        });
        break;
      case "done": {
        const state = get();
        const blocks: ContentBlock[] = [];
        if (state.streamingThinking) {
          blocks.push({ type: "thinking", thinking: state.streamingThinking });
        }
        for (const tc of state.activeToolCalls) {
          blocks.push({
            type: "toolUse",
            id: tc.id,
            name: tc.name,
            input: {},
          });
          if (tc.result !== undefined) {
            blocks.push({
              type: "toolResult",
              toolUseId: tc.id,
              content: tc.result,
              isError: tc.isError ?? false,
            });
          }
        }
        if (state.streamingText) {
          blocks.push({ type: "text", text: state.streamingText });
        }
        const durationMs =
          turnStartedAt > 0 ? Math.max(0, Date.now() - turnStartedAt) : undefined;
        const assistantMsg: MessageData = {
          role: "assistant",
          content: blocks,
          durationMs,
          inputTokens: turnInputTokens > 0 ? turnInputTokens : undefined,
          outputTokens: turnOutputTokens > 0 ? turnOutputTokens : undefined,
        };
        set((s) => ({
          messages: [...s.messages, assistantMsg],
          isStreaming: false,
          streamingText: "",
          streamingThinking: "",
          activeToolCalls: [],
          pendingConfirm: null,
        }));
        turnStartedAt = 0;
        turnInputTokens = 0;
        turnOutputTokens = 0;
        break;
      }
    }
  };
  return onEvent;
}

async function doNewSession(
  set: (partial: Partial<ChatState> | ((s: ChatState) => Partial<ChatState>)) => void,
  get: () => ChatState
) {
  // Already on an empty draft — do not create another "session".
  const cur = get();
  if (cur.activeSessionId && cur.messages.length === 0) {
    return;
  }

  const session = await invoke<SessionSummary>("new_session");
  set((s) => ({
    // Draft is active but NOT listed in history until it has user content.
    sessions: s.sessions.filter((x) => x.id !== session.id),
    activeSessionId: session.id,
    messages: [],
    streamingText: "",
    streamingThinking: "",
    activeToolCalls: [],
    inputTokens: 0,
    outputTokens: 0,
    // Keep path for promoting into the list after first message.
    draftSession: session,
  }));
}

async function doLoadSession(
  path: string,
  set: (partial: Partial<ChatState> | ((s: ChatState) => Partial<ChatState>)) => void
) {
  const data = await invoke<LoadedSessionData>("load_session", { path });
  set({
    activeSessionId: data.id,
    messages: data.messages,
    inputTokens: data.inputTokens,
    outputTokens: data.outputTokens,
    streamingText: "",
    streamingThinking: "",
    activeToolCalls: [],
  });
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  draftSession: null,
  sessionsLoading: false,
  sessionsError: null,
  activeSessionId: null,
  messages: [],
  streamingText: "",
  streamingThinking: "",
  activeToolCalls: [],
  isStreaming: false,
  inputTokens: 0,
  outputTokens: 0,
  lastReflection: null,
  microReview: null,
  microReviewOpen: false,
  pendingConfirm: null,
  proposedSkills: [],
  sessionEnd: null,
  reflectJobId: 0,

  fetchSessions: async () => {
    set({ sessionsLoading: true, sessionsError: null });
    try {
      const sessions = await invoke<SessionSummary[]>("list_sessions");
      set({ sessions, sessionsLoading: false, sessionsError: null });
    } catch (e) {
      set({
        sessionsLoading: false,
        sessionsError: String(e),
      });
    }
  },

  clearSessionsError: () => set({ sessionsError: null }),

  runAfterSessionEnd: async (action) => {
    const { activeSessionId, isStreaming, messages } = get();
    // Allow leave even while a previous background reflect is running.
    if (isStreaming) {
      toast.info(useUiStore.getState().t("toast.streamingBusy"));
      return;
    }

    const leavingId = activeSessionId;
    // Empty draft: nothing to reflect, no disk history.
    const hadContent = messages.length > 0;

    // 1) Leave first — never wait on the LLM for New Chat / switch.
    await action();

    if (!leavingId || !hadContent) return;

    const jobId = get().reflectJobId + 1;
    set({
      reflectJobId: jobId,
      sessionEnd: { status: "background", sessionId: leavingId },
    });

    // 2) Background reflection; open review when candidates exist (backend seeds
    //    a work episode on timeout/empty so we rarely die silent).
    void (async () => {
      const t = useUiStore.getState().t;
      try {
        const outcome = await invoke<SessionEndReflectionOutcome>("run_session_end_reflection", {
          sessionId: leavingId,
        });
        if (get().reflectJobId !== jobId) return; // superseded

        if (outcome.status === "skipped") {
          set({ sessionEnd: null });
          // Quiet: no toast for below min_turns (avoid noise).
          return;
        }
        // Default quiet path: inbox only, no modal.
        if (outcome.status === "enqueued") {
          set({ sessionEnd: null });
          if (outcome.added > 0) {
            toast.info(
              t("toast.inboxAdded")
                .replace("{n}", String(outcome.added))
                .replace("{total}", String(outcome.total))
            );
            window.dispatchEvent(new CustomEvent("hermes:inbox-changed"));
          }
          return;
        }
        // Legacy modal when pop_inbox_on_leave is enabled.
        const reflection = outcome.reflection;
        if (!reflection || !reflectionHasCandidates(reflection)) {
          set({ sessionEnd: null });
          return;
        }
        set({
          sessionEnd: {
            status: "review",
            sessionId: leavingId,
            result: reflection,
          },
        });
      } catch (e) {
        console.warn("session-end reflection failed", e);
        if (get().reflectJobId === jobId) {
          set({ sessionEnd: null });
          // Quiet fail: no blocking toast spam; log only.
        }
      }
    })();
  },

  updateSessionEndResult: (result) => {
    set((s) => {
      if (!s.sessionEnd || s.sessionEnd.status !== "review") return {};
      if (result === null) {
        return { sessionEnd: null };
      }
      return {
        sessionEnd: { ...s.sessionEnd, result },
      };
    });
  },

  completeSessionEnd: async () => {
    const end = get().sessionEnd;
    set({ sessionEnd: null });
    if (end?.status === "review") {
      const t = useUiStore.getState().t;
      playScroll(t("ritual.scrollClosed"));
      toast.info(t("ritual.scrollToast"));
    }
  },

  dismissSessionEnd: () => {
    // Bump job id so an in-flight background call cannot re-open the modal.
    set((s) => ({ sessionEnd: null, reflectJobId: s.reflectJobId + 1 }));
  },

  newSession: async () => {
    // Empty draft: no leave/reflect — just stay.
    if (get().activeSessionId && get().messages.length === 0) {
      return;
    }
    await get().runAfterSessionEnd(async () => {
      await doNewSession(set, get);
    });
  },

  loadSession: async (path: string) => {
    const { sessions, activeSessionId } = get();
    const target = sessions.find((s) => s.path === path);
    if (target && target.id === activeSessionId) return;

    await get().runAfterSessionEnd(async () => {
      await doLoadSession(path, set);
      set({ draftSession: null });
    });
  },

  deleteSession: async (path: string) => {
    const { sessions, activeSessionId } = get();
    const target = sessions.find((s) => s.path === path);
    const isActive = !!(target && target.id === activeSessionId);

    const remove = async () => {
      await invoke("delete_session", { path });
      set((s) => {
        const nextSessions = s.sessions.filter((sess) => sess.path !== path);
        if (!isActive) {
          return { sessions: nextSessions };
        }
        return {
          sessions: nextSessions,
          activeSessionId: null,
          messages: [],
          streamingText: "",
          streamingThinking: "",
          activeToolCalls: [],
          inputTokens: 0,
          outputTokens: 0,
        };
      });
    };

    if (isActive) {
      await get().runAfterSessionEnd(remove);
    } else {
      await remove();
    }
  },

  sendMessage: async (content: string) => {
    const { activeSessionId } = get();
    if (!activeSessionId || get().isStreaming) return;

    // No API key configured → guide to Settings instead of firing a doomed
    // request. The user's input is untouched (we return before appending).
    if (useUiStore.getState().hasApiKey === false) {
      toast.info(useUiStore.getState().t("toast.apiKeyNeededSend"));
      useNavStore.getState().setPanel("settings");
      return;
    }

    const userMsg: MessageData = {
      role: "user",
      content: [{ type: "text", text: content }],
    };
    set((s) => {
      const derivedTitle = deriveSessionTitle(content);
      const canTitle =
        !isTrivialUserText(content) && derivedTitle !== "New Chat";
      const title = canTitle ? derivedTitle : "New Chat";
      let sessions = s.sessions;
      const inList = sessions.some((x) => x.id === activeSessionId);
      if (!inList && activeSessionId) {
        const base =
          s.draftSession?.id === activeSessionId
            ? s.draftSession
            : {
                id: activeSessionId,
                title,
                createdAt: new Date().toISOString(),
                path: s.draftSession?.path ?? "",
              };
        sessions = [
          { ...base, title },
          ...sessions.filter((x) => x.id !== activeSessionId),
        ];
      } else if (canTitle) {
        sessions = sessions.map((sess) =>
          sess.id === activeSessionId &&
          (isDefaultTitle(sess.title) || isTrivialUserText(sess.title))
            ? { ...sess, title: derivedTitle }
            : sess
        );
      }
      return {
        messages: [...s.messages, userMsg],
        isStreaming: true,
        streamingText: "",
        streamingThinking: "",
        activeToolCalls: [],
        sessions,
        draftSession: null,
      };
    });

    const onEvent = bindStreamChannel(set, get);
    try {
      await invoke("send_message", {
        sessionId: activeSessionId,
        content,
        onEvent,
      });
    } catch (err) {
      // A rejected invoke (e.g. missing API key via direct call, bad state)
      // must not leave the UI stuck in "generating".
      set({ isStreaming: false });
      toast.error(String(err));
    }
  },

  regenerateLast: async () => {
    const { activeSessionId, isStreaming, messages } = get();
    if (!activeSessionId || isStreaming) return;

    const keep = indexAfterLastHumanUser(messages);
    if (keep === null) {
      toast.error(useUiStore.getState().t("message.regenerateEmpty"));
      return;
    }

    try {
      await invoke("truncate_after_last_user", { sessionId: activeSessionId });
    } catch (e) {
      toast.error(String(e));
      return;
    }

    set({
      messages: messages.slice(0, keep),
      isStreaming: true,
      streamingText: "",
      streamingThinking: "",
      activeToolCalls: [],
      pendingConfirm: null,
    });

    const onEvent = bindStreamChannel(set, get);
    try {
      await invoke("regenerate_turn", {
        sessionId: activeSessionId,
        onEvent,
      });
    } catch (e) {
      set({ isStreaming: false });
      toast.error(String(e));
    }
  },

  editAndResend: async (rawStart, content) => {
    const { activeSessionId, isStreaming, messages } = get();
    if (!activeSessionId || isStreaming) return;
    if (rawStart < 0 || rawStart > messages.length) return;

    const trimmed = content.trim();
    if (!trimmed) return;

    try {
      await invoke("truncate_session", {
        sessionId: activeSessionId,
        keepCount: rawStart,
      });
    } catch (e) {
      toast.error(String(e));
      return;
    }

    set({
      messages: messages.slice(0, rawStart),
    });

    await get().sendMessage(content);
  },

  cancelStream: () => {
    const { activeSessionId } = get();
    if (activeSessionId) {
      invoke("cancel_stream", { sessionId: activeSessionId });
    }
    set({ pendingConfirm: null });
  },

  clearReflection: () => set({ lastReflection: null }),
  openMicroReview: () => {
    const mr = get().microReview;
    if (mr && reflectionHasCandidates(mr)) {
      set({ microReviewOpen: true });
    }
  },
  updateMicroReview: (result) =>
    set({
      microReview: result,
      ...(result === null || !reflectionHasCandidates(result)
        ? { microReviewOpen: false, lastReflection: null }
        : {}),
    }),
  dismissMicroReview: () => set({ microReviewOpen: false }),

  applyMicroReflection: (ev) => {
    const active = get().activeSessionId;
    // Only surface UI for the session the user is looking at.
    if (active && ev.sessionId !== active) {
      return;
    }
    const raw = ev.reflection;
    const pending: ReflectionResult | null = raw
      ? {
          summary: raw.summary,
          skillCandidates: raw.skillCandidates ?? [],
          memoryCandidates: raw.memoryCandidates ?? [],
          conflicts: raw.conflicts ?? [],
        }
      : null;
    const hasPending = !!(pending && reflectionHasCandidates(pending));
    set({
      lastReflection: {
        summary: ev.summary,
        memoryCount: ev.memoryCount,
        skillCount: ev.skillCount,
        autoAccepted: ev.autoAccepted ?? 0,
      },
      ...(hasPending ? { microReview: pending } : {}),
    });
    if ((ev.autoAccepted ?? 0) > 0) {
      sealRememberedAuto(ev.autoAccepted ?? 0);
      toast.success(
        useUiStore.getState().t("toast.microAutoAccepted", {
          count: ev.autoAccepted,
        })
      );
    } else if (hasPending) {
      toast.info(useUiStore.getState().t("toast.microPending"));
    }
  },

  respondConfirm: async (action, reason) => {
    const pending = get().pendingConfirm;
    if (!pending) return;
    set({ pendingConfirm: null });
    await invoke("respond_confirm", {
      id: pending.id,
      action,
      toolName: pending.toolName,
      reason: reason && reason.trim() ? reason : null,
    });
  },

  acceptProposedSkill: async (name) => {
    const candidate = get().proposedSkills.find((p) => p.name === name);
    if (!candidate) return;
    const t = useUiStore.getState().t;
    try {
      await invoke("accept_skill_candidate", {
        name: candidate.name,
        description: candidate.description,
        triggers: candidate.triggers,
        body: candidate.body,
      });
      set((s) => ({ proposedSkills: s.proposedSkills.filter((p) => p.name !== name) }));
      playSeal(t("ritual.sealSkill"));
      toast.success(t("toast.skillAccepted"));
    } catch (e) {
      toast.error(String(e));
    }
  },

  dismissProposedSkill: (name) => {
    set((s) => ({ proposedSkills: s.proposedSkills.filter((p) => p.name !== name) }));
    toast.info(useUiStore.getState().t("toast.skillRejected"));
  },
}));

/** Subscribe once for post-turn micro-reflection (not on the stream Channel). */
export async function bindMicroReflectionListener(): Promise<UnlistenFn> {
  return listen<MicroReflectionEvent>(MICRO_REFLECTION_EVENT, (event) => {
    useChatStore.getState().applyMicroReflection(event.payload);
  });
}
