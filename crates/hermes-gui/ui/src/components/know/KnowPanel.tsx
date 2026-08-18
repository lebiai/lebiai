import { useUiStore } from "../../store/uiStore";
import { useNavStore, type KnowTab } from "../../store/navStore";
import { MemoryPanel } from "../memory/MemoryPanel";
import { MaterialsPanel } from "../materials/MaterialsPanel";
import { SkillPanel } from "../skills/SkillPanel";

/** Continuity + Evolve surface: what it knows about you, and how you taught it. */
export function KnowPanel() {
  const t = useUiStore((s) => s.t);
  const knowTab = useNavStore((s) => s.knowTab);
  const setKnowTab = useNavStore((s) => s.setKnowTab);

  const tabBtn = (tab: KnowTab, label: string) => (
    <button
      key={tab}
      type="button"
      onClick={() => setKnowTab(tab)}
      className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
        knowTab === tab
          ? "bg-app-surface dark:bg-slate-800 text-app-fg dark:text-slate-100 shadow-sm"
          : "text-app-fg-secondary hover:bg-app-muted/80 dark:hover:bg-slate-800/80 hover:text-app-fg"
      }`}
    >
      {label}
    </button>
  );

  return (
    <div className="flex-1 flex flex-col min-h-0 min-w-0">
      <header className="shrink-0 px-5 pt-4 pb-3 border-b border-app-border dark:border-slate-800">
        <h1 className="text-base font-semibold tracking-tight text-app-fg dark:text-slate-100">
          {t("know.title")}
        </h1>
        <p className="mt-0.5 text-xs text-app-fg-tertiary dark:text-slate-500 leading-relaxed max-w-xl">
          {knowTab === "materials" ? t("materials.subtitle") : t("know.subtitle")}
        </p>
        <div className="mt-3 inline-flex gap-0.5 p-0.5 rounded-xl bg-app-muted dark:bg-slate-800/80">
          {tabBtn("you", t("know.tabYou"))}
          {tabBtn("ways", t("know.tabWays"))}
          {tabBtn("materials", t("know.tabMaterials"))}
        </div>
      </header>
      <div className="flex-1 min-h-0 min-w-0 flex flex-col">
        {knowTab === "you" ? (
          <MemoryPanel embedded />
        ) : knowTab === "ways" ? (
          <SkillPanel embedded />
        ) : (
          <MaterialsPanel />
        )}
      </div>
    </div>
  );
}
