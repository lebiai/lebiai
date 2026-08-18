/** First-keep tip only. After that, materials stay quiet. */

const KEY = "hermes.materials.firstKeep.v1";

export function isFirstKeepTipPending(): boolean {
  try {
    return localStorage.getItem(KEY) !== "1";
  } catch {
    return false;
  }
}

export function markFirstKeepTipSeen(): void {
  try {
    localStorage.setItem(KEY, "1");
  } catch {
    /* ignore */
  }
}
