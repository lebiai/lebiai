import { useEffect, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Brain,
  ListChecks,
  MessageCircle,
  PenLine,
  Search,
  Sparkles,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { markOnboardingDone } from "../../utils/onboarding";
import { Button, ui } from "../common/ui";
import { Select } from "../common/Select";
import { AmbientStage } from "../motion/AmbientStage";
import type { TranslationKey } from "../../i18n";

const SCENES = [
  { tag: "write", icon: PenLine, titleKey: "onboarding.sceneWrite.title" },
  { tag: "think", icon: Brain, titleKey: "onboarding.sceneThink.title" },
  { tag: "research", icon: Search, titleKey: "onboarding.sceneResearch.title" },
  { tag: "track", icon: ListChecks, titleKey: "onboarding.sceneTrack.title" },
  { tag: "other", icon: Sparkles, titleKey: "onboarding.sceneOther.title" },
] as const;

/**
 * First-run ceremony, three screens (产品定死路径):
 *   0 开场（价值 + 「你可以这样说」演示）→ 1 选战场（场景 + 称呼）→ 2 开始干活（CTA 区分）
 * Screen 2 fits on one viewport: compact 5-in-a-row scene picker + name.
 * Collects a minimal seed (name + scenarios) persisted as one pinned memory
 * via `onboarding_seed_set` — only when the user picks the main CTA.
 * No WeChat here (settings only). No engine jargon.
 */
export function OnboardingRitual({ onDone }: { onDone: () => void }) {
  const t = useUiStore((s) => s.t);
  const hasApiKey = useUiStore((s) => s.hasApiKey);
  const setPanel = useNavStore((s) => s.setPanel);
  const [step, setStep] = useState(0);
  const [scenarios, setScenarios] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [exiting, setExiting] = useState(false);
  const [saving, setSaving] = useState(false);
  // Inline model setup (step 2, only when no key yet)
  const [providers, setProviders] = useState<
    { key: string; hasApiKey: boolean }[]
  >([]);
  const [providerKey, setProviderKey] = useState("");
  const [keyInput, setKeyInput] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [keySaved, setKeySaved] = useState(false);

  useEffect(() => {
    void invoke<{
      defaultProvider: string;
      providers: { key: string; hasApiKey: boolean }[];
    }>("get_config")
      .then((c) => {
        setProviders(c.providers);
        setProviderKey(c.defaultProvider);
      })
      .catch(() => undefined);
  }, []);

  const saveKeyInline = async () => {
    if (savingKey || !keyInput.trim()) return;
    setSavingKey(true);
    setKeySaved(false);
    try {
      await invoke("update_config", {
        update: {
          defaultProvider: providerKey,
          apiKey: keyInput.trim(),
        },
      });
      useUiStore.getState().setHasApiKey(true);
      setKeySaved(true);
    } catch (e) {
      console.error("save key failed", e);
    } finally {
      setSavingKey(false);
    }
  };

  /** One primary action: save the key (if provided) then start working. */
  const saveKeyAndStart = async () => {
    if (saving) return;
    if (keyInput.trim()) {
      await saveKeyInline();
    }
    await finishWithSeed(false);
  };

  const toggleScenario = (tag: string) => {
    setScenarios((prev) =>
      prev.includes(tag) ? prev.filter((s) => s !== tag) : [...prev, tag],
    );
  };

  const finish = (goSettings: boolean) => {
    markOnboardingDone();
    setExiting(true);
    window.setTimeout(() => {
      if (goSettings) setPanel("settings");
      onDone();
    }, 220);
  };

  const finishWithSeed = async (goSettings: boolean) => {
    if (saving) return;
    setSaving(true);
    try {
      await invoke("onboarding_seed_set", {
        displayName: name.trim(),
        scenarios,
      });
      // Live-sync so sidebar / welcome / settings greeting show it instantly.
      useUiStore.getState().setDisplayName(name.trim() || null);
    } catch (e) {
      console.error("onboarding_seed_set failed", e);
    }
    setSaving(false);
    finish(goSettings);
  };

  return (
    <div
      className={`fixed inset-0 z-[9500] flex items-center justify-center bg-app-bg dark:bg-slate-950 transition-opacity duration-[var(--motion-base)] ${
        exiting ? "opacity-0" : "opacity-100"
      }`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboarding-title"
    >
      <AmbientStage accent rich className="absolute inset-0" />

      <div className="relative z-[1] w-full max-w-[30rem] px-5 sm:px-6 py-8 fade-up-in">
        <div className="rounded-3xl border border-app-border/90 dark:border-slate-700/80 bg-app-surface/85 dark:bg-slate-900/80 backdrop-blur-md shadow-[0_20px_50px_-20px_rgb(15_23_42/0.28),var(--shadow-app-card)] dark:shadow-[0_24px_60px_-16px_rgb(0_0_0/0.55)] px-6 sm:px-8 pt-7 pb-6 max-h-[94vh] overflow-y-auto">
          <div className="flex flex-col items-center text-center">
            <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-app-accent dark:text-violet-300 mb-2.5">
              {t("onboarding.kicker")}
            </p>
            <h1
              id="onboarding-title"
              className={`font-semibold text-app-fg dark:text-white tracking-tight leading-snug ${
                step === 0
                  ? "text-2xl sm:text-[1.8rem]"
                  : "text-lg sm:text-xl"
              }`}
            >
              {step === 0
                ? t("onboarding.title")
                : step === 1
                  ? t("onboarding.scenesTitle")
                  : t("onboarding.ctaStart")}
            </h1>
            {step === 0 && (
              <p className="mt-3 text-sm text-app-fg-secondary dark:text-slate-400 leading-relaxed max-w-sm">
                {t("onboarding.subtitle")}
              </p>
            )}
          </div>

          <div className="mt-5">
            {/* progress dots */}
            <div className="flex items-center justify-center gap-1.5 mb-5">
              {[0, 1, 2].map((i) => (
                <span
                  key={i}
                  className={`h-1.5 rounded-full transition-all duration-300 ${
                    i === step
                      ? "w-5 bg-app-accent dark:bg-violet-300"
                      : "w-1.5 bg-app-muted dark:bg-slate-700"
                  }`}
                />
              ))}
            </div>

            {step === 0 && (
              <div className="space-y-5 fade-up-in">
                <div className="rounded-2xl border border-app-border/80 dark:border-slate-700/70 bg-app-bg/60 dark:bg-slate-950/50 p-4">
                  <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-app-fg-tertiary mb-2.5">
                    <MessageCircle size={13} className="text-app-accent dark:text-violet-300" />
                    {t("onboarding.demoLabel")}
                  </div>
                  <p className="text-sm text-app-fg dark:text-slate-100 leading-relaxed">
                    {t("onboarding.demoPrompt")}
                  </p>
                </div>
                <p className="text-xs text-app-fg-tertiary dark:text-slate-500 leading-relaxed text-center">
                  {t("onboarding.trustLine")}
                </p>
              </div>
            )}

            {step === 1 && (
              <div className="space-y-4 fade-up-in">
                {/* 5 scenes in one compact row — must fit the viewport */}
                <div className="grid grid-cols-5 gap-2">
                  {SCENES.map(({ tag, icon: Icon, titleKey }) => {
                    const active = scenarios.includes(tag);
                    return (
                      <button
                        key={tag}
                        type="button"
                        onClick={() => toggleScenario(tag)}
                        className={`flex flex-col items-center gap-1.5 rounded-2xl border px-1 py-3 text-center transition-colors duration-[var(--motion-fast)] ${
                          active
                            ? "border-app-accent/70 dark:border-violet-400/70 bg-app-accent-soft dark:bg-violet-950/40"
                            : "border-app-border/80 dark:border-slate-700/70 bg-app-bg/60 dark:bg-slate-950/50 hover:border-app-border dark:hover:border-slate-600"
                        }`}
                      >
                        <Icon
                          size={18}
                          strokeWidth={1.75}
                          className={
                            active
                              ? "text-app-accent dark:text-violet-300"
                              : "text-app-fg-secondary dark:text-slate-400"
                          }
                        />
                        <span className="text-xs font-medium text-app-fg dark:text-slate-100 leading-tight">
                          {t(titleKey)}
                        </span>
                      </button>
                    );
                  })}
                </div>
                <p className="text-[11px] text-app-fg-tertiary dark:text-slate-500 text-center">
                  {t("onboarding.scenesHint")}
                </p>

                <div className="flex items-center gap-3">
                  <label className="shrink-0 text-xs text-app-fg-secondary dark:text-slate-400">
                    {t("onboarding.nameLabel")}
                  </label>
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t("onboarding.namePlaceholder")}
                    className="w-full rounded-xl border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 px-3 py-1.5 text-sm text-app-fg dark:text-slate-100 placeholder:text-app-fg-tertiary focus:outline-none focus:ring-2 focus:ring-app-accent/40"
                  />
                </div>

                {scenarios.length > 0 && (
                  <p className="text-xs text-app-accent dark:text-violet-300 text-center">
                    {t("onboarding.savedFeedback")}
                  </p>
                )}
              </div>
            )}

            {step === 2 && (
              <div className="space-y-4 fade-up-in text-center">
                <p className="text-sm text-app-fg-secondary dark:text-slate-400 leading-relaxed">
                  {scenarios.length > 0 || name.trim()
                    ? t("onboarding.savedFeedback")
                    : t("onboarding.subtitle")}
                </p>

                {hasApiKey === false && (
                  <div className="text-left rounded-2xl border border-app-border/80 dark:border-slate-700/70 bg-app-bg/60 dark:bg-slate-950/50 p-4 space-y-3">
                    <p className="text-sm font-medium text-app-fg dark:text-slate-100">
                      {t("onboarding.keyTitle")}
                    </p>
                    <p className="text-xs text-app-fg-tertiary dark:text-slate-500 leading-relaxed">
                      {t("onboarding.keyHint")}
                    </p>
                    <Select
                      label={t("settings.providerSelect")}
                      value={providerKey}
                      onChange={setProviderKey}
                      options={providers.map((p) => ({
                        value: p.key,
                        label: t(`provider.${p.key}` as TranslationKey),
                      }))}
                    />
                    <input
                      type="password"
                      value={keyInput}
                      onChange={(e) => setKeyInput(e.target.value)}
                      placeholder={t("settings.apiKeyPlaceholderEmpty")}
                      className={ui.input}
                    />
                    {keySaved && (
                      <p className="text-xs text-emerald-700 dark:text-emerald-300">
                        {t("onboarding.keySaved")}
                      </p>
                    )}
                  </div>
                )}

                <div className="flex flex-col gap-2.5 pt-1">
                  {hasApiKey === false ? (
                    <Button
                      variant="primary"
                      className="w-full btn-press"
                      onClick={() => void saveKeyAndStart()}
                      disabled={saving || savingKey || !keyInput.trim()}
                    >
                      {savingKey || saving ? t("settings.saving") : t("onboarding.keySave")}
                    </Button>
                  ) : hasApiKey === null ? (
                    <Button variant="primary" className="w-full" disabled>
                      {t("onboarding.ctaChecking")}
                    </Button>
                  ) : (
                    <Button variant="primary" className="w-full btn-press" onClick={() => finishWithSeed(false)} disabled={saving}>
                      {t("onboarding.ctaStart")}
                    </Button>
                  )}
                  {hasApiKey === false && (
                    <button
                      type="button"
                      disabled={saving}
                      onClick={() => void finishWithSeed(true)}
                      className="text-xs text-app-fg-tertiary hover:text-app-fg-secondary underline underline-offset-2 disabled:opacity-60"
                    >
                      {t("onboarding.ctaKeyHelp")}
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>

          <div className="mt-6 flex items-center justify-between">
            {step > 0 ? (
              <button
                type="button"
                onClick={() => setStep((s) => s - 1)}
                className="inline-flex items-center gap-1 text-xs text-app-fg-tertiary hover:text-app-fg-secondary"
              >
                <ArrowLeft size={13} />
                {t("common.back")}
              </button>
            ) : (
              <span />
            )}
            {step < 2 ? (
              <Button size="sm" onClick={() => setStep((s) => s + 1)}>
                {t("common.continue")}
                <ArrowRight size={13} />
              </Button>
            ) : (
              <span />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
