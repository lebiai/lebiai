export type ThemeMode = "system" | "light" | "dark";

export function normalizeTheme(value: string | null | undefined): ThemeMode {
  if (value === "light" || value === "dark" || value === "system") return value;
  return "system";
}

/** Apply theme by toggling `dark` on <html> (Tailwind class strategy). */
export function applyTheme(theme: ThemeMode) {
  const root = document.documentElement;
  const preferDark =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = theme === "dark" || (theme === "system" && preferDark);
  root.classList.toggle("dark", dark);
  root.dataset.theme = theme;
}
