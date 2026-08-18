import { FileText } from "lucide-react";
import type { ParsedAttachment } from "../../utils/attachments";

type ChipProps = {
  originalName: string;
  mdRelPath?: string;
  chars?: number;
  /** pending | ready | error */
  status?: "pending" | "ready" | "error";
  onRemove?: () => void;
  removeLabel?: string;
  errorLabel?: string;
  /**
   * composer — input chips
   * message — outside bubble (default chat UX)
   */
  variant?: "composer" | "message";
};

export function AttachmentChip({
  originalName,
  mdRelPath,
  chars,
  status = "ready",
  onRemove,
  removeLabel = "Remove",
  errorLabel,
  variant = "composer",
}: ChipProps) {
  const isMessage = variant === "message";
  const base = isMessage
    ? "border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-900 text-app-fg dark:text-slate-100 shadow-sm"
    : "border-app-border dark:border-slate-600 bg-app-muted/60 dark:bg-slate-800/80 text-app-fg-secondary";

  const statusDot =
    status === "pending"
      ? "bg-amber-400 animate-pulse"
      : status === "error"
        ? "bg-red-400"
        : "bg-app-primary/70";

  return (
    <span
      className={`inline-flex items-center gap-1.5 max-w-[16rem] rounded-xl border px-2.5 py-1.5 text-[11px] ${base}`}
      title={mdRelPath ?? originalName}
    >
      <span className={`h-1.5 w-1.5 rounded-full shrink-0 ${statusDot}`} />
      <FileText
        size={13}
        className={`shrink-0 ${isMessage ? "text-app-primary dark:text-blue-400" : "opacity-80"}`}
      />
      <span className="truncate font-medium leading-snug">{originalName}</span>
      {status === "error" && (
        <span className="shrink-0 text-red-600 dark:text-red-400">
          {errorLabel || "—"}
        </span>
      )}
      {typeof chars === "number" && status === "ready" && (
        <span className="shrink-0 tabular-nums text-app-fg-tertiary">
          {chars.toLocaleString()}
        </span>
      )}
      {onRemove && (
        <button
          type="button"
          className="shrink-0 p-0.5 rounded hover:bg-app-muted dark:hover:bg-slate-700 text-app-fg-tertiary"
          aria-label={removeLabel}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          ×
        </button>
      )}
    </span>
  );
}

export function AttachmentCardList({
  items,
  variant = "composer",
  className = "",
}: {
  items: ParsedAttachment[];
  variant?: "composer" | "message";
  className?: string;
}) {
  if (items.length === 0) return null;
  return (
    <div className={`flex flex-wrap gap-1.5 ${className}`}>
      {items.map((a, i) => (
        <AttachmentChip
          key={`${a.mdRelPath}-${a.originalName}-${i}`}
          originalName={a.originalName}
          mdRelPath={a.mdRelPath}
          chars={a.chars}
          variant={variant}
        />
      ))}
    </div>
  );
}
