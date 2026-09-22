import { CalendarDays, ChevronDown, ChevronRight } from "lucide-react";
import { useUiStore } from "../../store/uiStore";

/**
 * 一条折起来的「那天」。
 *
 * 一条工位/项目组的会话要日复一日地往下走，所以它必然越来越长。更早的日子收成
 * 一条横条（`9 月 16 日 · 41 轮`），点开才把那一天取回来——**取回来只是给你看**，
 * 旧账里没有编辑/重发（`docs/spec/projects.md` §4.4）。
 *
 * 四态：收起 / 正在翻 / 展开 / 读不出来。慢可以，装死不行。
 */
export function DayFold({
  label,
  turns,
  open,
  loading,
  failed,
  onToggle,
}: {
  label: string;
  turns: number;
  open: boolean;
  loading: boolean;
  failed: boolean;
  onToggle: () => void;
}) {
  const t = useUiStore((s) => s.t);

  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={open}
      className={`w-full flex items-center gap-2 px-3 py-1.5 mb-5 rounded-lg text-left border transition-colors ${
        failed
          ? "border-app-border dark:border-slate-700 bg-app-surface/50 dark:bg-slate-900/30"
          : "border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800/50 hover:border-app-fg/20"
      }`}
    >
      {open ? (
        <ChevronDown size={13} strokeWidth={1.75} className="shrink-0 text-app-fg-tertiary" />
      ) : (
        <ChevronRight size={13} strokeWidth={1.75} className="shrink-0 text-app-fg-tertiary" />
      )}
      <CalendarDays size={13} strokeWidth={1.75} className="shrink-0 text-app-fg-tertiary" />
      <span className="text-xs text-app-fg-secondary dark:text-slate-400 truncate">
        {label}
      </span>
      <span className="text-app-sub text-app-fg-secondary dark:text-slate-400">
        {loading
          ? t("day.loading")
          : failed
            ? t("day.error")
            : open
              ? t("day.collapse")
              : t("day.turns", { count: turns })}
      </span>
      {failed && !loading && (
        <span className="ml-auto text-app-sub text-app-fg-secondary dark:text-slate-400">
          {t("day.retry")}
        </span>
      )}
    </button>
  );
}
