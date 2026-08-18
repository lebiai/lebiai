import brandLogo from "../../assets/logo.png";
import { useUiStore } from "../../store/uiStore";
import { LicenseBuyHint, LicenseForm } from "./LicenseForm";
import { useLicenseStore } from "../../store/licenseStore";

/**
 * Unclosable full-screen lock when trial/license expired.
 * Brand + slogan + paste code + WeChat (docs/license-ux.md).
 */
export function LicenseLockScreen() {
  const t = useUiStore((s) => s.t);
  const status = useLicenseStore((s) => s.status);
  const refresh = useLicenseStore((s) => s.refresh);

  if (!status?.showFullLock) return null;

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center p-6 bg-app-bg dark:bg-slate-950"
      role="dialog"
      aria-modal="true"
      aria-labelledby="license-lock-title"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
    >
      <div className="w-full max-w-md space-y-6">
        <div className="flex flex-col items-center text-center space-y-3">
          <img
            src={brandLogo}
            alt={t("app.brand")}
            className="h-14 w-14 rounded-2xl object-cover shadow-md"
          />
          <div>
            <h1
              id="license-lock-title"
              className="text-xl font-semibold tracking-tight text-app-fg dark:text-slate-50"
            >
              {t("app.brand")}
            </h1>
            <p className="mt-1 text-sm text-app-fg-secondary dark:text-slate-400">
              {t("app.tagline")}
            </p>
          </div>
          <div className="space-y-1 pt-2">
            <p className="text-base font-medium text-app-fg dark:text-slate-100">
              {t("license.lockTitle")}
            </p>
            <p className="text-sm text-app-fg-secondary leading-relaxed max-w-sm">
              {t("license.lockBody")}
            </p>
          </div>
        </div>

        <div className="rounded-2xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-900 p-5 shadow-lg space-y-4">
          <LicenseBuyHint />
          <LicenseForm
            autoFocus
            onSuccess={() => {
              void refresh();
            }}
          />
        </div>
      </div>
    </div>
  );
}
