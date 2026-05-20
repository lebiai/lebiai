import { create } from "zustand";
import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ChatStreamEvent,
  ConfirmAction,
  ContentBlock,
  LoadedSessionData,
  MessageData,
  PendingConfirm,
  SessionSummary,
} from "../types";

interface ToolCall {
  id: string;
  name: string;
  result?: string;
  isError?: boolean;
}

interface ChatState {
  sessions: SessionSummary[];
  activeSessionId: string | null;
  messages: MessageData[];
  streamingText: string;
  streamingThinking: string;
  activeToolCalls: ToolCall[];
  isStreaming: boolean;
  inputTokens: number;
  outputTokens: number;
  lastReflection: { summary: string; memoryCount: number; skillCount: number } | null;
  pendingConfirm: PendingConfirm | null;

  fetchSessions: () => Promise<void>;
  newSession: () => Promise<void>;
  loadSession: (path: string) => Promise<void>;
  deleteSession: (path: string) => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  cancelStream: () => void;
  clearReflection: () => void;
  respondConfirm: (action: ConfirmAction, reason?: string) => Promise<void>;
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  messages: [],
  streamingText: "",
  streamingThinking: "",
  activeToolCalls: [],
  isStreaming: false,
  inputTokens: 0,
  outputTokens: 0,
  lastReflection: null,
  pendingConfirm: null,

  fetchSessions: async () => {
    const sessions = await invoke<SessionSummary[]>("list_sessions");
    set({ sessions });
  },

  newSession: async () => {
    const session = await invoke<SessionSummary>("new_session");
    set((s) => ({
      sessions: [session, ...s.sessions],
      activeSessionId: session.id,
      messages: [],
      streamingText: "",
      streamingThinking: "",
      activeToolCalls: [],
      inputTokens: 0,
      outputTokens: 0,
    }));
  },

  loadSession: async (path: string) => {
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
  },

  deleteSession: async (path: string) => {
    await invoke("delete_session", { path });
    set((s) => ({
      sessions: s.sessions.filter((sess) => sess.path !== path),
    }));
  },

  sendMessage: async (content: string) => {
    const { activeSessionId } = get();
    if (!activeSessionId) return;

    const userMsg: MessageData = {
      role: "user",
      content: [{ type: "text", text: content }],
    };
    set((s) => {
      const isFirstTurn = s.messages.length === 0;
      const chars = Array.from(content);
      const derivedTitle =
        chars.length > 60 ? chars.slice(0, 57).join("") + "..." : content;
      return {
        messages: [...s.messages, userMsg],
        isStreaming: true,
        streamingText: "",
        streamingThinking: "",
        activeToolCalls: [],
        sessions: isFirstTurn
          ? s.sessions.map((sess) =>
              sess.id === activeSessionId && sess.title === "New Chat"
                ? { ...sess, title: derivedTitle }
                : sess
            )
          : s.sessions,
      };
    });

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
        case "toolUseResult":
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
          break;
        case "confirmRequired":
          set({
            pendingConfirm: {
              id: event.data.id,
              toolName: event.data.toolName,
              summary: event.data.summary,
            },
          });
          break;
        case "usageUpdate":
          set((s) => ({
            inputTokens: s.inputTokens + event.data.inputTokens,
            outputTokens: s.outputTokens + event.data.outputTokens,
          }));
          break;
        case "error":
          set((s) => ({
            streamingText: s.streamingText + `\n\n**Error:** ${event.data.message}`,
          }));
          break;
        case "microReflection":
          set({ lastReflection: event.data });
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
          const assistantMsg: MessageData = {
            role: "assistant",
            content: blocks,
          };
          set((s) => ({
            messages: [...s.messages, assistantMsg],
            isStreaming: false,
            streamingText: "",
            streamingThinking: "",
            activeToolCalls: [],
            pendingConfirm: null,
          }));
          break;
        }
      }
    };

    await invoke("send_message", {
      sessionId: activeSessionId,
      content,
      onEvent,
    });
  },

  cancelStream: () => {
    const { activeSessionId } = get();
    if (activeSessionId) {
      invoke("cancel_stream", { sessionId: activeSessionId });
    }
    set({ pendingConfirm: null });
  },

  clearReflection: () => set({ lastReflection: null }),

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
}));
