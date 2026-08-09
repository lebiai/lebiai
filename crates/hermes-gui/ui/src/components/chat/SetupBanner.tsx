import { KeyRound, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { Button } from "../common/ui";

/** Non-blocking banner when API key is missing — points to Settings. */
export function SetupBanner() {
  const t = useUiStore((s) => s.t);
  const hasApiKey = useUiStore((s) => s.hasApiKey);
  const dismissed = useUiStore((s) => s.setupBannerDismissed);
  const dismissSetupBanner = useUiStore((s) => s.dismissSetupBanner);
  const setPanel = useNavStore((s) => s.setPanel);

  if (hasApiKey !== false || dismissed) return null;

  return (
    <div className="mx-4 mt-3 flex items-start gap-3 rounded-xl border border-amber-200 dark:border-amber-800/70 bg-amber-50 dark:bg-amber-950/30 px-3.5 py-3 shadow-[var(--shadow-app-card)]">
      <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-100 dark:bg-amber-900/50 text-amber-700 dark:text-amber-300">
        <KeyRound size={16} />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-amber-900 dark:text-amber-100">
          {t("setup.apiKeyTitle")}
        </p>
        <p className="text-xs text-amber-800/80 dark:text-amber-200/70 mt-0.5 leading-relaxed">
          {t("setup.apiKeyBody")}
        </p>
        <div className="mt-2.5">
          <Button
            size="sm"
            variant="secondary"
            className="border-amber-300 dark:border-amber-700 bg-white/80 dark:bg-slate-900/60"
            onClick={() => setPanel("settings")}
          >
            {t("setup.openSettings")}
          </Button>
        </div>
      </div>
      <button
        type="button"
        onClick={dismissSetupBanner}
        className="p-1 rounded-md text-amber-600/70 hover:bg-amber-100 dark:hover:bg-amber-900/40"
        aria-label={t("common.dismiss")}
      >
        <X size={14} />
      </button>
    </div>
  );
}
