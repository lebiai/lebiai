import { useState } from "react";
import { useLicenseStore, formatExpiresAt } from "../../store/licenseStore";
import { useUiStore } from "../../store/uiStore";
import { Button, ui } from "../common/ui";
import { toast } from "../../utils/toast";

const ERR_KEYS: Record<string, string> = {
  license_invalid_format: "license.err.invalid",
  license_bad_signature: "license.err.invalid",
  license_wrong_product: "license.err.wrongProduct",
  license_expired: "license.err.expired",
  license_older: "license.err.older",
  license_same: "license.err.same",
};

function errMessage(code: string, t: (k: never) => string): string {
  const key = ERR_KEYS[code];
  if (key) return t(key as never);
  return t("license.err.generic" as never);
}

/** Paste + activate only — no status chrome (parent owns that). */
export function LicenseForm({
  compact,
  onSuccess,
  autoFocus,
}: {
  compact?: boolean;
  onSuccess?: () => void;
  autoFocus?: boolean;
}) {
  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);
  const applyToken = useLicenseStore((s) => s.applyToken);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const raw = token.trim();
    if (!raw) {
      setError(t("license.err.empty"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const st = await applyToken(raw);
      setToken("");
      toast.success(
        t("license.successUntil", {
          date: formatExpiresAt(st.expiresAt, language),
        }),
      );
      onSuccess?.();
    } catch (e) {
      setError(errMessage(String(e), t as never));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={compact ? "space-y-2" : "space-y-3"}>
      <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
        {t("license.codeLabel")}
      </label>
      <textarea
        value={token}
        onChange={(e) => setToken(e.target.value)}
        rows={compact ? 3 : 4}
        placeholder={t("license.codePlaceholder")}
        className={`${ui.input} resize-y font-mono text-xs`}
        spellCheck={false}
        autoComplete="off"
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus={autoFocus}
      />
      {error && <p className="text-sm text-red-600 dark:text-red-300">{error}</p>}
      <Button size="sm" onClick={() => void submit()} disabled={busy}>
        {busy ? t("license.applying") : t("license.apply")}
      </Button>
    </div>
  );
}

export function LicenseBuyHint({ compact }: { compact?: boolean }) {
  const t = useUiStore((s) => s.t);
  const wechat = useLicenseStore((s) => s.status?.wechat) ?? "iodine001";

  const copyWechat = async () => {
    try {
      await navigator.clipboard.writeText(wechat);
      toast.success(t("license.wechatCopied"));
    } catch {
      toast.info(wechat);
    }
  };

  return (
    <div className={compact ? "space-y-1.5" : `${ui.cardMuted} p-3 space-y-2`}>
      {!compact && (
        <p className="text-sm text-app-fg dark:text-slate-100 font-medium">
          {t("license.buyTitle")}
        </p>
      )}
      <p className="text-xs text-app-fg-secondary leading-relaxed">
        {t("license.buyHint")}
      </p>
      <div className="flex items-center gap-2 flex-wrap">
        <code className="text-sm font-semibold px-2 py-1 rounded-lg bg-app-surface dark:bg-slate-900 border border-app-border">
          {wechat}
        </code>
        <Button size="sm" variant="secondary" onClick={() => void copyWechat()}>
          {t("license.copyWechat")}
        </Button>
      </div>
    </div>
  );
}
