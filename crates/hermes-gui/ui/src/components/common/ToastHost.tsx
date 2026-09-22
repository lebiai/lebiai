import { useEffect, useState } from "react";
import { CheckCircle2, Info, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import {
  subscribeToasts,
  dismissToast,
  type ToastItem,
  type ToastVariant,
} from "../../utils/toast";

function styles(variant: ToastVariant): string {
  if (variant === "error") {
    // 错误不用红底红叉：满屏红叉读起来像「处处都坏了」。深色底 + 一道玫红边
    // + 一个「没成」的字样，说的是同一件事，但不吓人。
    return "bg-slate-800 text-white border-slate-900 border-l-4 border-l-rose-400 dark:bg-slate-700 dark:border-slate-600 dark:border-l-rose-400";
  }
  if (variant === "success") {
    return "bg-emerald-600 text-white border-emerald-700/80";
  }
  return "bg-slate-800 text-white border-slate-900 dark:bg-slate-700 dark:border-slate-600";
}

function Marker({ variant, label }: { variant: ToastVariant; label: string }) {
  if (variant === "error") {
    return (
      <span className="shrink-0 mt-0.5 text-xs font-semibold text-rose-300">{label}</span>
    );
  }
  if (variant === "success") return <CheckCircle2 size={16} className="shrink-0 mt-0.5" />;
  return <Info size={16} className="shrink-0 mt-0.5" />;
}

/** Global toast host — mount once in App. */
export function ToastHost() {
  const t = useUiStore((s) => s.t);
  const [items, setItems] = useState<ToastItem[]>([]);

  useEffect(() => subscribeToasts(setItems), []);

  if (items.length === 0) return null;

  return (
    <div
      className="fixed bottom-8 left-1/2 -translate-x-1/2 z-[9000] flex flex-col-reverse gap-2 w-[min(28rem,calc(100vw-2rem))] pointer-events-none"
      role="region"
      aria-label="notifications"
    >
      {items.map((item) => (
        <div
          key={item.id}
          className={`pointer-events-auto shadow-lg rounded-xl border px-4 py-2.5 text-sm leading-relaxed flex items-start gap-2.5 toast-in motion-safe-only ${styles(item.variant)}`}
          role="status"
          aria-live={item.variant === "error" ? "assertive" : "polite"}
        >
          <Marker variant={item.variant} label={t("toast.failed")} />
          <span className="flex-1 break-words whitespace-pre-wrap">{item.message}</span>
          {item.action ? (
            <button
              type="button"
              className="shrink-0 underline underline-offset-2 opacity-95 hover:opacity-100 text-xs font-medium"
              onClick={() => {
                item.action?.onClick();
                dismissToast(item.id);
              }}
            >
              {item.action.label}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => dismissToast(item.id)}
            className="shrink-0 opacity-80 hover:opacity-100 p-0.5 rounded"
            aria-label="dismiss"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
