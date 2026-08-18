import { Sparkles, X } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { reflectionHasCandidates } from "../../types";
import { ReflectionReview } from "./ReflectionReview";
import { ui } from "../common/ui";

/** Non-blocking mid-chat review for micro-reflection candidates. */
export function MicroReviewModal() {
  const open = useChatStore((s) => s.microReviewOpen);
  const result = useChatStore((s) => s.microReview);
  const updateMicroReview = useChatStore((s) => s.updateMicroReview);
  const dismissMicroReview = useChatStore((s) => s.dismissMicroReview);
  const t = useUiStore((s) => s.t);

  if (!open || !result || !reflectionHasCandidates(result)) return null;

  return (
    <div className={`${ui.overlay} z-50 p-4`}>
      <div
        className="w-full max-w-2xl max-h-[85vh] flex flex-col rounded-2xl bg-app-surface dark:bg-slate-900 shadow-2xl border border-app-border dark:border-slate-700"
        role="dialog"
        aria-modal="true"
        aria-labelledby="micro-reflect-title"
      >
        <header className="flex items-center gap-2 px-4 py-3.5 border-b border-app-border dark:border-slate-800 shrink-0">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-app-accent-soft dark:bg-violet-950/50 text-app-accent">
            <Sparkles size={16} />
          </div>
          <div className="flex-1 min-w-0">
            <h2
              id="micro-reflect-title"
              className="text-base font-semibold text-app-fg dark:text-slate-100"
            >
              {t("reflect.microTitle")}
            </h2>
            <p className="text-[11px] text-app-fg-tertiary mt-0.5">
              {t("reflect.evolutionHint")}
            </p>
          </div>
          <button
            type="button"
            onClick={() => dismissMicroReview()}
            className="p-1.5 rounded-lg hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-tertiary"
            title={t("common.dismiss")}
          >
            <X size={16} />
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-4">
          <p className="text-xs text-app-fg-secondary dark:text-slate-400 mb-3 leading-relaxed">
            {t("reflect.microReviewHint")}
          </p>
          <ReflectionReview result={result} onChange={updateMicroReview} />
        </div>

        <footer className="flex justify-end gap-2 px-4 py-3 border-t border-app-border dark:border-slate-800 shrink-0 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl">
          <button
            type="button"
            onClick={() => {
              dismissMicroReview();
              updateMicroReview(null);
            }}
            className="px-4 py-2 text-sm rounded-xl bg-app-accent text-white hover:bg-violet-700 font-medium shadow-sm"
          >
            {t("reflect.microDone")}
          </button>
        </footer>
      </div>
    </div>
  );
}
