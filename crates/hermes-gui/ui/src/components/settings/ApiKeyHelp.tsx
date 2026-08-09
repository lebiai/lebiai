import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ExternalLink, KeyRound, ShieldCheck } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { TranslationKey } from "../../i18n";
import { Button, ui } from "../common/ui";

/**
 * Collapsible "how do I get an API key" help, shown next to the API-key
 * field while the selected provider has no key. Closed by default so the
 * field stays the visual focus; the title row expands the guide. Opens the
 * selected provider's official page through the whitelisted
 * `open_api_key_guide` command (fixed provider → fixed URL, no free-form
 * input reaches the browser opener).
 */
export function ApiKeyHelp({ provider }: { provider: string }) {
  const t = useUiStore((s) => s.t);
  const [open, setOpen] = useState(false);

  const toggle = () => setOpen((v) => !v);
  const providerName = t(`provider.${provider}` as TranslationKey);
  const openGuide = () => {
    invoke("open_api_key_guide", { provider }).catch((e) => console.error(e));
  };

  return (
    <div
      className={`${ui.card} p-4 space-y-3 border-amber-200 dark:border-amber-800/70 bg-amber-50/60 dark:bg-amber-950/20`}
    >
      <button
        type="button"
        onClick={toggle}
        className="flex w-full items-center justify-between gap-2 text-left"
      >
        <span className="flex items-center gap-2">
          <KeyRound size={15} className="text-amber-600 dark:text-amber-400" />
          <span className="text-sm font-medium text-app-fg dark:text-slate-100">
            {t("settings.apiKeyHelpTitle")}
          </span>
        </span>
        <ChevronDown
          size={15}
          className={`text-app-fg-tertiary transition-transform duration-[var(--motion-fast)] ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open && (
        <div className="space-y-3">
          <p className="text-xs leading-relaxed text-app-fg-secondary dark:text-slate-400">
            {t("settings.apiKeyHelpWhy")}
          </p>
          <ol className="list-decimal list-inside text-xs space-y-1.5 text-app-fg-secondary dark:text-slate-400">
            <li>{t("settings.apiKeyHelpStep1")}</li>
            <li>{t("settings.apiKeyHelpStep2")}</li>
            <li>{t("settings.apiKeyHelpStep3")}</li>
          </ol>
          <Button size="sm" variant="secondary" onClick={openGuide}>
            <ExternalLink size={13} /> {t("settings.apiKeyOpen", { name: providerName })}
          </Button>
          <p className="flex items-start gap-1.5 text-xs text-app-fg-tertiary dark:text-slate-500">
            <ShieldCheck size={13} className="mt-0.5 shrink-0" />
            <span>{t("settings.apiKeyHelpNotice")}</span>
          </p>
        </div>
      )}
    </div>
  );
}
