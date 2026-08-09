/** Local-only first-run flag (no secrets). P0: local-first, no cloud. */

const KEY = "hermes.onboarding.v1.done";

export function isOnboardingDone(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return true; // storage blocked → skip overlay rather than trap user
  }
}

export function markOnboardingDone(): void {
  try {
    localStorage.setItem(KEY, "1");
  } catch {
    /* ignore */
  }
}

/** Settings / debug: allow replaying the welcome ritual. */
export function resetOnboarding(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}
