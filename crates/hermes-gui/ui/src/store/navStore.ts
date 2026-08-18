import { create } from "zustand";

export type Panel = "chat" | "know" | "settings";
export type KnowTab = "you" | "ways";

interface NavState {
  activePanel: Panel;
  knowTab: KnowTab;
  /** Bumped when the user follows the pending-review badge. */
  pendingFocusSeq: number;
  setPanel: (panel: Panel) => void;
  setKnowTab: (tab: KnowTab) => void;
  /** Open Continuity / Evolve. Default tab is “what it knows about you”. */
  openKnow: (tab?: KnowTab) => void;
  /** Open Know → 它懂你 → 待审. */
  openPendingReview: () => void;
}

export const useNavStore = create<NavState>((set) => ({
  activePanel: "chat",
  knowTab: "you",
  pendingFocusSeq: 0,
  setPanel: (panel) => set({ activePanel: panel }),
  setKnowTab: (tab) => set({ knowTab: tab, activePanel: "know" }),
  openKnow: (tab) =>
    set({
      activePanel: "know",
      knowTab: tab ?? "you",
    }),
  openPendingReview: () =>
    set((s) => ({
      activePanel: "know",
      knowTab: "you",
      pendingFocusSeq: s.pendingFocusSeq + 1,
    })),
}));
