import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { InboxItemView } from "../../types";
import { ui } from "../common/ui";
import { toast } from "../../utils/toast";
import { playSeal } from "../../utils/ritual";

/**
 * Pending evolution candidates — lives under 记忆, not a separate product surface.
 */
export function PendingReviewSection({ onAccepted }: { onAccepted?: () => void }) {
  const t = useUiStore((s) => s.t);
  const [items, setItems] = useState<InboxItemView[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<InboxItemView[]>("list_pending_review");
      setItems(list);
    } catch {
      setItems([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => void refresh();
    window.addEventListener("hermes:inbox-changed", onChange);
    return () => window.removeEventListener("hermes:inbox-changed", onChange);
  }, [refresh]);

  if (items.length === 0) return null;

  const accept = async (id: string) => {
    setBusyId(id);
    try {
      await invoke("accept_pending_review", { id });
      playSeal(t("ritual.sealMemory"));
      toast.success(t("toast.inboxAccepted"));
      setItems((prev) => prev.filter((i) => i.id !== id));
      window.dispatchEvent(new CustomEvent("hermes:inbox-changed"));
      onAccepted?.();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const reject = async (id: string) => {
    setBusyId(id);
    try {
      await invoke("reject_pending_review", { id });
      toast.info(t("toast.inboxRejected"));
      setItems((prev) => prev.filter((i) => i.id !== id));
      window.dispatchEvent(new CustomEvent("hermes:inbox-changed"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="mb-4 space-y-2">
      <div className="flex items-baseline justify-between gap-2 px-0.5">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-app-fg-secondary">
          {t("memory.pendingTitle")}
          <span className="ml-1.5 normal-case font-medium text-app-primary tabular-nums">
            {items.length}
          </span>
        </h3>
        <p className="text-[11px] text-app-fg-tertiary">{t("memory.pendingHint")}</p>
      </div>
      <div className="space-y-2">
        {items.map((item) => (
          <article key={item.id} className={`${ui.card} p-3 space-y-1.5`}>
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex flex-wrap gap-1.5 items-center">
                  <span className="text-[10px] uppercase tracking-wide font-medium px-1.5 py-0.5 rounded bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
                    {item.kind === "skill" ? t("inbox.kindSkill") : t("inbox.kindMemory")}
                  </span>
                  {item.zone && (
                    <span className="text-[10px] text-app-fg-tertiary">{item.zone}</span>
                  )}
                </div>
                <pre className="text-xs text-app-fg dark:text-slate-100 whitespace-pre-wrap font-sans leading-relaxed max-h-36 overflow-y-auto">
                  {item.body}
                </pre>
              </div>
              <div className="flex flex-col gap-1 shrink-0">
                <button
                  type="button"
                  disabled={busyId === item.id}
                  onClick={() => void accept(item.id)}
                  className="p-1.5 rounded-lg hover:bg-emerald-100 dark:hover:bg-emerald-900/40 text-app-success disabled:opacity-40"
                  title={t("inbox.accept")}
                >
                  <Check size={15} />
                </button>
                <button
                  type="button"
                  disabled={busyId === item.id}
                  onClick={() => void reject(item.id)}
                  className="p-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/40 text-app-danger disabled:opacity-40"
                  title={t("inbox.reject")}
                >
                  <X size={15} />
                </button>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
