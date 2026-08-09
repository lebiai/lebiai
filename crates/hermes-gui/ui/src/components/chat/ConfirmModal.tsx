import { useState } from "react";
import { ShieldAlert, Check, Ban, ShieldCheck } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";

export function ConfirmModal() {
  const pending = useChatStore((s) => s.pendingConfirm);
  const respondConfirm = useChatStore((s) => s.respondConfirm);
  const [reason, setReason] = useState("");
  const t = useUiStore((s) => s.t);

  if (!pending) return null;

  const handle = (action: "allow" | "alwaysAllow" | "deny") => {
    const r = action === "deny" ? reason : undefined;
    setReason("");
    respondConfirm(action, r);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 backdrop-blur-[2px]">
      <div className="w-full max-w-md mx-4 rounded-2xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-900 shadow-2xl">
        <div className="flex items-start gap-3 px-5 pt-5 pb-3">
          <div className="shrink-0 mt-0.5 flex h-9 w-9 items-center justify-center rounded-xl bg-amber-50 dark:bg-amber-950/40 text-amber-500">
            <ShieldAlert size={18} />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm font-semibold text-app-fg dark:text-slate-100">
              {t("confirm.title")}
            </div>
            <div className="mt-0.5 text-xs text-app-fg-secondary dark:text-slate-400">
              {t("confirm.description")}
            </div>
          </div>
        </div>

        <div className="px-5 pb-3 space-y-2">
          <div className="flex items-center gap-2 text-xs">
            <span className="text-app-fg-secondary dark:text-slate-400">{t("confirm.tool")}</span>
            <span className="font-mono px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg dark:text-slate-200">
              {pending.toolName}
            </span>
          </div>
          {pending.reason && (
            <div className="rounded-xl border border-amber-200/80 dark:border-amber-800/50 bg-amber-50/80 dark:bg-amber-950/30 px-3 py-2 text-xs text-amber-900 dark:text-amber-100 leading-relaxed">
              <span className="font-semibold">{t("confirm.why")}</span>{" "}
              {pending.reason}
            </div>
          )}
          <pre className="text-xs font-mono whitespace-pre-wrap break-all bg-app-muted/60 dark:bg-slate-800/60 border border-app-border dark:border-slate-700 rounded-xl px-3 py-2 max-h-40 overflow-y-auto text-app-fg-secondary dark:text-slate-300">
            {pending.summary}
          </pre>
        </div>

        <div className="px-5 pb-3">
          <label className="block text-xs text-app-fg-secondary dark:text-slate-400 mb-1">
            {t("confirm.reason")}
          </label>
          <textarea
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={2}
            placeholder={t("confirm.reasonPlaceholder")}
            className="w-full resize-none rounded-xl border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 px-2.5 py-1.5 text-xs focus:outline-none focus:ring-2 focus:ring-app-primary/40"
          />
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-app-border dark:border-slate-800 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl">
          <button
            type="button"
            onClick={() => handle("deny")}
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-xl border border-app-border dark:border-slate-600 text-app-fg dark:text-slate-200 hover:bg-app-muted dark:hover:bg-slate-800"
          >
            <Ban size={13} />
            {t("confirm.deny")}
          </button>
          <button
            type="button"
            onClick={() => handle("alwaysAllow")}
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-xl bg-amber-100 dark:bg-amber-900/40 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-900/60"
            title={t("confirm.alwaysAllowTitle")}
          >
            <ShieldCheck size={13} />
            {t("confirm.alwaysAllow")}
          </button>
          <button
            type="button"
            onClick={() => handle("allow")}
            autoFocus
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-xl bg-app-primary text-white hover:bg-app-primary-hover"
          >
            <Check size={13} />
            {t("confirm.allow")}
          </button>
        </div>
      </div>
    </div>
  );
}
