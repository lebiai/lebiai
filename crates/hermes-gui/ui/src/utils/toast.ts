/**
 * App-wide floating toast API.
 * toast.success("Saved") / toast.error("Failed") / toast.info("…")
 * Host: <ToastHost /> mounted once in App.
 */

export type ToastVariant = "success" | "error" | "info";

export interface ToastItem {
  id: number;
  message: string;
  variant: ToastVariant;
  durationMs: number;
}

type Listener = (items: ToastItem[]) => void;

const DEFAULT_MS = 3000;
let seq = 1;
let items: ToastItem[] = [];
const listeners = new Set<Listener>();
const timers = new Map<number, number>();

function emit() {
  for (const l of listeners) l([...items]);
}

export function subscribeToasts(listener: Listener): () => void {
  listeners.add(listener);
  listener([...items]);
  return () => {
    listeners.delete(listener);
  };
}

export function dismissToast(id: number) {
  const t = timers.get(id);
  if (t != null) {
    window.clearTimeout(t);
    timers.delete(id);
  }
  const before = items.length;
  items = items.filter((x) => x.id !== id);
  if (items.length !== before) emit();
}

function show(
  message: string,
  variant: ToastVariant,
  durationMs = DEFAULT_MS
): number {
  const text = String(message ?? "").trim();
  if (!text) return -1;
  const id = seq++;
  while (items.length >= 3) {
    const old = items.shift();
    if (old) dismissToast(old.id);
  }
  items = [...items, { id, message: text, variant, durationMs }];
  emit();
  if (durationMs > 0) {
    const handle = window.setTimeout(() => dismissToast(id), durationMs);
    timers.set(id, handle);
  }
  return id;
}

export const toast = {
  show,
  success: (message: string, durationMs = DEFAULT_MS) =>
    show(message, "success", durationMs),
  error: (message: string, durationMs = DEFAULT_MS) =>
    show(message, "error", durationMs),
  info: (message: string, durationMs = DEFAULT_MS) =>
    show(message, "info", durationMs),
  dismiss: dismissToast,
  clear: () => {
    for (const id of [...timers.keys()]) dismissToast(id);
    items = [];
    emit();
  },
};
