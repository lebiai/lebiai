import { create } from "zustand";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChatStreamEvent,
  ContentBlock,
  EpisodeView,
  LoadedSessionData,
  MessageData,
  MicroReflectionEvent,
  PersonaItem,
  ReflectionResult,
  SessionDay,
  SessionEndReflectionOutcome,
  SessionSummary,
  TeamItem,
} from "../types";
import {
  MICRO_REFLECTION_EVENT,
  reflectionHasCandidates,
} from "../types";
import { useUiStore } from "./uiStore";
import { useNavStore } from "./navStore";
import { useLicenseStore } from "./licenseStore";
import { useSettingsNavStore } from "./settingsNavStore";
import {
  deriveSessionTitle,
  isDefaultTitle,
  isTrivialUserText,
} from "../utils/sessionTitle";
import { toast } from "../utils/toast";
import { useZaibanStore } from "./zaibanStore";
import { playScroll } from "../utils/ritual";
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
  /** From load_session — not only the sidebar list (which is capped). */
  activeReadOnly: boolean;
  activeChannel: string | null;
  /** 工位列表（含 enabled 计算）。 */
  personas: PersonaItem[];
  /** 当前会话/草稿所属工位；null = 没有工位（人物落地之前的老会话、别的渠道进来的会话）。 */
  personaId: string | null;
  /** 项目组列表（名册 + 缺席 + 这活跑到哪了）。 */
  teams: TeamItem[];
  /** 当前会话/草稿所属项目组；null = 不在任何项目组。与 personaId 互斥。 */
  teamId: string | null;
  /** 今天的产物 / 待点头的选题 / 这一棒在谁手上。真源是文件，不另存一份。 */
  episode: EpisodeView | null;
  messages: MessageData[];
  /**
   * 更早的日子（最新在前）。空 = 这条会话还不长，没有折叠条。
   * 真源是会话文件；这里只是窗口（`load_session` 只发最近一截，见 spec §4.4）。
   */
  days: SessionDay[];
  /** 窗口前还有多少条消息 —— 编辑重发要用它把下标换算回会话数组。 */
  baseOffset: number;
  /** 已经展开过的旧账：天的 key → 那一段消息（只读）。 */
  expandedDays: Record<string, MessageData[]>;
  /** 正在翻的那一天（条上写「正在翻…」）。 */
  dayLoading: string | null;
  /** 哪一天没翻出来（灰字 + 再试一次，不弹红）。 */
  dayError: string | null;
  streamingText: string;
  streamingThinking: string;
  /**
   * 本轮发送时较早的上下文被整理成摘要。一次性的安静提示：留在本轮回答上方，
   * 下次发送或切换会话时清掉；不落进消息列表，也不要求用户做任何决定。
   */
  contextCompacted: boolean;
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
  /** Background / review reflection after leaving a session. */
  sessionEnd: SessionEndState | null;
  /** Monotonic id so stale background jobs cannot clobber newer UI state. */
  reflectJobId: number;

  fetchSessions: () => Promise<void>;
  clearSessionsError: () => void;
  fetchPersonas: () => Promise<void>;
  fetchTeams: () => Promise<void>;
  /** 重读今天有没有待点头的选题、棒在谁手上。组会话在切换、每轮结束时都会回来读一次。 */
  fetchEpisode: () => Promise<void>;
  /** 对一份决定的答复：点头 / 要改。只改我们写的头。 */
  answerDecision: (
    path: string,
    approve: boolean,
    note?: string
  ) => Promise<void>;
  setPersonas: (ids: string[]) => Promise<void>;
  newSession: (
    personaId?: string | null,
    teamId?: string | null
  ) => Promise<void>;
  loadSession: (path: string) => Promise<void>;
  /** 翻旧账：展开/收起某一天（第一次点才去取那一段）。 */
  toggleDay: (day: string | null) => Promise<void>;
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

/** 旧账的 key：没有日期的那一段（「更早」）也要能展开。 */
export function dayKey(day: string | null): string {
  return day ?? "__earlier__";
}

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

