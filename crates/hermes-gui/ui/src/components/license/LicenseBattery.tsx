import { useLicenseStore, formatRemaining } from "../../store/licenseStore";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { useSettingsNavStore } from "../../store/settingsNavStore";

/**
 * Inline status for sidebar user row — not a separate "battery column".
 * Purpose: glanceable time left + one tap to authorization when it matters.
 */
export function LicenseSidebarHint() {
  const status = useLicenseStore((s) => s.status);
  const setPanel = useNavStore((s) => s.setPanel);
  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);

  if (!status || status.showFullLock) return null;

  const needsAttention =
    status.onTrial || status.urgency === "expiring" || status.phase === "locked";

  const openLicense = () => {
    setPanel("settings");
    // Single deep-link path (settings-ia) — do not also bump licenseFocusId.
    useSettingsNavStore.getState().openTo("more", "license");
  };

  // Licensed with plenty of time: quiet one-line date, no toy battery bar.
  if (!needsAttention && status.phase === "licensed") {
    const date = status.expiresAt
      ? new Date(status.expiresAt).toLocaleDateString(
          language === "zh-CN" ? "zh-CN" : "en-US",
          { month: "short", day: "numeric" },
        )
      : "";
    return (
      <button
        type="button"
        onClick={openLicense}
        title={t("license.sidebarLicensedTitle")}
        className="text-[10px] text-app-fg-tertiary hover:text-app-fg-secondary truncate text-left max-w-full"
      >
        {date ? t("license.sidebarUntil", { date }) : t("license.batteryOk")}
      </button>
    );
  }

  // Trial / expiring: short chip, actionable.
  const chip =
    status.urgency === "expiring"
      ? "bg-amber-100 dark:bg-amber-950/50 text-amber-900 dark:text-amber-200"
      : "bg-app-muted dark:bg-slate-800 text-app-fg-secondary";

  const text = status.onTrial
    ? `${t("license.batteryTrial")} · ${formatRemaining(status.remainingSecs, t as never)}`
    : formatRemaining(status.remainingSecs, t as never);

  return (
    <button
      type="button"
      onClick={openLicense}
      title={text}
      className={`text-[10px] font-medium px-1.5 py-0.5 rounded-md truncate max-w-full ${chip}`}
    >
      {text}
    </button>
  );
}
