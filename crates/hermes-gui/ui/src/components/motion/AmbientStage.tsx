import type { ReactNode } from "react";

/**
 * Soft breathing gradients behind empty-home / onboarding heroes.
 * Workbench calm (primary soft) + optional evolution tint (accent soft).
 * `rich` adds a second blob + soft vignette for stage presence (phase D).
 */
export function AmbientStage({
  children,
  className = "",
  accent = false,
  rich = false,
}: {
  children?: ReactNode;
  className?: string;
  /** Extra violet glow for evolution-themed surfaces */
  accent?: boolean;
  /** Deeper stage: dual primary blobs + vignette (empty home / ceremony) */
  rich?: boolean;
}) {
  return (
    <div className={`relative overflow-hidden ${className}`}>
      <div
        className="pointer-events-none absolute inset-0 ambient-breathe motion-safe-only"
        aria-hidden
      >
        <div
          className={`ambient-drift absolute -top-[20%] left-1/2 -translate-x-1/2 rounded-full bg-app-primary-soft/90 dark:bg-blue-950/50 blur-3xl ${
            rich ? "h-[78%] w-[88%]" : "h-[70%] w-[80%]"
          }`}
        />
        {rich && (
          <div className="absolute -bottom-[15%] -left-[10%] h-[55%] w-[60%] rounded-full bg-app-primary-soft/50 dark:bg-blue-950/30 blur-3xl" />
        )}
        {accent && (
          <div className="absolute bottom-0 right-0 h-[52%] w-[58%] rounded-full bg-app-accent-soft/75 dark:bg-violet-950/40 blur-3xl" />
        )}
      </div>
      {rich && (
        <div
          className="pointer-events-none absolute inset-0 ambient-vignette"
          aria-hidden
        />
      )}
      <div className="relative z-[1]">{children}</div>
    </div>
  );
}
