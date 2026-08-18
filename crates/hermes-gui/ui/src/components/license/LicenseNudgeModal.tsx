import { useEffect, useState } from "react";
import { useChatStore } from "../../store/chatStore";
import { useLicenseStore, formatExpiresAt, formatRemaining } from "../../store/licenseStore";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { Button, ui } from "../common/ui";
import { LicenseBuyHint, LicenseForm } from "./LicenseForm";

/**
 * Daily renew reminder when urgency=expiring (incl. trial last 3 days).
 * Closable; max once per local day (backend last_nudge_date).
 */
export function LicenseNudgeModal() {
  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);
  const status = useLicenseStore((s) => s.status);
  const markNudgeSeen = useLicenseStore((s) => s.markNudgeSeen);
  const refresh = useLicenseStore((s) => s.refresh);
  const setPanel = useNavStore((s) => s.setPanel);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const [open, setOpen] = useState(false);
  const [showPaste, setShowPaste] = useState(false);

  useEffect(() => {
    if (!status?.shouldNudge) return;
    if (status.showFullLock) return;
    if (isStreaming) return;
    // Brief delay after shell ready
    const tmr = window.setTimeout(() => setOpen(true), 1200);
    return () => window.clearTimeout(tmr);
  }, [status?.shouldNudge, status?.showFullLock, isStreaming]);

  if (!open || !status?.shouldNudge) return null;

  const dismiss = async () => {
    await markNudgeSeen();
    setOpen(false);
    setShowPaste(false);
  };

  const goPaste = () => {
    setShowPaste(true);
  };

  const goSettings = async () => {
    await markNudgeSeen();
    setOpen(false);
    setPanel("settings");
    const { useSettingsNavStore } = await import(
      "../../store/settingsNavStore"
    );
    useSettingsNavStore.getState().openTo("more", "license");
  };

  return (
    <div className={`${ui.overlay} z-[150] p-4`}>
      <div
        className={`${ui.card} w-full max-w-md p-5 space-y-4 shadow-xl`}
        role="dialog"
        aria-labelledby="license-nudge-title"
      >
        <div className="space-y-1">
          <h2
            id="license-nudge-title"
            className="text-base font-semibold text-app-fg dark:text-slate-100"
          >
            {status.onTrial ? t("license.nudgeTitleTrial") : t("license.nudgeTitle")}
          </h2>
          <p className="text-sm text-app-fg-secondary leading-relaxed">
            {t("license.nudgeBody", {
              remaining: formatRemaining(status.remainingSecs, t as never),
              date: formatExpiresAt(status.expiresAt, language),
            })}
          </p>
        </div>

        {showPaste ? (
          <div className="space-y-3">
            <LicenseBuyHint compact />
            <LicenseForm
              compact
              autoFocus
              onSuccess={() => {
                void refresh();
                setOpen(false);
                setShowPaste(false);
              }}
            />
          </div>
        ) : (
          <div className="flex flex-col sm:flex-row gap-2 sm:justify-end">
            <Button size="sm" variant="secondary" onClick={() => void dismiss()}>
              {t("license.nudgeLater")}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => void goSettings()}>
              {t("license.nudgeSettings")}
            </Button>
            <Button size="sm" onClick={goPaste}>
              {t("license.nudgePaste")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
