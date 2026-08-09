import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import {
  normalizeLanguage,
  translate,
  type Language,
  type TranslationKey,
} from "../i18n";
import { applyTheme, normalizeTheme, type ThemeMode } from "../utils/theme";

interface UiState {
  language: Language;
  setLanguage: (language: string) => void;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
  /** User's display name from the onboarding seed (null = unknown). */
  displayName: string | null;
  setDisplayName: (name: string | null) => void;
  /** Short label like “DeepSeek · deepseek-v4-flash” for the sidebar footer. */
  providerLabel: string | null;
  setProviderLabel: (label: string | null) => void;
  /** One-shot fill for the chat composer (e.g. Welcome example cards). */
  composerPrefill: string | null;
  setComposerPrefill: (text: string) => void;
  clearComposerPrefill: () => void;
  theme: ThemeMode;
  setTheme: (theme: string) => void;
  /** null = not loaded yet */
  hasApiKey: boolean | null;
  setHasApiKey: (v: boolean) => void;
  /** Request full-screen onboarding (Settings replay). */
  onboardingRequestId: number;
  requestOnboarding: () => void;
  /**
   * Memory id to pulse-highlight in MemoryPanel after a successful write.
   * Cleared by the panel or after timeout — never opens Reflect.
   */
  highlightMemoryId: string | null;
  pulseMemoryHighlight: (id: string) => void;
  clearMemoryHighlight: () => void;
}

const makeTranslator =
  (language: Language) =>
  (key: TranslationKey, params?: Record<string, string | number>) =>
    translate(language, key, params);

let memoryHighlightTimer: number | null = null;

export const useUiStore = create<UiState>((set) => ({
  language: "zh-CN",
  setLanguage: (language) => {
    const normalized = normalizeLanguage(language);
    set({ language: normalized, t: makeTranslator(normalized) });
  },
  t: makeTranslator("zh-CN"),
  displayName: null,
  setDisplayName: (name) => set({ displayName: name }),
  providerLabel: null,
  setProviderLabel: (label) => set({ providerLabel: label }),
  composerPrefill: null,
  setComposerPrefill: (text) => set({ composerPrefill: text }),
  clearComposerPrefill: () => set({ composerPrefill: null }),
  theme: "system",
  setTheme: (theme) => {
    const normalized = normalizeTheme(theme);
    applyTheme(normalized);
    set({ theme: normalized });
  },
  hasApiKey: null,
  setHasApiKey: (v) => set({ hasApiKey: v }),
  onboardingRequestId: 0,
  requestOnboarding: () =>
    set((s) => ({ onboardingRequestId: s.onboardingRequestId + 1 })),
  highlightMemoryId: null,
  pulseMemoryHighlight: (id) => {
    if (memoryHighlightTimer != null) {
      window.clearTimeout(memoryHighlightTimer);
      memoryHighlightTimer = null;
    }
    set({ highlightMemoryId: id });
    memoryHighlightTimer = window.setTimeout(() => {
      set({ highlightMemoryId: null });
      memoryHighlightTimer = null;
    }, 2800);
  },
  clearMemoryHighlight: () => {
    if (memoryHighlightTimer != null) {
      window.clearTimeout(memoryHighlightTimer);
      memoryHighlightTimer = null;
    }
    set({ highlightMemoryId: null });
  },
}));

/** Refresh the “provider · model” label shown in the sidebar footer. */
export async function refreshProviderLabel() {
  try {
    const c = await invoke<{ defaultProvider: string; model: string }>("get_config");
    const t = useUiStore.getState().t;
    useUiStore
      .getState()
      .setProviderLabel(
        `${t(`provider.${c.defaultProvider}` as TranslationKey)} · ${c.model}`,
      );
  } catch {
    /* non-fatal */
  }
}

/** Re-apply theme when OS preference changes (only if theme === system). */
export function bindSystemThemeWatcher() {
  const onChange = () => {
    const { theme } = useUiStore.getState();
    if (theme === "system") applyTheme("system");
  };
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}
