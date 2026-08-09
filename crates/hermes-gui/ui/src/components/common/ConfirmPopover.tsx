import { useEffect, useRef, type ReactNode } from "react";
import { Button } from "./ui";
import { useUiStore } from "../../store/uiStore";

interface Props {
  open: boolean;
  message: string;
  onCancel: () => void;
  onConfirm: () => void;
  /** Anchor: render as absolute dropdown near trigger (parent relative). */
  children?: ReactNode;
  confirmLabel?: string;
  danger?: boolean;
}

/** Lightweight confirm dropdown (session/skill/memory delete). */
export function ConfirmPopover({
  open,
  message,
  onCancel,
  onConfirm,
  confirmLabel,
  danger = true,
}: Props) {
  const t = useUiStore((s) => s.t);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onCancel();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onCancel();
      }
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      ref={ref}
      className="absolute right-1 top-full z-30 mt-1 w-48 rounded-xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-900 shadow-lg p-2"
      onClick={(e) => e.stopPropagation()}
    >
      <p className="text-[11px] text-app-fg-secondary dark:text-slate-400 mb-2 px-0.5 leading-relaxed">
        {message}
      </p>
      <div className="flex gap-1.5">
        <Button size="sm" variant="secondary" className="flex-1" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button
          size="sm"
          variant={danger ? "danger" : "primary"}
          className="flex-1"
          onClick={onConfirm}
        >
          {confirmLabel ?? t("common.delete")}
        </Button>
      </div>
    </div>
  );
}
