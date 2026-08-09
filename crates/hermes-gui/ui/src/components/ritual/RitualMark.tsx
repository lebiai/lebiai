import type { ReactNode } from "react";
import { Sparkles } from "lucide-react";

/** Square icon stage — evolution moments use accent; workbench uses primary. */
export function RitualMark({
  children,
  size = "md",
  tone = "accent",
  className = "",
}: {
  children?: ReactNode;
  size?: "sm" | "md" | "lg";
  tone?: "accent" | "primary";
  className?: string;
}) {
  const box =
    size === "sm"
      ? "h-8 w-8 rounded-lg"
      : size === "lg"
        ? "h-14 w-14 rounded-2xl"
        : "h-12 w-12 rounded-2xl";
  const icon = size === "sm" ? 16 : size === "lg" ? 26 : 22;
  const colors =
    tone === "primary"
      ? "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300"
      : "bg-app-accent-soft dark:bg-violet-950/50 text-app-accent dark:text-violet-300";

  return (
    <div
      className={`inline-flex items-center justify-center ${box} ${colors} ${className}`}
    >
      {children ?? <Sparkles size={icon} />}
    </div>
  );
}
