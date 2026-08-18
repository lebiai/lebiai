import { create } from "zustand";

/** Settings IA tabs — docs/settings-ia.md */
export type SettingsTab =
  | "overview"
  | "dialogue"
  | "appearance"
  | "connections"
  | "more";

interface SettingsNavState {
  tab: SettingsTab;
  /** Bumps when external code wants Settings to switch tab / focus a block. */
  navRequestId: number;
  /** Optional anchor inside a tab (e.g. license). */
  focus: "license" | null;
  setTab: (tab: SettingsTab) => void;
  /** Open settings to a tab (call after setPanel("settings")). */
  openTo: (tab: SettingsTab, focus?: "license" | null) => void;
  clearFocus: () => void;
}

export const useSettingsNavStore = create<SettingsNavState>((set) => ({
  tab: "overview",
  navRequestId: 0,
  focus: null,
  setTab: (tab) => set({ tab, focus: null }),
  openTo: (tab, focus = null) =>
    set((s) => ({
      tab,
      focus,
      navRequestId: s.navRequestId + 1,
    })),
  clearFocus: () => set({ focus: null }),
}));
