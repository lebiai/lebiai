/**
 * Global “seal” ritual after the user accepts evolution candidates.
 * Host: <RitualSealHost /> in App. Pure CSS animation; respects reduced-motion.
 */

export type RitualKind = "seal" | "scroll";

export interface RitualPayload {
  kind: RitualKind;
  /** Short label under the mark (already translated). */
  label: string;
  /** First-ever seal: longer, with subtitle for discoverability */
  first?: boolean;
}

type Listener = (payload: RitualPayload | null) => void;

const listeners = new Set<Listener>();
let clearTimer: number | null = null;

const FIRST_SEAL_KEY = "hermes.ritual.v1.firstSealDone";

function emit(payload: RitualPayload | null) {
  for (const l of listeners) l(payload);
}

export function subscribeRitual(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function isFirstSeal(): boolean {
  try {
    return localStorage.getItem(FIRST_SEAL_KEY) !== "1";
  } catch {
    return false;
  }
}

function markFirstSealDone(): void {
  try {
    localStorage.setItem(FIRST_SEAL_KEY, "1");
  } catch {
    /* ignore */
  }
}

function reducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** Play a center-stage seal (memory / skill accept). */
export function playSeal(label: string) {
  if (clearTimer != null) {
    window.clearTimeout(clearTimer);
    clearTimer = null;
  }
  const first = isFirstSeal();
  if (first) markFirstSealDone();
  emit({ kind: "seal", label, first });
  // First seal stays longer so users notice the evolution moment.
  const ms = reducedMotion() ? (first ? 700 : 450) : first ? 1600 : 1100;
  clearTimer = window.setTimeout(() => {
    emit(null);
    clearTimer = null;
  }, ms);
}

/** Session-end “scroll closed”. */
export function playScroll(label: string) {
  if (clearTimer != null) {
    window.clearTimeout(clearTimer);
    clearTimer = null;
  }
  emit({ kind: "scroll", label, first: false });
  const ms = reducedMotion() ? 500 : 1200;
  clearTimer = window.setTimeout(() => {
    emit(null);
    clearTimer = null;
  }, ms);
}
