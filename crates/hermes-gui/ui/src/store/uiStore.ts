import { create } from "zustand";
import { normalizeLanguage, translate, type Language, type TranslationKey } from "../i18n";

interface UiState {
  language: Language;
  setLanguage: (language: string) => void;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
}

const makeTranslator =
  (language: Language) =>
  (key: TranslationKey, params?: Record<string, string | number>) =>
    translate(language, key, params);

export const useUiStore = create<UiState>((set) => ({
  language: "en-US",
  setLanguage: (language) => {
    const normalized = normalizeLanguage(language);
    set({ language: normalized, t: makeTranslator(normalized) });
  },
  t: makeTranslator("en-US"),
}));
