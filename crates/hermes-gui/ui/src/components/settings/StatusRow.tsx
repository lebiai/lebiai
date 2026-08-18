import type { ReactNode } from "react";

export function StatusRow({
  tone,
  title,
  subtitle,
  action,
}: {
  tone: "ok" | "warn" | "danger" | "neutral";
  title: string;
  subtitle?: string;
  action?: ReactNode;
}) {
  const bar =
    tone === "ok"
      ? "border-l-emerald-500"
      : tone === "warn"
        ? "border-l-amber-500"
        : tone === "danger"
          ? "border-l-red-500"
          : "border-l-app-border dark:border-l-slate-600";
  return (
    <div
      className={`flex items-center gap-3 pl-3 border-l-2 ${bar} py-1 min-w-0`}
    >
      <div className="min-w-0 flex-1">
        <p className="text-sm text-app-fg dark:text-slate-100 truncate">{title}</p>
        {subtitle && (
          <p className="text-[11px] text-app-fg-tertiary truncate">{subtitle}</p>
        )}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
