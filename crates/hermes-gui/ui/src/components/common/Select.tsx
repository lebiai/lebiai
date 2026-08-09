import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { ui } from "./ui";

export interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value: string;
  onChange: (v: string) => void;
  options: SelectOption[];
  label?: string;
  className?: string;
  disabled?: boolean;
}

/** Custom dropdown matching the app's card language (native <select> is replaced everywhere). */
export function Select({
  value,
  onChange,
  options,
  label,
  className = "",
  disabled = false,
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onEsc);
    };
  }, []);

  return (
    <div ref={rootRef} className={`relative ${className}`}>
      {label && (
        <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      )}
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={`w-full flex items-center justify-between gap-2 ${ui.input} disabled:opacity-60 disabled:cursor-not-allowed`}
      >
        <span className="truncate text-left">
          {selected?.label ?? options[0]?.label ?? ""}
        </span>
        <ChevronDown
          size={14}
          className={`shrink-0 text-app-fg-tertiary transition-transform duration-[var(--motion-fast)] ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>
      {open && (
        <div className="absolute z-30 mt-1.5 w-full rounded-xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800 shadow-[0_12px_32px_-8px_rgb(15_23_42/0.25)] p-1 max-h-64 overflow-y-auto">
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => {
                onChange(opt.value);
                setOpen(false);
              }}
              className={`w-full flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-sm text-left transition-colors duration-[var(--motion-fast)] ${
                opt.value === value
                  ? "bg-app-primary-soft dark:bg-blue-950/40 text-app-primary dark:text-blue-300 font-medium"
                  : "text-app-fg dark:text-slate-200 hover:bg-app-muted dark:hover:bg-slate-700/60"
              }`}
            >
              <span className="truncate">{opt.label}</span>
              {opt.value === value && <Check size={14} className="shrink-0" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
