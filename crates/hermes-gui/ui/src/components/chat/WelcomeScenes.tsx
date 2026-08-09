import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Brain, ListChecks, PenLine, Search } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { TranslationKey } from "../../i18n";
import { returnGreetingKey } from "../../utils/greeting";
import { AmbientStage } from "../motion/AmbientStage";
import { MotionCard } from "../motion/MotionCard";
import { RitualMark } from "../ritual/RitualMark";

interface ScenarioDef {
  tag: string;
  icon: typeof PenLine;
  titleKey: TranslationKey;
  descKey: TranslationKey;
  promptKey: TranslationKey;
}

const SCENARIOS: ScenarioDef[] = [
  {
    tag: "write",
    icon: PenLine,
    titleKey: "welcome.sceneWrite.title",
    descKey: "welcome.sceneWrite.desc",
    promptKey: "welcome.sceneWrite.prompt",
  },
  {
    tag: "think",
    icon: Brain,
    titleKey: "welcome.sceneThink.title",
    descKey: "welcome.sceneThink.desc",
    promptKey: "welcome.sceneThink.prompt",
  },
  {
    tag: "research",
    icon: Search,
    titleKey: "welcome.sceneResearch.title",
    descKey: "welcome.sceneResearch.desc",
    promptKey: "welcome.sceneResearch.prompt",
  },
  {
    tag: "track",
    icon: ListChecks,
    titleKey: "welcome.sceneTrack.title",
    descKey: "welcome.sceneTrack.desc",
    promptKey: "welcome.sceneTrack.prompt",
  },
];

/**
 * Empty-session home: work-scenario cards ordered by the user's onboarding
 * seed (selected scenarios first). Single source: `onboarding_seed_get`
 * reads the same pinned memory the engine loads — no UI copy.
 */
export function WelcomeScenes({
  onPick,
  disabled,
}: {
  onPick: (prompt: string) => void;
  disabled?: boolean;
}) {
  const t = useUiStore((s) => s.t);
  const returnKey = returnGreetingKey();
  // Name comes from the global store (synced by onboarding/settings writes);
  // scenarios are still fetched so the seed stays the single source.
  const displayName = useUiStore((s) => s.displayName);
  const [seedScenarios, setSeedScenarios] = useState<string[] | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<{ displayName: string; scenarios: string[] } | null>("onboarding_seed_get")
      .then((seed) => {
        if (!alive) return;
        setSeedScenarios(seed?.scenarios ?? []);
      })
      .catch(() => {
        if (alive) setSeedScenarios([]);
      });
    return () => {
      alive = false;
    };
    // Refetch when the name changes (e.g. right after onboarding finishes) so
    // the scenario order catches up with the just-written seed.
  }, [displayName]);

  const scenarios =
    seedScenarios === null
      ? SCENARIOS
      : [...SCENARIOS].sort((a, b) => {
          const ai = seedScenarios.includes(a.tag) ? 0 : 1;
          const bi = seedScenarios.includes(b.tag) ? 0 : 1;
          return ai - bi;
        });

  return (
    <AmbientStage
      rich
      className="flex flex-col items-center justify-center px-4 py-12 sm:py-14 min-h-[46vh]"
    >
      <div className="max-w-2xl w-full fade-up-in">
        <div className="flex justify-center mb-5">
          <RitualMark size="md" tone="primary" className="ritual-mark-ring-primary" />
        </div>

        {returnKey && (
          <p className="text-center text-[12px] font-medium text-app-primary dark:text-blue-300/90 tracking-wide mb-2">
            {t(returnKey)}
          </p>
        )}

        <h1 className="text-xl sm:text-[1.65rem] font-semibold text-app-fg dark:text-white text-center tracking-tight leading-snug">
          {displayName
            ? t("welcome.titleWithName", { name: displayName })
            : t("welcome.title")}
        </h1>
        <p className="text-sm text-app-fg-secondary dark:text-slate-400 text-center mt-2.5 mb-8 leading-relaxed max-w-lg mx-auto">
          {t("welcome.subtitle")}
        </p>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 stagger-in">
          {scenarios.map(({ icon: Icon, titleKey, descKey, promptKey }) => (
            <MotionCard
              key={titleKey}
              disabled={disabled}
              onClick={() => onPick(t(promptKey))}
              className="p-4 dark:bg-slate-900/90 border-app-border/90 dark:border-slate-700/90"
            >
              <div className="flex items-start gap-3">
                <div className="p-2 rounded-xl bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 shrink-0">
                  <Icon size={17} strokeWidth={1.75} />
                </div>
                <div className="min-w-0 pt-0.5">
                  <div className="text-sm font-medium text-app-fg dark:text-slate-100 mb-1">
                    {t(titleKey)}
                  </div>
                  <div className="text-xs text-app-fg-secondary dark:text-slate-400 leading-relaxed">
                    {t(descKey)}
                  </div>
                </div>
              </div>
            </MotionCard>
          ))}
        </div>

        <p className="text-center text-[11px] text-app-fg-tertiary dark:text-slate-500 mt-7 leading-relaxed">
          {t("welcome.hint")}
        </p>
      </div>
    </AmbientStage>
  );
}
