import { useEffect, useState } from "react";
import { BookMarked, Stamp } from "lucide-react";
import { subscribeRitual, type RitualPayload } from "../../utils/ritual";
import { useUiStore } from "../../store/uiStore";
import { RitualMark } from "./RitualMark";

/** Center-stage seal after accept — mount once in App. More visible: dim scrim. */
export function RitualSealHost() {
  const [payload, setPayload] = useState<RitualPayload | null>(null);
  const t = useUiStore((s) => s.t);

  useEffect(() => subscribeRitual(setPayload), []);

  if (!payload) return null;

  const isScroll = payload.kind === "scroll";

  return (
    <div
      className="fixed inset-0 z-[8500] flex items-center justify-center pointer-events-none"
      role="status"
      aria-live="polite"
      aria-label={payload.label}
    >
      {/* Scrim so the seal is impossible to miss */}
      <div
        className="absolute inset-0 bg-slate-900/35 dark:bg-black/50 fade-up-in motion-safe-only"
        aria-hidden
      />
      <div className="relative flex flex-col items-center gap-3 px-6">
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          <div className="h-28 w-28 rounded-full border-2 border-app-accent/50 dark:border-violet-400/50 ritual-ring motion-safe-only" />
        </div>
        <div className="ritual-seal-pop motion-safe-only relative">
          <RitualMark
            size="lg"
            tone="accent"
            className="shadow-xl shadow-violet-600/30 ring-4 ring-app-accent/20 dark:ring-violet-400/25"
          >
            {isScroll ? (
              <BookMarked size={26} strokeWidth={1.75} />
            ) : (
              <Stamp size={26} strokeWidth={1.75} />
            )}
          </RitualMark>
        </div>
        <div className="flex flex-col items-center gap-1 fade-up-in">
          <p className="text-sm font-semibold text-white dark:text-violet-100 bg-app-accent dark:bg-violet-700 px-4 py-1.5 rounded-full shadow-lg">
            {payload.label}
          </p>
          {payload.first && (
            <p className="text-[11px] text-center text-white/90 dark:text-violet-100/90 max-w-[16rem] leading-relaxed bg-slate-900/50 dark:bg-black/40 px-3 py-1.5 rounded-lg mt-1">
              {t("ritual.firstSealHint")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
