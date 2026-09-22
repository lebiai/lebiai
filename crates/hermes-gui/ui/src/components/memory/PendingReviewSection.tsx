import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, FolderTree, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { InboxItemView } from "../../types";
import { companionGroup, memoryGroupLabel } from "../../utils/memoryGroup";
import { toast } from "../../utils/toast";
import { playSeal } from "../../utils/ritual";
import { errorText } from "../../utils/errorText";

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
      // 回执要说清这句话落在了**谁**名下（`docs/spec/projects.md` §5.1 规矩 4）：
      // 「记下了」而不说归谁，用户没法判断它到底学没学会、下次会不会端出来。
      const out = await invoke<{
        ownerId?: string;
        ownerName?: string;
        alreadyKnown?: boolean;
      }>("accept_pending_review", { id });
      playSeal(t("ritual.sealMemory"));
      toast.success(
        out?.alreadyKnown
          ? t("toast.inboxAcceptedAlreadyKnown")
          : out?.ownerName
            ? t("toast.inboxAcceptedAs", { owner: out.ownerName })
            : t("toast.inboxAccepted"),
      );
      setItems((prev) => prev.filter((i) => i.id !== id));
      window.dispatchEvent(new CustomEvent("hermes:inbox-changed"));
      onAccepted?.();
      onChanged?.();
    } catch (e) {
      toast.error(errorText(e));
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
      toast.error(errorText(e));
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
            <span className="inline-flex items-center rounded-full bg-amber-500 text-white text-xs font-bold uppercase tracking-wide px-2 py-0.5 shadow-sm">
              {t("memory.pendingBadge")}
            </span>
            <h3 className="text-app-body font-semibold text-amber-950 dark:text-amber-50">
              {t("memory.pendingTitle")}
            </h3>
            <span className="inline-flex items-center justify-center min-w-[1.25rem] h-5 px-1.5 rounded-full bg-amber-500 text-white text-xs font-bold tabular-nums">
              {items.length > 99 ? "99+" : items.length}
            </span>
          </div>
          <p className="mt-1.5 text-app-sub text-amber-900/85 dark:text-amber-100/85 leading-snug">
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
                  <span className="text-xs font-semibold px-1.5 py-0.5 rounded-md bg-amber-100 dark:bg-amber-900/60 text-amber-900 dark:text-amber-100">
                    {t("memory.pendingBadge")}
                  </span>
                  <span className="text-xs uppercase tracking-wide font-medium px-1.5 py-0.5 rounded bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
                    {item.kind === "skill" ? t("inbox.kindSkill") : t("inbox.kindMemory")}
                  </span>
                  {item.zone && (
                    <span className="text-xs text-app-fg-secondary">
                      {memoryGroupLabel(t, companionGroup(item))}
                    </span>
                  )}
                </div>
                {item.title && (
                  <p className="text-xs font-semibold text-app-fg dark:text-slate-100 truncate">
                    {item.title}
                  </p>
                )}
                <pre className="text-app-body text-app-fg dark:text-slate-100 whitespace-pre-wrap font-sans leading-relaxed max-h-36 overflow-y-auto">
                  {item.body}
                </pre>
                {/*
                  归属必须在**点头之前**看得见：用户要判断的不是「这条对不对」，
                  还有「它以后在哪儿算数」——事后 toast 是报账，不是知情（规格 §5.1 规矩 4）。
                */}
                {item.kind === "memory" && (
                  <p className="text-app-sub text-app-fg-secondary flex items-center gap-1">
                    <FolderTree size={13} strokeWidth={1.75} className="shrink-0" />
                    {item.ownerName
                      ? t("inbox.willFileUnder", { owner: item.ownerName })
                      : t("inbox.willFileGlobal")}
                  </p>
                )}
              </div>
            </div>
            {/* 决定要写成字：一个孤零零的对勾/叉，用户得先悬停才知道它是什么。 */}
            <div className="flex items-center gap-2 pt-0.5">
              <button
                type="button"
                disabled={busyId === item.id}
                onClick={() => void accept(item.id)}
                className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg text-app-sub font-medium bg-app-primary text-white hover:bg-app-primary-hover disabled:opacity-40 transition-colors duration-[var(--motion-fast)]"
              >
                <Check size={14} strokeWidth={2} />
                {t("inbox.accept")}
              </button>
              <button
                type="button"
                disabled={busyId === item.id}
                onClick={() => void reject(item.id)}
                className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg text-app-sub font-medium text-app-fg-secondary hover:text-app-danger hover:bg-red-50 dark:hover:bg-red-950/40 disabled:opacity-40 transition-colors duration-[var(--motion-fast)]"
              >
                <X size={14} strokeWidth={2} />
                {t("inbox.reject")}
              </button>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
