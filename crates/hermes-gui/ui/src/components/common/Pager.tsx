import { ChevronLeft, ChevronRight } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button } from "./ui";

/**
 * How many rows one page of a list shows.
 *
 * One rule for every long list in the app — what it knows, what you brought in,
 * what it produced. See `docs/records/20260913-list-readability-tabs-and-paging.md`.
 */
export const PAGE_SIZE = 10;

/** One page out of a list. */
export function pageSlice<T>(items: T[], page: number): T[] {
  const start = (Math.max(1, page) - 1) * PAGE_SIZE;
  return items.slice(start, start + PAGE_SIZE);
}

export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PAGE_SIZE));
}

/** Page controls. Renders nothing when everything already fits on one page. */
export function Pager({
  page,
  total,
  onPage,
}: {
  page: number;
  total: number;
  onPage: (page: number) => void;
}) {
  const t = useUiStore((s) => s.t);
  const pages = pageCount(total);
  if (pages <= 1) return null;
  const current = Math.min(Math.max(1, page), pages);

  return (
    <div className="flex items-center justify-between gap-2 pt-3">
      <span className="text-xs text-app-fg-tertiary tabular-nums">
        {t("pager.summary", { total, page: current, pages })}
      </span>
      <div className="flex items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          disabled={current <= 1}
          onClick={() => onPage(current - 1)}
        >
          <ChevronLeft size={14} />
          {t("pager.prev")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          disabled={current >= pages}
          onClick={() => onPage(current + 1)}
        >
          {t("pager.next")}
          <ChevronRight size={14} />
        </Button>
      </div>
    </div>
  );
}