function bindStreamChannel(
  set: SetFn,
  get: GetFn,
  sessionId: string,
): Channel<ChatStreamEvent> {
  turnStartedAt = Date.now();
  turnInputTokens = 0;
  turnOutputTokens = 0;

  /**
   * 流式文本按帧合并再进 state。模型一秒能吐几十个 token；逐个 set 会让整条对话跟着
   * 重解析几十遍 markdown（用户原话：「文字不出来，最后整段蹦出来」）。合并到一帧一次：
   * 同一帧里肉眼看不出差别，主线程少跑一大截。
   * 帧回调在窗口藏起来时不一定触发——所以每个非文本事件、以及回合收尾都**先冲一次**。
   */
  let pendingText = "";
  let pendingThinking = "";
  let flushHandle: number | null = null;

  const flush = () => {
    if (flushHandle !== null) {
      cancelAnimationFrame(flushHandle);
      flushHandle = null;
    }
    if (!pendingText && !pendingThinking) return;
    const text = pendingText;
    const thinking = pendingThinking;
    pendingText = "";
    pendingThinking = "";
    set((s) => ({
      streamingText: text ? s.streamingText + text : s.streamingText,
      streamingThinking: thinking
        ? s.streamingThinking + thinking
        : s.streamingThinking,
    }));
  };

  const scheduleFlush = () => {
    if (flushHandle !== null) return;
    flushHandle = requestAnimationFrame(() => {
      flushHandle = null;
      flush();
    });
  };

  const onEvent = new Channel<ChatStreamEvent>();
  onEvent.onmessage = (event) => {
    // Drop late events from a previous turn after the user switched session.
    if (get().activeSessionId !== sessionId) {
      if (event.event === "done" || event.event === "error" || event.event === "cancelled") {
        // Still clear streaming if this session is not active — avoid stuck flag
        // only when we still believe we are streaming for this id.
        if (get().isStreaming && get().activeSessionId === sessionId) {
          set({ isStreaming: false });
        }
      }
      return;
    }
    switch (event.event) {
      case "textDelta":
        pendingText += event.data.text;
        scheduleFlush();
        break;
      case "textCorrected":
        // 纠正文才是权威全文：还没冲出去的增量直接作废，别让它盖回去。
        pendingText = "";
        flush();
        set({ streamingText: event.data.text });
        break;
      case "thinkingDelta":
        pendingThinking += event.data.text;
        scheduleFlush();
        break;
      case "toolUseStart":
        // 工具调用开新一段之前，先把上一段话冲出去，否则顺序会倒。
        flush();
        set((s) => ({
          // The segment just finished was process narration, not the answer:
          // a tool call starts a new segment. This matches what survives the
          // turn (`mergeAssistantContent` keeps only the last text segment), so
          // the live view no longer shows a wall of text that later collapses.
          streamingText: "",
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
      case "usageUpdate":
        turnInputTokens += event.data.inputTokens;
        turnOutputTokens += event.data.outputTokens;
        set((s) => ({
          inputTokens: s.inputTokens + event.data.inputTokens,
          outputTokens: s.outputTokens + event.data.outputTokens,
        }));
        break;
      case "error": {
        flush();
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
        flush();
        set((s) => ({
          streamingText:
            s.streamingText.trim().length > 0
              ? `${s.streamingText}\n\n*${useUiStore.getState().t("chat.stopped")}*`
              : `*${useUiStore.getState().t("chat.stopped")}*`,
        }));
        toast.info(useUiStore.getState().t("toast.generationStopped"));
        break;
      case "zaibanUpdated":
        useZaibanStore.getState().applyStream(event.data);
        break;
      case "contextCompacted":
        // 安静的一行提示：本轮发送时较早的上下文被整理成了摘要。
        set({ contextCompacted: true });
        break;
      case "rememberQueued":
        toast.info(useUiStore.getState().t("materials.rememberQueued"));
        break;
      case "skillCandidateProposed":
        // Inbox only — do not open a mid-dialogue modal.
        break;
      case "done": {
        // 最后一帧可能还没冲：done 是在 run_turn 里发的，随后不会再有事件了。
        flush();
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
        set((s) => {
          const now = new Date().toISOString();
          const sessions = s.sessions.map((sess) =>
            sess.id === s.activeSessionId
              ? { ...sess, updatedAt: now }
              : sess,
          );
          return {
            messages: [...s.messages, assistantMsg],
            isStreaming: false,
            streamingText: "",
            streamingThinking: "",
            activeToolCalls: [],
            sessions,
          };
        });
        turnStartedAt = 0;
        turnInputTokens = 0;
        turnOutputTokens = 0;
        // 这一轮可能写出了一份新决定——回读待点头的选题和侧栏那一行小字。
        void get().fetchEpisode();
        void get().fetchTeams();
        break;
      }
    }
  };
  return onEvent;
}

async function doNewSession(
  set: (partial: Partial<ChatState> | ((s: ChatState) => Partial<ChatState>)) => void,
  get: () => ChatState,
  personaId: string | null = null,
  teamId: string | null = null
) {
  // Already on an empty draft for this same 工作位（人 / 项目组）— do not create another.
  const cur = get();
  if (
    cur.activeSessionId &&
    cur.messages.length === 0 &&
    cur.personaId === personaId &&
    cur.teamId === teamId
  ) {
    return;
  }

  const session = await invoke<SessionSummary>("new_session", {
    personaId,
    teamId,
  });
  set((s) => ({
    // Draft is active but NOT listed in history until it has user content.
    sessions: s.sessions.filter((x) => x.id !== session.id),
    activeSessionId: session.id,
    activeReadOnly: false,
    activeChannel: null,
    personaId: session.persona ?? personaId,
    teamId: session.team ?? teamId,
    messages: [],
    streamingText: "",
    streamingThinking: "",
    contextCompacted: false,
    activeToolCalls: [],
    inputTokens: 0,
    outputTokens: 0,
    lastReflection: null,
    microReview: null,
    microReviewOpen: false,
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
    activeReadOnly: !!data.readOnly,
    activeChannel: data.channel ?? null,
    personaId: data.persona ?? null,
    teamId: data.team ?? null,
    messages: data.messages,
    days: data.days ?? [],
    baseOffset: data.baseOffset ?? 0,
    expandedDays: {},
    dayLoading: null,
    dayError: null,
    inputTokens: data.inputTokens,
    outputTokens: data.outputTokens,
    streamingText: "",
    streamingThinking: "",
    contextCompacted: false,
    activeToolCalls: [],
    lastReflection: null,
    microReview: null,
    microReviewOpen: false,
  });
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  draftSession: null,
  sessionsLoading: false,
  sessionsError: null,
  activeSessionId: null,
  activeReadOnly: false,
  activeChannel: null,
  personas: [],
  personaId: null,
  teams: [],
  teamId: null,
  episode: null,
  messages: [],
  days: [],
  baseOffset: 0,
  expandedDays: {},
  dayLoading: null,
  dayError: null,
  streamingText: "",
  streamingThinking: "",
  contextCompacted: false,
  activeToolCalls: [],
  isStreaming: false,
  inputTokens: 0,
  outputTokens: 0,
  lastReflection: null,
  microReview: null,
  microReviewOpen: false,
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

  fetchPersonas: async () => {
    try {
      const personas = await invoke<PersonaItem[]>("list_personas");
      set({ personas });
    } catch {
      // 没有工位列表不影响对话；静默留空。
      set({ personas: [] });
    }
  },

  fetchTeams: async () => {
    try {
      const teams = await invoke<TeamItem[]>("list_teams");
      set({ teams });
    } catch {
      // 没有项目组不影响对话；静默留空。
      set({ teams: [] });
    }
  },

  fetchEpisode: async () => {
    const teamId = get().teamId;
    if (!teamId) {
      set({ episode: null });
      return;
    }
    try {
      const episode = await invoke<EpisodeView>("list_episode", { teamId });
      // 这中间用户可能已经换了桌子——晚到的答案不许盖上来。
      if (get().teamId === teamId) set({ episode });
    } catch {
      // 读不到今天的产物不影响对话；静默留空。
      if (get().teamId === teamId) set({ episode: null });
    }
  },

  answerDecision: async (path, approve, note) => {
    try {
      await invoke("answer_decision", {
        path,
        approve,
        note: note && note.trim() ? note : null,
      });
      await Promise.all([get().fetchEpisode(), get().fetchTeams()]);
      toast.success(
        useUiStore.getState().t(approve ? "decision.approved" : "decision.revising")
      );
    } catch (e) {
      toast.error(String(e));
    }
  },

  setPersonas: async (ids) => {
    try {
      const personas = await invoke<PersonaItem[]>("set_personas", { ids });
      set({ personas });
      // 谁在册变了，桌上的人也跟着变（缺席是同一份判据算出来的）。
      void get().fetchTeams();
      const cur = get().personaId;
      // 被取消勾选的工位若正开着，退回自由对话，不让用户卡在空工位上。
      if (cur && !personas.some((p) => p.id === cur && p.enabled)) {
        set({ personaId: null });
      }
      toast.success(useUiStore.getState().t("persona.saved"));
    } catch (e) {
      toast.error(String(e));
    }
  },

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

    try {
      const needed = await invoke<boolean>("session_needs_distill", {
        sessionId: leavingId,
      });
      if (!needed) return;
    } catch {
      // If the check fails, still try a distill rather than go silent forever.
    }

    const jobId = get().reflectJobId + 1;
    set({
      reflectJobId: jobId,
      sessionEnd: { status: "background", sessionId: leavingId },
    });

    // 2) Background reflection; open review when candidates exist.
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

  newSession: async (personaId, teamId) => {
    const targetPersona = personaId ?? null;
    const targetTeam = teamId ?? null;
    // Empty draft for the same 工作位: no leave/reflect — just stay.
    const cur = get();
    if (
      cur.activeSessionId &&
      cur.messages.length === 0 &&
      cur.personaId === targetPersona &&
      cur.teamId === targetTeam
    ) {
      return;
    }
    await get().runAfterSessionEnd(async () => {
      await doNewSession(set, get, targetPersona, targetTeam);
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

  toggleDay: async (day: string | null) => {
    const { activeSessionId, expandedDays, dayLoading } = get();
    const key = dayKey(day);
    if (!activeSessionId || dayLoading) return;

    // 已经展开过 → 收起来（不丢数据，再点开还是它）。
    if (expandedDays[key]) {
      const next = { ...expandedDays };
      delete next[key];
      set({ expandedDays: next, dayError: null });
      return;
    }

    set({ dayLoading: key, dayError: null });
    try {
      const messages = await invoke<MessageData[]>("load_session_day", {
        sessionId: activeSessionId,
        day,
      });
      set({
        expandedDays: { ...get().expandedDays, [key]: messages },
        dayLoading: null,
      });
    } catch (e) {
      // 读不出来就说读不出来，给一条再试一次——不弹红。
      set({ dayLoading: null, dayError: key });
    }
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
          draftSession: null,
          teamId: null,
          messages: [],
          days: [],
          baseOffset: 0,
          expandedDays: {},
          dayLoading: null,
          dayError: null,
          streamingText: "",
          streamingThinking: "",
          contextCompacted: false,
          activeToolCalls: [],
          inputTokens: 0,
          outputTokens: 0,
        };
      });
    };

    if (isActive) {
      await get().runAfterSessionEnd(remove);
      if (!get().activeSessionId) {
        await doNewSession(
          set,
          get,
          get().personaId ?? null,
          get().teamId ?? null
        );
      }
    } else {
      await remove();
    }
  },

  sendMessage: async (content: string) => {
    const { activeSessionId } = get();
    if (!activeSessionId || get().isStreaming) return;

    // License / trial lock (docs/license-ux.md).
    const lic = useLicenseStore.getState();
    if (lic.status && !lic.status.canUseMain) {
      toast.info(useUiStore.getState().t("license.toastLocked"));
      void lic.refresh();
      return;
    }

    // No API key configured → guide to Settings instead of firing a doomed
    // request. The user's input is untouched (we return before appending).
    if (useUiStore.getState().hasApiKey === false) {
      toast.info(useUiStore.getState().t("toast.apiKeyNeededSend"));
      useNavStore.getState().setPanel("settings");
      useSettingsNavStore.getState().openTo("dialogue");
      return;
    }

    const userMsg: MessageData = {
      role: "user",
      content: [{ type: "text", text: content }],
    };
    set((s) => {
      const derivedTitle = deriveSessionTitle(content);
      const canTitle =
        !isTrivialUserText(content) && !isDefaultTitle(derivedTitle);
      const title = canTitle ? derivedTitle : "新对话";
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
        contextCompacted: false,
        activeToolCalls: [],
        sessions,
        draftSession: null,
      };
    });

    const onEvent = bindStreamChannel(set, get, activeSessionId);
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
      const msg = String(err).replace(/^(config|session):\s*/i, "");
      toast.error(msg);
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
      contextCompacted: false,
      activeToolCalls: [],
    });

    const onEvent = bindStreamChannel(set, get, activeSessionId);
    try {
      await invoke("regenerate_turn", {
        sessionId: activeSessionId,
        onEvent,
      });
    } catch (e) {
      set({ isStreaming: false });
      toast.error(String(e).replace(/^(config|session):\s*/i, ""));
    }
  },

  editAndResend: async (rawStart, content) => {
    const { activeSessionId, isStreaming, messages, baseOffset } = get();
    if (!activeSessionId || isStreaming) return;
    if (rawStart < 0 || rawStart > messages.length) return;

    const trimmed = content.trim();
    if (!trimmed) return;

    try {
      // rawStart 是**窗口内**的下标；会话数组前面还折着 `baseOffset` 条。
      // 截断吃的是会话数组，所以这里必须加回去——偏移只有这一处。
      await invoke("truncate_session", {
        sessionId: activeSessionId,
        keepCount: rawStart + baseOffset,
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

}));

/**
 * 组会话里这一轮开口的人（接棒的；没交过棒就是第一棒采集）。别的会话没有「说话人」这一说——
 * 头部已经写着这是谁的工位，再标一次就是噪音。
 */
export function speakerNameOf(s: {
  teamId: string | null;
  teams: TeamItem[];
}): string | null {
  if (!s.teamId) return null;
  const team = s.teams.find((t) => t.id === s.teamId);
  if (!team) return null;
  return team.members.find((m) => m.id === team.speakerId)?.name ?? null;
}

/** Subscribe once for post-turn micro-reflection (not on the stream Channel). */
export async function bindMicroReflectionListener(): Promise<UnlistenFn> {
  return listen<MicroReflectionEvent>(MICRO_REFLECTION_EVENT, (event) => {
    useChatStore.getState().applyMicroReflection(event.payload);
  });
}
