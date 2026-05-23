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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div className="w-full max-w-md mx-4 rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-2xl">
        <div className="flex items-start gap-3 px-5 pt-5 pb-3">
          <div className="shrink-0 mt-0.5 text-amber-500">
            <ShieldAlert size={20} />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm font-semibold text-gray-900 dark:text-gray-100">
              {t("confirm.title")}
            </div>
            <div className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">
              {t("confirm.description")}
            </div>
          </div>
        </div>

        <div className="px-5 pb-3 space-y-2">
          <div className="flex items-center gap-2 text-xs">
            <span className="text-gray-500 dark:text-gray-400">{t("confirm.tool")}</span>
            <span className="font-mono px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-800 dark:text-gray-200">
              {pending.toolName}
            </span>
          </div>
          <pre className="text-xs font-mono whitespace-pre-wrap break-all bg-gray-50 dark:bg-gray-800/60 border border-gray-200 dark:border-gray-700 rounded-md px-3 py-2 max-h-40 overflow-y-auto text-gray-700 dark:text-gray-300">
            {pending.summary}
          </pre>
        </div>

        <div className="px-5 pb-3">
          <label className="block text-xs text-gray-500 dark:text-gray-400 mb-1">
            {t("confirm.reason")}
          </label>
          <textarea
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={2}
            placeholder={t("confirm.reasonPlaceholder")}
            className="w-full resize-none rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-2.5 py-1.5 text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/60 rounded-b-xl">
          <button
            onClick={() => handle("deny")}
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            <Ban size={13} />
            {t("confirm.deny")}
          </button>
          <button
            onClick={() => handle("alwaysAllow")}
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md bg-amber-100 dark:bg-amber-900/40 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-900/60"
            title={t("confirm.alwaysAllowTitle")}
          >
            <ShieldCheck size={13} />
            {t("confirm.alwaysAllow")}
          </button>
          <button
            onClick={() => handle("allow")}
            autoFocus
            className="inline-flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md bg-blue-600 text-white hover:bg-blue-700"
          >
            <Check size={13} />
            {t("confirm.allow")}
          </button>
        </div>
      </div>
    </div>
  );
}
