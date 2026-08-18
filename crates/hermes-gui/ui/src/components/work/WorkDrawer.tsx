import { X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useWorkDrawerStore } from "../../store/workDrawerStore";
import { ZaibanBlock } from "../zaiban/ZaibanBlock";
import { ReviewPane } from "./ReviewPane";

export function WorkDrawer() {
  const t = useUiStore((s) => s.t);
  const { tab, setTab, close } = useWorkDrawerStore();

  return (
    <aside
      className="work-drawer h-full w-[min(22.5rem,42vw)] min-w-[17.5rem] shrink-0 flex flex-col border-l border-app-border/80 dark:border-slate-800 bg-app-surface dark:bg-slate-900 shadow-[-12px_0_40px_-16px_rgb(15_23_42/0.18)]"
      aria-label={t("zaiban.title")}
    >
      <header className="shrink-0 px-4 pt-3.5 pb-2 flex items-center gap-2">
        <div className="flex-1 inline-flex p-0.5 rounded-xl bg-app-muted dark:bg-slate-800/80">
          <Tab
            active={tab === "zaiban"}
            onClick={() => setTab("zaiban")}
            label={t("zaiban.title")}
          />
          <Tab
            active={tab === "review"}
            onClick={() => setTab("review")}
            label={t("review.tab")}
          />
        </div>
        <button
          type="button"
          onClick={close}
          className="p-1.5 rounded-lg text-app-fg-tertiary hover:bg-app-muted dark:hover:bg-slate-800"
          aria-label={t("common.dismiss")}
        >
          <X size={15} />
        </button>
      </header>
      {tab === "zaiban" ? (
        <div className="flex-1 min-h-0 overflow-y-auto">
          <ZaibanBlock />
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex flex-col">
          <ReviewPane />
        </div>
      )}
    </aside>
  );
}

function Tab({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex-1 py-1.5 text-[12.5px] rounded-[10px] transition-colors ${
        active
          ? "bg-app-surface dark:bg-slate-700 text-app-fg dark:text-slate-100 font-medium shadow-sm"
          : "text-app-fg-secondary hover:text-app-fg"
      }`}
    >
      {label}
    </button>
  );
}
