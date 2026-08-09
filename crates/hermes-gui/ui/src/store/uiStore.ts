import { create } from "zustand";
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
  /** One-shot fill for the chat composer (e.g. Welcome example cards). */
  composerPrefill: string | null;
  setComposerPrefill: (text: string) => void;
  clearComposerPrefill: () => void;
  theme: ThemeMode;
  setTheme: (theme: string) => void;
  /** null = not loaded yet */
  hasApiKey: boolean | null;
  setHasApiKey: (v: boolean) => void;
  /** Session-local dismiss of the setup banner */
  setupBannerDismissed: boolean;
  dismissSetupBanner: () => void;
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
  language: "en-US",
  setLanguage: (language) => {
    const normalized = normalizeLanguage(language);
    set({ language: normalized, t: makeTranslator(normalized) });
  },
  t: makeTranslator("en-US"),
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
  setupBannerDismissed: false,
  dismissSetupBanner: () => set({ setupBannerDismissed: true }),
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
