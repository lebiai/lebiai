import { create } from "zustand";

export type Panel =
  | "chat"
  | "memory"
  | "skills"
  | "mcp"
  | "settings"
  | "reflect";

interface NavState {
  activePanel: Panel;
  setPanel: (panel: Panel) => void;
}

export const useNavStore = create<NavState>((set) => ({
  activePanel: "chat",
  setPanel: (panel) => set({ activePanel: panel }),
}));
