import { useEffect, useRef, useState } from "react";
import {
  useLicenseStore,
  formatExpiresAt,
  formatRemaining,
} from "../../store/licenseStore";
import { useUiStore } from "../../store/uiStore";
import { Button } from "../common/ui";
import { LicenseBuyHint, LicenseForm } from "./LicenseForm";
import { useSettingsNavStore } from "../../store/settingsNavStore";

/**
 * Authorization in Settings — product path:
 *
 * | State              | User sees                                      |
 * |--------------------|------------------------------------------------|
 * | Licensed (ample)   | Status only; renew is opt-in                   |
 * | Trial              | Status + how to buy; paste only if they opt in |
 * | Expiring / locked  | Status + buy + paste open                      |
 */
export function LicenseSettingsCard() {
  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);
  const status = useLicenseStore((s) => s.status);
  const navRequestId = useSettingsNavStore((s) => s.navRequestId);
  const navFocus = useSettingsNavStore((s) => s.focus);
  const clearFocus = useSettingsNavStore((s) => s.clearFocus);
  const ref = useRef<HTMLDivElement>(null);
  const [pasteOpen, setPasteOpen] = useState(false);
  /** Last navRequestId we already handled — avoid re-entry loops. */
  const handledNavId = useRef(0);

  // Deep-link from sidebar / overview: expand paste once per openTo().
  useEffect(() => {
    if (navRequestId <= 0 || navFocus !== "license") return;
    if (handledNavId.current === navRequestId) return;
    handledNavId.current = navRequestId;
    setPasteOpen(true);
    // Defer scroll so accordion has painted.
    requestAnimationFrame(() => {
      ref.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
    clearFocus();
  }, [navRequestId, navFocus, clearFocus]);

  if (!status) return null;

  const isLocked = status.phase === "locked";
  const isTrial = status.onTrial && !isLocked;
  const isLicensed = status.phase === "licensed";
  const isExpiring = status.urgency === "expiring" && !isLocked;
  /** Input is primary only when time is short or access is gone. */
  const mustPaste = isLocked || isExpiring;
  // User-opened paste, or must-show states. Do not auto-collapse while mustPaste.
  const showPaste = mustPaste || pasteOpen;

  const pct = Math.round(Math.max(0, Math.min(1, status.remainingRatio)) * 100);
  const fill =
    status.urgency === "expired"
      ? "bg-red-500"
      : status.urgency === "expiring"
        ? "bg-amber-500"
        : "bg-emerald-500";

  const statusLabel = isLocked
    ? t("license.statusLocked")
    : isTrial
      ? t("license.statusTrial")
      : t("license.statusLicensed");

  const hint = isLicensed && !isExpiring
    ? t("license.settingsHintLicensed")
    : isTrial
      ? t("license.settingsHintTrial")
      : t("license.settingsHint");

  return (
    <div ref={ref} id="settings-license" className="space-y-4 p-1">
      <div className="space-y-1">
        <h3 className="text-sm font-semibold text-app-fg dark:text-slate-100">
          {t("license.settingsTitle")}
        </h3>
        <p className="text-xs text-app-fg-tertiary leading-relaxed">{hint}</p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <span
            className={`text-xs font-semibold px-2 py-0.5 rounded-md ${
              isLocked
                ? "bg-red-100 dark:bg-red-950/50 text-red-800 dark:text-red-200"
                : isExpiring || isTrial
                  ? "bg-amber-100 dark:bg-amber-950/40 text-amber-900 dark:text-amber-200"
                  : "bg-emerald-100 dark:bg-emerald-950/40 text-emerald-900 dark:text-emerald-200"
            }`}
          >
            {statusLabel}
          </span>
          <span className="text-xs text-app-fg-secondary">
            {status.expiresAt
              ? t("license.until", {
                  date: formatExpiresAt(status.expiresAt, language),
                })
              : "—"}
          </span>
        </div>
        {!isLocked && (
          <>
            <div className="h-1.5 rounded-full bg-app-muted dark:bg-slate-800 overflow-hidden">
              <div
                className={`h-full ${fill} transition-all`}
                style={{ width: `${Math.max(pct, pct > 0 ? 4 : 0)}%` }}
              />
            </div>
            <p className="text-[11px] text-app-fg-tertiary">
              {formatRemaining(status.remainingSecs, t as never)}
            </p>
          </>
        )}
        {isLocked && (
          <p className="text-[11px] text-app-fg-tertiary">{t("license.remainingNone")}</p>
        )}
      </div>

      {/* Calm licensed: no empty input staring at the user */}
      {isLicensed && !isExpiring && !showPaste && (
        <div className="flex flex-wrap items-center gap-2 pt-0.5">
          <p className="text-xs text-app-fg-secondary flex-1 min-w-[10rem]">
            {t("license.activeQuiet")}
          </p>
          <Button size="sm" variant="secondary" onClick={() => setPasteOpen(true)}>
            {t("license.showRenew")}
          </Button>
        </div>
      )}

      {/* Trial calm: buy channel + optional “I have a code” */}
      {isTrial && !isExpiring && !showPaste && (
        <div className="space-y-3">
          <LicenseBuyHint />
          <Button size="sm" variant="secondary" onClick={() => setPasteOpen(true)}>
            {t("license.iHaveCode")}
          </Button>
        </div>
      )}

      {/* Paste open: buy + form (+ collapse when not forced) */}
      {showPaste && (
        <div className="space-y-3 border-t border-app-border dark:border-slate-700/80 pt-3">
          {!mustPaste && (
            <div className="flex justify-end">
              <button
                type="button"
                className="text-xs text-app-fg-tertiary hover:text-app-fg-secondary"
                onClick={() => setPasteOpen(false)}
              >
                {t("license.hideRenew")}
              </button>
            </div>
          )}
          <LicenseBuyHint />
          <LicenseForm
            autoFocus={mustPaste || pasteOpen}
            onSuccess={() => {
              if (!mustPaste) setPasteOpen(false);
            }}
          />
        </div>
      )}
    </div>
  );
}
