import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { InboxItemView } from "../../types";
import { toast } from "../../utils/toast";
import { playSeal } from "../../utils/ritual";

/**
 * Pending evolution candidates — lives under 记忆, not a separate product surface.
 * Strong visual highlight so users who saw the sidebar count find the same zone inside.
 */
export function PendingReviewSection({
  onAccepted,
  onChanged,
}: {
  onAccepted?: () => void;
  onChanged?: () => void;
}) {
  const t = useUiStore((s) => s.t);
  const [items, setItems] = useState<InboxItemView[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<InboxItemView[]>("list_pending_review");
      setItems(list);
    } catch {
      setItems([]);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => void refresh();
    window.addEventListener("hermes:inbox-changed", onChange);
    return () => window.removeEventListener("hermes:inbox-changed", onChange);
  }, [refresh]);

  if (!loaded || items.length === 0) return null;

  const accept = async (id: string) => {
    setBusyId(id);
    try {
      await invoke("accept_pending_review", { id });
      playSeal(t("ritual.sealMemory"));
      toast.success(t("toast.inboxAccepted"));
      setItems((prev) => prev.filter((i) => i.id !== id));
      window.dispatchEvent(new CustomEvent("hermes:inbox-changed"));
      onAccepted?.();
      onChanged?.();
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
      onChanged?.();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section
      className="mb-5 space-y-2 rounded-2xl border-2 border-amber-400 dark:border-amber-500/70 bg-amber-50 dark:bg-amber-950/35 p-3.5 shadow-md ring-2 ring-amber-200/80 dark:ring-amber-800/40"
      aria-label={t("memory.pendingTitle")}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center rounded-full bg-amber-500 text-white text-[10px] font-bold uppercase tracking-wide px-2 py-0.5 shadow-sm">
              {t("memory.pendingBadge")}
            </span>
            <h3 className="text-sm font-semibold text-amber-950 dark:text-amber-50">
              {t("memory.pendingTitle")}
            </h3>
            <span className="inline-flex items-center justify-center min-w-[1.25rem] h-5 px-1.5 rounded-full bg-amber-500 text-white text-[11px] font-bold tabular-nums">
              {items.length > 99 ? "99+" : items.length}
            </span>
          </div>
          <p className="mt-1.5 text-[11px] text-amber-900/85 dark:text-amber-100/85 leading-snug">
            {t("memory.pendingHint")}
          </p>
        </div>
      </div>
      <div className="space-y-2">
        {items.map((item) => (
          <article
            key={item.id}
            className="rounded-xl border border-amber-300 dark:border-amber-700/70 bg-white dark:bg-slate-900/90 p-3 space-y-1.5 shadow-sm"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex flex-wrap gap-1.5 items-center">
                  <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded-md bg-amber-100 dark:bg-amber-900/60 text-amber-900 dark:text-amber-100">
                    {t("memory.pendingBadge")}
                  </span>
                  <span className="text-[10px] uppercase tracking-wide font-medium px-1.5 py-0.5 rounded bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
                    {item.kind === "skill" ? t("inbox.kindSkill") : t("inbox.kindMemory")}
                  </span>
                  {item.zone && (
                    <span className="text-[10px] text-app-fg-tertiary">{item.zone}</span>
                  )}
                </div>
                {item.title && (
                  <p className="text-xs font-semibold text-app-fg dark:text-slate-100 truncate">
                    {item.title}
                  </p>
                )}
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
