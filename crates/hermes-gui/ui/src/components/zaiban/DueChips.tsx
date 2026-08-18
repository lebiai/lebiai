import { useState } from "react";
import { useUiStore } from "../../store/uiStore";
import { Chip, ui } from "../common/ui";

export const DUE_PRESETS = ["今天", "这周", "下周"] as const;

export function DueChips({
  value,
  onChange,
}: {
  value: string;
  onChange: (phrase: string) => void;
}) {
  const t = useUiStore((s) => s.t);
  const [custom, setCustom] = useState(false);
  const preset = DUE_PRESETS.includes(value as (typeof DUE_PRESETS)[number]);

  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap gap-1.5">
        {DUE_PRESETS.map((p) => (
          <Chip
            key={p}
            active={value === p && !custom}
            onClick={() => {
              setCustom(false);
              onChange(p);
            }}
          >
            {p === "今天"
              ? t("zaiban.dueTodayChip")
              : p === "这周"
                ? t("zaiban.dueWeekChip")
                : t("zaiban.dueNextChip")}
          </Chip>
        ))}
        <button
          type="button"
          onClick={() => {
            setCustom(true);
            if (preset || !value) onChange("");
          }}
          className="px-2.5 py-1 text-[12px] text-app-primary dark:text-blue-300"
        >
          {t("zaiban.duePick")}
        </button>
      </div>
      {custom && (
        <input
          type="date"
          value={/^\d{4}-\d{2}-\d{2}$/.test(value) ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          className={`${ui.input} py-1.5 text-[12px]`}
        />
      )}
    </div>
  );
}

export function dueLabel(
  item: { softDue?: string | null; dueDate?: string | null; overdue: boolean; dueToday?: boolean },
  t: (k: "zaiban.dueToday" | "zaiban.overdueDue", p?: { due: string }) => string
): string {
  if (item.overdue) {
    return t("zaiban.overdueDue", { due: item.softDue || item.dueDate || "" });
  }
  if (item.dueToday) return t("zaiban.dueToday");
  return item.softDue || item.dueDate || "";
}
