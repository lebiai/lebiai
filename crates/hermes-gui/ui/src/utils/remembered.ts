/**
 * Natural “you were remembered” feedback — seal + toast + optional memory list highlight.
 * Used after memory_save (chat tool) or GUI create_memory. Never steers the user to Reflect.
 */

import { toast } from "./toast";
import { playSeal } from "./ritual";
import { useUiStore } from "../store/uiStore";

/** Tool result line: `Saved memory {id} → {path}` */
const SAVED_MEMORY_RE = /^Saved memory\s+(\S+)/;

let batchCount = 0;
let batchTimer: number | null = null;
let lastMemoryId: string | undefined;

export function parseSavedMemoryId(content: string | undefined | null): string | undefined {
  if (!content) return undefined;
  const m = content.match(SAVED_MEMORY_RE);
  return m?.[1];
}

/**
 * Queue a remembered celebration. Multiple successes within ~450ms batch into one seal.
 * @param memoryId optional id for Memory panel pulse highlight
 */
export function notifyRemembered(memoryId?: string) {
  batchCount += 1;
  if (memoryId) lastMemoryId = memoryId;

  if (batchTimer != null) {
    window.clearTimeout(batchTimer);
  }
  batchTimer = window.setTimeout(() => {
    const n = batchCount;
    const id = lastMemoryId;
    batchCount = 0;
    lastMemoryId = undefined;
    batchTimer = null;

    const t = useUiStore.getState().t;
    playSeal(t("ritual.sealMemory"));
    if (n > 1) {
      toast.success(t("toast.memorySavedMany", { count: n }));
    } else {
      toast.success(t("toast.memorySaved"));
    }
    if (id) {
      useUiStore.getState().pulseMemoryHighlight(id);
    }
  }, 450);
}

/** Fire after micro-reflection auto-accepted memories (already has its own toast). */
export function sealRememberedAuto(count: number) {
  if (count <= 0) return;
  const t = useUiStore.getState().t;
  playSeal(t("ritual.sealMemory"));
  // Toast is handled by chatStore (micro-specific copy).
}
