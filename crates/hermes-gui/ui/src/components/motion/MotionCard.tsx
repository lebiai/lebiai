import type { ButtonHTMLAttributes, ReactNode } from "react";
import { ui } from "../common/ui";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode;
  /** Use accent (evolution) hover border instead of primary */
  accent?: boolean;
};

/** Unified interactive card: token surface + lift hover. Prefer over ad-hoc borders. */
export function MotionCard({
  children,
  className = "p-3.5",
  accent = false,
  type = "button",
  ...rest
}: Props) {
  const hover = accent
    ? "hover:border-app-accent/45 dark:hover:border-violet-500/45 hover:bg-app-accent-soft/40 dark:hover:bg-violet-950/25"
    : "hover:border-app-primary/50 dark:hover:border-blue-500/50 hover:bg-app-primary-soft/50 dark:hover:bg-blue-950/30";

  return (
    <button
      type={type}
      className={`${ui.card} motion-lift text-left disabled:opacity-50 disabled:pointer-events-none ${hover} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
