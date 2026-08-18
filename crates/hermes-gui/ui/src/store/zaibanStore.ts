import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ZaibanItem = {
  id: string;
  title: string;
  status: "open" | "suggested" | "waiting" | "done" | "dropped";
  doneWhen?: string | null;
  softDue?: string | null;
  note?: string | null;
  sessionId?: string | null;
  overdue: boolean;
  dueToday?: boolean;
  dueDate?: string | null;
};

export type MergeHint = {
  keepId: string;
  keepTitle: string;
  otherId: string;
  otherTitle: string;
};

export type ZaibanList = {
  items: ZaibanItem[];
  owedCount: number;
  overdueCount?: number;
  crowded: boolean;
  recentDone?: ZaibanItem[];
  mergeHint?: MergeHint | null;
};

export type StreamCue = {
  action: string;
  id?: string | null;
  title?: string | null;
  existingId?: string | null;
  existingTitle?: string | null;
};

type StartAsk = { id: string; title: string };
type RedueAsk = { id: string; title: string };

interface ZaibanState {
  list: ZaibanList | null;
  error: boolean;
  streamCue: StreamCue | null;
  pendingStart: StartAsk | null;
  pendingRedue: RedueAsk | null;
  dismissedMerge: string | null;
  highlightId: string | null;
  refresh: () => Promise<void>;
  applyStream: (cue: StreamCue) => void;
  clearStreamCue: () => void;
  setPendingStart: (item: StartAsk | null) => void;
  setPendingRedue: (item: RedueAsk | null) => void;
  dismissMerge: (key: string) => void;
  setHighlight: (id: string | null) => void;
}

export const useZaibanStore = create<ZaibanState>((set, get) => ({
  list: null,
  error: false,
  streamCue: null,
  pendingStart: null,
  pendingRedue: null,
  dismissedMerge: null,
  highlightId: null,
  refresh: async () => {
    try {
      const next = await invoke<ZaibanList>("list_commitments");
      set({ list: next, error: false });
    } catch {
      set({ error: true });
    }
  },
  applyStream: (cue) => {
    set({ streamCue: cue, highlightId: cue.id ?? cue.existingId ?? null });
    void get().refresh();
  },
  clearStreamCue: () => set({ streamCue: null }),
  setPendingStart: (item) => set({ pendingStart: item }),
  setPendingRedue: (item) => set({ pendingRedue: item }),
  dismissMerge: (key) => set({ dismissedMerge: key }),
  setHighlight: (id) => set({ highlightId: id }),
}));

let bound = false;
export function bindZaibanListener(): void {
  if (bound) return;
  bound = true;
  void listen("hermes://zaiban-changed", () => {
    void useZaibanStore.getState().refresh();
  });
}
