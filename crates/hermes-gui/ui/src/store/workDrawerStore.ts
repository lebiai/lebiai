import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useZaibanStore } from "./zaibanStore";

export type WorkTab = "zaiban" | "review";

type Prefs = {
  weekday: number;
  defaultSpan: string;
  inviteDue: boolean;
  reviewed: boolean;
  from: string;
  to: string;
};

interface WorkDrawerState {
  open: boolean;
  tab: WorkTab;
  prefs: Prefs | null;
  toggle: () => Promise<void>;
  close: () => void;
  openTo: (tab: WorkTab) => void;
  setTab: (tab: WorkTab) => void;
  refreshPrefs: () => Promise<Prefs | null>;
}

export const useWorkDrawerStore = create<WorkDrawerState>((set, get) => ({
  open: false,
  tab: "zaiban",
  prefs: null,
  toggle: async () => {
    if (get().open) {
      set({ open: false });
      return;
    }
    const prefs = await get().refreshPrefs();
    await useZaibanStore.getState().refresh();
    const owed = useZaibanStore.getState().list?.owedCount ?? 0;
    const tab: WorkTab =
      owed > 0 ? "zaiban" : prefs?.inviteDue ? "review" : "zaiban";
    set({ open: true, tab });
  },
  close: () => set({ open: false }),
  openTo: (tab) => set({ open: true, tab }),
  setTab: (tab) => set({ tab }),
  refreshPrefs: async () => {
    try {
      const prefs = await invoke<Prefs>("get_review_prefs");
      set({ prefs });
      return prefs;
    } catch {
      return get().prefs;
    }
  },
}));
