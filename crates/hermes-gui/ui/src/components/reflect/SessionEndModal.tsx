import { Sparkles, X } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { ReflectionReview } from "./ReflectionReview";
import { Button } from "../common/ui";

/**
 * Leave-session optional review — non-blocking background chip, then quiet modal.
 * Not a mandatory task; Done closes without pushing Reflect as a product home.
 */
export function SessionEndModal() {
  const sessionEnd = useChatStore((s) => s.sessionEnd);
  const updateSessionEndResult = useChatStore((s) => s.updateSessionEndResult);
  const completeSessionEnd = useChatStore((s) => s.completeSessionEnd);
  const dismissSessionEnd = useChatStore((s) => s.dismissSessionEnd);
  const t = useUiStore((s) => s.t);

  // Subtle non-blocking chip while work runs in the background.
  if (sessionEnd?.status === "background") {
    return (
      <div className="fixed bottom-4 right-4 z-40 max-w-sm rounded-xl border border-app-border dark:border-slate-700 bg-app-surface/95 dark:bg-slate-900/95 backdrop-blur-sm shadow-[var(--shadow-app-card)] px-3.5 py-2.5 flex items-center gap-2.5 fade-up-in">
        <Sparkles
          size={15}
          className="text-app-fg-tertiary shrink-0 motion-safe-only animate-pulse"
        />
        <p className="text-xs text-app-fg-secondary dark:text-slate-300 flex-1 leading-relaxed">
          {t("reflect.sessionEndBackground")}
        </p>
        <button
          type="button"
          onClick={() => dismissSessionEnd()}
          className="p-1 rounded-md hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-tertiary transition-colors duration-[var(--motion-fast)]"
          title={t("reflect.sessionEndDismiss")}
          aria-label={t("reflect.sessionEndDismiss")}
        >
          <X size={14} />
        </button>
      </div>
    );
  }

  if (!sessionEnd || sessionEnd.status !== "review") return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 dark:bg-black/50 backdrop-blur-[2px] p-4 fade-up-in">
      <div
        className="w-full max-w-2xl max-h-[85vh] flex flex-col rounded-2xl bg-app-surface dark:bg-slate-900 shadow-2xl border border-app-border dark:border-slate-700 session-enter"
        role="dialog"
        aria-modal="true"
        aria-labelledby="session-end-reflect-title"
      >
        <header className="flex items-center gap-2.5 px-4 py-3.5 border-b border-app-border dark:border-slate-800 shrink-0">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
            <Sparkles size={15} strokeWidth={1.75} />
          </div>
          <div className="flex-1 min-w-0">
            <h2
              id="session-end-reflect-title"
              className="text-base font-semibold text-app-fg dark:text-slate-100"
            >
              {t("reflect.sessionEndTitle")}
            </h2>
            <p className="text-[11px] text-app-fg-tertiary mt-0.5">
              {t("reflect.evolutionHint")}
            </p>
          </div>
          <button
            type="button"
            onClick={() => completeSessionEnd()}
            className="p-1.5 rounded-lg hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-tertiary transition-colors duration-[var(--motion-fast)]"
            title={t("reflect.sessionEndContinue")}
            aria-label={t("reflect.sessionEndContinue")}
          >
            <X size={16} />
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-4">
          <p className="text-xs text-app-fg-secondary dark:text-slate-400 mb-3 leading-relaxed">
            {t("reflect.sessionEndReviewHint")}
          </p>
          <ReflectionReview
            result={sessionEnd.result}
            onChange={updateSessionEndResult}
          />
        </div>

        <footer className="flex justify-end gap-2 px-4 py-3 border-t border-app-border dark:border-slate-800 shrink-0 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl">
          <Button
            size="sm"
            variant="primary"
            className="btn-press"
            onClick={() => completeSessionEnd()}
          >
            {t("reflect.sessionEndContinue")}
          </Button>
        </footer>
      </div>
    </div>
  );
}
