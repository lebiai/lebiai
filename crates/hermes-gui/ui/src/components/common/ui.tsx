import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
} from "react";

/** Shared class fragments — single shell language for all panels (phase A). */
export const ui = {
  panel: "bg-app-surface dark:bg-slate-900 text-app-fg dark:text-slate-100",
  page: "bg-app-bg dark:bg-slate-950",
  /** Shell chrome: not document text — disable drag-select (inputs opt back in). */
  sidebar:
    "bg-app-sidebar dark:bg-slate-900/95 border-r border-app-border dark:border-slate-800 select-none",
  card:
    "rounded-xl border border-app-border dark:border-slate-700/80 bg-app-surface dark:bg-slate-900 shadow-[var(--shadow-app-card)]",
  cardMuted:
    "rounded-xl border border-app-border dark:border-slate-700/80 bg-app-muted/60 dark:bg-slate-800/50",
  input:
    "w-full rounded-xl border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 px-3 py-2 text-sm text-app-fg dark:text-slate-100 placeholder:text-app-fg-tertiary focus:outline-none focus:ring-2 focus:ring-app-primary/40 focus:border-app-primary/60 transition-shadow duration-[var(--motion-fast)] select-text",
  header:
    "flex items-center justify-between gap-3 px-4 py-3 border-b border-app-border dark:border-slate-800 bg-app-surface/80 dark:bg-slate-900/80 backdrop-blur-sm shrink-0",
  /** Scroll body inside a full-height panel */
  body: "flex-1 min-h-0 overflow-y-auto",
  navItem:
    "w-full flex items-center gap-2 px-3 py-2 rounded-lg text-sm transition-colors duration-[var(--motion-fast)] select-none",
  navItemActive:
    "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 font-medium",
  navItemIdle:
    "text-app-fg-secondary dark:text-slate-300 hover:bg-app-muted dark:hover:bg-slate-800/80",
  sessionActive:
    "relative bg-app-primary-soft dark:bg-blue-950/40 text-app-fg dark:text-slate-100 before:absolute before:left-0 before:top-1.5 before:bottom-1.5 before:w-[3px] before:rounded-full before:bg-app-primary dark:before:bg-blue-400",
  sessionIdle:
    "hover:bg-app-muted dark:hover:bg-slate-800/60 text-app-fg dark:text-slate-200 transition-colors duration-[var(--motion-fast)]",
  sectionLabel:
    "text-[10px] font-semibold uppercase tracking-wider text-app-fg-tertiary",
  /** One overlay language. Callers add z-index. */
  overlay:
    "fixed inset-0 flex items-center justify-center bg-black/45 backdrop-blur-[2px]",
} as const;

type BtnVariant = "primary" | "secondary" | "ghost" | "danger" | "accent";

const btnVariants: Record<BtnVariant, string> = {
  primary:
    "bg-app-primary text-white hover:bg-app-primary-hover disabled:opacity-40 disabled:cursor-not-allowed shadow-sm",
  secondary:
    "border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 text-app-fg dark:text-slate-100 hover:bg-app-muted dark:hover:bg-slate-700",
  ghost:
    "text-app-fg-secondary hover:bg-app-muted dark:hover:bg-slate-800 hover:text-app-fg",
  danger: "bg-app-danger text-white hover:bg-red-600 disabled:opacity-40",
  accent: "bg-app-accent text-white hover:bg-violet-700 disabled:opacity-40 shadow-sm",
};

interface BtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: BtnVariant;
  size?: "sm" | "md" | "icon";
  children: ReactNode;
}

export function Button({
  variant = "primary",
  size = "md",
  className = "",
  children,
  type = "button",
  ...rest
}: BtnProps) {
  const sizes =
    size === "sm"
      ? "px-3 py-1.5 text-xs gap-1"
      : size === "icon"
        ? "p-2.5"
        : "px-3.5 py-2 text-sm gap-1.5";
  return (
    <button
      type={type}
      className={`inline-flex items-center justify-center rounded-xl font-medium transition-colors duration-[var(--motion-fast)] ${btnVariants[variant]} ${sizes} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}

export function EmptyState({
  title,
  description,
  action,
  icon,
  tone = "primary",
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  /** Optional lucide (or any) icon node */
  icon?: ReactNode;
  /** primary = workbench; neutral = no tinted tile */
  tone?: "primary" | "neutral";
}) {
  const tile =
    tone === "primary"
      ? "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300"
      : "bg-app-muted dark:bg-slate-800 text-app-fg-secondary dark:text-slate-400";

  return (
    <div className="flex flex-col items-center justify-center text-center px-6 py-12 max-w-md mx-auto fade-up-in">
      {icon && (
        <div
          className={`mb-4 flex h-12 w-12 items-center justify-center rounded-2xl ${tile}`}
        >
          {icon}
        </div>
      )}
      <p className="text-base font-medium text-app-fg dark:text-slate-100">{title}</p>
      {description && (
        <p className="mt-2 text-sm text-app-fg-secondary dark:text-slate-400 leading-relaxed">
          {description}
        </p>
      )}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}

type InputProps = InputHTMLAttributes<HTMLInputElement>;

export function TextInput({ className = "", ...rest }: InputProps) {
  return <input className={`${ui.input} ${className}`} {...rest} />;
}

export function Chip({
  active,
  onClick,
  children,
  className = "",
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`px-2.5 py-1 rounded-full text-[12px] transition-colors ${
        active
          ? "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 font-medium"
          : "bg-app-muted dark:bg-slate-800 text-app-fg-secondary hover:text-app-fg"
      } ${className}`}
    >
      {children}
    </button>
  );
}
