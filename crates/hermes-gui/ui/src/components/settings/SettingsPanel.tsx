import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Save,
  Plus,
  X,
  ChevronDown,
  CheckCircle2,
  FolderOpen,
  User,
  Cpu,
  Palette,
  SlidersHorizontal,
} from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { refreshProviderLabel } from "../../store/uiStore";
import type { Language, TranslationKey } from "../../i18n";
import type { ThemeMode } from "../../utils/theme";
import { Button, ui } from "../common/ui";
import { Select } from "../common/Select";
import { toast } from "../../utils/toast";
import { resetOnboarding } from "../../utils/onboarding";
import { timeGreetingPart } from "../../utils/greeting";
import { WechatConnectCard } from "./WechatConnectCard";
import { ApiKeyHelp } from "./ApiKeyHelp";

interface ConfigView {
  defaultProvider: string;
  model: string;
  maxTokens: number;
  baseUrl: string;
  providers: ProviderOption[];
  reflectMinTurns: number;
  reflectAutoAcceptMemories: boolean;
  contextModelLimit: number;
  permissionsAllow: string[];
  permissionsDeny: string[];
  workspaceRoot: string;
  dataDir: string;
  uiLanguage: Language;
  uiTheme: ThemeMode;
  persistThinking: boolean;
  hasApiKey: boolean;
}

interface ProviderOption {
  key: string;
  model: string;
  maxTokens: number;
  baseUrl: string;
  apiKeyMasked: string;
  hasApiKey: boolean;
}

interface Form {
  provider: string;
  model: string;
  maxTokens: string;
  baseUrl: string;
  apiKey: string;
  reflectMinTurns: string;
  reflectAutoAcceptMemories: boolean;
  contextModelLimit: string;
  permissionsAllow: string[];
  permissionsDeny: string[];
  uiLanguage: Language;
  uiTheme: ThemeMode;
  persistThinking: boolean;
}

type SettingsTab = "account" | "model" | "experience" | "advanced";

const TABS: { id: SettingsTab; icon: typeof User; labelKey: TranslationKey }[] = [
  { id: "account", icon: User, labelKey: "settings.tabAccount" },
  { id: "model", icon: Cpu, labelKey: "settings.tabModel" },
  { id: "experience", icon: Palette, labelKey: "settings.tabExperience" },
  { id: "advanced", icon: SlidersHorizontal, labelKey: "settings.tabAdvanced" },
];

function toForm(c: ConfigView): Form {
  return {
    provider: c.defaultProvider,
    model: c.model,
    maxTokens: String(c.maxTokens),
    baseUrl: c.baseUrl,
    apiKey: "",
    reflectMinTurns: String(c.reflectMinTurns),
    reflectAutoAcceptMemories: c.reflectAutoAcceptMemories,
    contextModelLimit: String(c.contextModelLimit),
    permissionsAllow: [...c.permissionsAllow],
    permissionsDeny: [...c.permissionsDeny],
    uiLanguage: c.uiLanguage === "zh-CN" ? "zh-CN" : "en-US",
    uiTheme:
      c.uiTheme === "light" || c.uiTheme === "dark" || c.uiTheme === "system"
        ? c.uiTheme
        : "system",
    persistThinking: !!c.persistThinking,
  };
}

export function SettingsPanel() {
  const [config, setConfig] = useState<ConfigView | null>(null);
  const [form, setForm] = useState<Form | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [tab, setTab] = useState<SettingsTab>("account");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  // Account: display name
  const [displayNameDraft, setDisplayNameDraft] = useState("");
  const [seedScenarios, setSeedScenarios] = useState<string[]>([]);
  const [nameSaving, setNameSaving] = useState(false);
  // Account: data location
  const [migrating, setMigrating] = useState(false);
  const [migratedHint, setMigratedHint] = useState(false);
  // Model: key editing state
  const [editingKey, setEditingKey] = useState(false);
  const [clearingKey, setClearingKey] = useState(false);

  const t = useUiStore((s) => s.t);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTheme = useUiStore((s) => s.setTheme);
  const setHasApiKey = useUiStore((s) => s.setHasApiKey);
  const setDisplayName = useUiStore((s) => s.setDisplayName);
  const displayName = useUiStore((s) => s.displayName);
  const providerLabel = useUiStore((s) => s.providerLabel);
  const requestOnboarding = useUiStore((s) => s.requestOnboarding);

  const load = async () => {
    try {
      const c = await invoke<ConfigView>("get_config");
      setConfig(c);
      setForm(toForm(c));
      setLanguage(c.uiLanguage);
      setTheme(c.uiTheme ?? "system");
      setHasApiKey(!!c.hasApiKey);
      setError(null);
      void refreshProviderLabel();
    } catch (e) {
      setError(String(e));
    }
    try {
      const seed = await invoke<{ displayName: string; scenarios: string[] } | null>(
        "onboarding_seed_get",
      );
      setDisplayNameDraft(seed?.displayName ?? "");
      setSeedScenarios(seed?.scenarios ?? []);
      setDisplayName(seed?.displayName?.trim() || null);
    } catch {
      /* non-fatal */
    }
  };

  useEffect(() => {
    load();
  }, []);

  const update = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm((f) => (f ? { ...f, [k]: v } : f));
    if (k === "uiTheme" && typeof v === "string") {
      setTheme(v);
    }
  };

  /** Selecting a provider pre-fills its preset values; the user only types
   *  the API key. Per-provider on-disk values (custom model, key) are kept. */
  const switchProvider = (key: string) => {
    const opt = config?.providers.find((p) => p.key === key);
    if (!opt) return;
    setForm((f) =>
      f
        ? {
            ...f,
            provider: key,
            model: opt.model,
            maxTokens: String(opt.maxTokens),
            baseUrl: opt.baseUrl,
            apiKey: "",
          }
        : f
    );
    setEditingKey(false);
  };

  const selectedProvider =
    config?.providers.find((p) => p.key === form?.provider) ?? config?.providers[0] ?? null;

  const reloadAfterWrite = async () => {
    const reloaded = await invoke<ConfigView>("get_config");
    setConfig(reloaded);
    setForm(toForm(reloaded));
    setHasApiKey(!!reloaded.hasApiKey);
  };

  /** Experience settings save the instant they change — no save button. */
  const saveExperienceField = async (patch: Record<string, unknown>) => {
    try {
      await invoke("update_config", { update: patch });
      setConfig((c) => (c ? { ...c, ...patch } : c));
      setSavedAt(Date.now());
      toast.success(t("toast.settingsSaved"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    }
  };

  const saveModel = async () => {
    if (!form) return;
    setSaving(true);
    setError(null);
    try {
      const maxTokens = Number(form.maxTokens);
      if (!Number.isFinite(maxTokens) || maxTokens <= 0) {
        throw new Error(t("settings.error.maxTokens"));
      }
      await invoke("update_config", {
        update: {
          defaultProvider: form.provider,
          model: form.model,
          maxTokens,
          baseUrl: form.baseUrl,
          apiKey: form.apiKey.trim() ? form.apiKey : null,
        },
      });
      await reloadAfterWrite();
      void refreshProviderLabel();
      setSavedAt(Date.now());
      setEditingKey(false);
      toast.success(t("toast.settingsSaved"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    } finally {
      setSaving(false);
    }
  };

  const saveAdvanced = async () => {
    if (!form) return;
    setSaving(true);
    setError(null);
    try {
      const minTurns = Number(form.reflectMinTurns);
      const ctxLimit = Number(form.contextModelLimit);
      if (!Number.isFinite(minTurns) || minTurns < 0) {
        throw new Error(t("settings.error.minTurns"));
      }
      if (!Number.isFinite(ctxLimit) || ctxLimit <= 0) {
        throw new Error(t("settings.error.contextLimit"));
      }
      await invoke("update_config", {
        update: {
          reflectMinTurns: minTurns,
          reflectAutoAcceptMemories: form.reflectAutoAcceptMemories,
          contextModelLimit: ctxLimit,
          permissionsAllow: form.permissionsAllow,
          permissionsDeny: form.permissionsDeny,
        },
      });
      await reloadAfterWrite();
      setSavedAt(Date.now());
      toast.success(t("toast.settingsSaved"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    } finally {
      setSaving(false);
    }
  };

  const saveDisplayName = async () => {
    setNameSaving(true);
    try {
      await invoke("onboarding_seed_set", {
        displayName: displayNameDraft.trim(),
        scenarios: seedScenarios,
      });
      setDisplayName(displayNameDraft.trim() || null);
      toast.success(t("toast.displayNameSaved"));
    } catch (e) {
      toast.error(String(e instanceof Error ? e.message : e));
    } finally {
      setNameSaving(false);
    }
  };

  const clearKey = async () => {
    if (!form) return;
    setClearingKey(true);
    setError(null);
    try {
      await invoke("update_config", {
        update: { defaultProvider: form.provider, clearApiKey: true },
      });
      await reloadAfterWrite();
      void refreshProviderLabel();
      setEditingKey(false);
      toast.success(t("toast.keyCleared"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    } finally {
      setClearingKey(false);
    }
  };

  const pickDataDir = async () => {
    setMigrating(true);
    setError(null);
    setMigratedHint(false);
    try {
      const picked = await invoke<string | null>("data_dir_pick");
      if (!picked) return;
      const view = await invoke<{ dataRoot: string }>("data_dir_migrate", {
        target: picked,
      });
      setConfig((c) => (c ? { ...c, dataDir: view.dataRoot } : c));
      setMigratedHint(true);
      toast.success(t("toast.dataDirMigrated"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    } finally {
      setMigrating(false);
    }
  };

  const resetDataDir = async () => {
    try {
      await invoke("data_dir_reset");
      await load();
      toast.success(t("toast.dataDirMigrated"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    }
  };

  if (error && !config) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-app-danger">{error}</p>
      </div>
    );
  }
  if (!config || !form) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-app-fg-secondary">{t("settings.loading")}</p>
      </div>
    );
  }

  const part = timeGreetingPart();
  const greetKey: TranslationKey =
    part === "morning"
      ? "settings.greetMorning"
      : part === "noon"
        ? "settings.greetNoon"
        : part === "afternoon"
          ? "settings.greetAfternoon"
          : part === "evening"
            ? "settings.greetEvening"
            : "settings.greetLate";
  const greetName = displayName?.trim() || t("settings.guestName");

  return (
    <div className={`flex-1 overflow-y-auto ${ui.page}`}>
      <div className="max-w-2xl mx-auto p-6 space-y-5">
        {/* ── 问候条 ────────────────────────────────────────────── */}
        <div className={`${ui.card} p-4 flex items-center gap-3`}>
          <div className="h-10 w-10 shrink-0 rounded-full bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 flex items-center justify-center font-semibold">
            {greetName.slice(0, 1).toUpperCase()}
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold text-app-fg dark:text-slate-100 truncate">
              {t("settings.greetTitle", { name: greetName })}
            </p>
            <p className="text-xs text-app-fg-secondary dark:text-slate-400">{t(greetKey)}</p>
          </div>
          <div className="shrink-0 text-right">
            <p className="text-[11px] text-app-fg-tertiary">{t("settings.currentModel")}</p>
            <p className="text-xs font-medium text-app-fg dark:text-slate-200 truncate max-w-40">
              {providerLabel ?? "—"}
            </p>
          </div>
        </div>

        {/* ── 分组 tab ──────────────────────────────────────────── */}
        <div className="flex gap-1.5 flex-wrap">
          {TABS.map(({ id, icon: Icon, labelKey }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={`inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-sm font-medium transition-colors duration-[var(--motion-fast)] ${
                tab === id
                  ? "bg-app-primary text-white shadow-sm"
                  : "text-app-fg-secondary hover:bg-app-muted dark:hover:bg-slate-800"
              }`}
            >
              <Icon size={14} strokeWidth={1.75} />
              {t(labelKey)}
            </button>
          ))}
        </div>

        {error && (
          <p className="text-sm text-red-600 dark:text-red-300 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded-xl px-3 py-2">
            {error}
          </p>
        )}
        {savedAt && !error && (
          <p className="text-xs text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/30 border border-emerald-200 dark:border-emerald-800 rounded-xl px-3 py-2">
            {t("settings.saved")}
          </p>
        )}

        {/* ── 账户 ──────────────────────────────────────────────── */}
        {tab === "account" && (
          <div className="space-y-4 fade-up-in">
            <section className={`${ui.card} p-4 space-y-4`}>
              <div className="flex items-center gap-2">
                <User size={15} className="text-app-fg-tertiary" />
                <h3 className="text-sm font-semibold text-app-fg dark:text-slate-100">
                  {t("settings.account")}
                </h3>
              </div>

              <div>
                <label className="block text-xs text-app-fg-secondary mb-1">
                  {t("settings.displayName")}
                </label>
                <div className="flex items-center gap-2">
                  <input
                    value={displayNameDraft}
                    onChange={(e) => setDisplayNameDraft(e.target.value)}
                    placeholder={
                      displayNameDraft
                        ? undefined
                        : t("settings.displayNamePlaceholder")
                    }
                    className={`${ui.input} font-normal`}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={saveDisplayName}
                    disabled={nameSaving}
                  >
                    {nameSaving ? t("settings.saving") : t("settings.save")}
                  </Button>
                </div>
                <p className="text-xs text-app-fg-tertiary mt-1.5">
                  {t("settings.displayNameHint")}
                </p>
              </div>

              <div className="space-y-2">
                <label className="block text-xs text-app-fg-secondary">
                  {t("settings.dataLocation")}
                </label>
                <div className="px-3 py-2 text-sm rounded-xl border border-app-border dark:border-slate-700 bg-app-muted/50 dark:bg-slate-800/60 font-mono break-all text-app-fg dark:text-slate-200">
                  {config.dataDir}
                </div>
                <p className="text-xs text-app-fg-tertiary leading-relaxed">
                  {t("settings.dataDirHint")}
                </p>
                <div className="flex items-center gap-2 pt-1">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={pickDataDir}
                    disabled={migrating}
                  >
                    <FolderOpen size={13} />
                    {t("settings.dataDirChoose")}
                  </Button>
                </div>
                {migrating && (
                  <p className="text-xs text-app-fg-secondary">{t("settings.dataDirMigrating")}</p>
                )}
                {migratedHint && (
                  <p className="text-xs text-emerald-700 dark:text-emerald-300">
                    {t("settings.dataDirRestartHint")}
                  </p>
                )}
                <button
                  type="button"
                  onClick={resetDataDir}
                  className="text-xs text-app-fg-tertiary hover:text-app-fg-secondary underline underline-offset-2"
                >
                  {t("settings.dataDirReset")}
                </button>
              </div>
            </section>

            <WechatConnectCard />
          </div>
        )}

        {/* ── 模型服务 ──────────────────────────────────────────── */}
        {tab === "model" && selectedProvider && (
          <section className={`${ui.card} p-4 space-y-4 fade-up-in`}>
            <div className="flex items-center gap-2">
              <Cpu size={15} className="text-app-fg-tertiary" />
              <h3 className="text-sm font-semibold text-app-fg dark:text-slate-100">
                {t("settings.provider")}
              </h3>
            </div>

            <SelectField
              label={t("settings.providerSelect")}
              value={form.provider}
              onChange={switchProvider}
              options={config.providers.map((p) => ({
                value: p.key,
                label: t(`provider.${p.key}` as TranslationKey),
              }))}
            />
            <p className="text-xs text-app-fg-tertiary">{t("settings.providerHint")}</p>

            {selectedProvider.hasApiKey && !editingKey ? (
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-emerald-200 dark:border-emerald-800/70 bg-emerald-50/60 dark:bg-emerald-950/30 px-3 py-2.5">
                <span className="inline-flex items-center gap-1.5 text-xs font-medium text-emerald-700 dark:text-emerald-300">
                  <CheckCircle2 size={14} />
                  {t("settings.keyConfigured")}
                </span>
                <span className="text-xs font-mono text-app-fg-secondary dark:text-slate-400">
                  {t("settings.apiKeyMasked", { key: selectedProvider.apiKeyMasked })}
                </span>
                <span className="flex items-center gap-2 ml-auto">
                  <button
                    type="button"
                    onClick={() => setEditingKey(true)}
                    className="text-xs text-app-primary dark:text-blue-300 hover:underline"
                  >
                    {t("settings.changeKey")}
                  </button>
                  <button
                    type="button"
                    onClick={clearKey}
                    disabled={clearingKey}
                    className="text-xs text-app-danger hover:underline disabled:opacity-50"
                  >
                    {t("settings.clearKey")}
                  </button>
                </span>
              </div>
            ) : (
              <>
                <TextField
                  label={t("settings.apiKey")}
                  value={form.apiKey}
                  onChange={(v) => update("apiKey", v)}
                  placeholder={
                    selectedProvider.hasApiKey
                      ? t("settings.apiKeyPlaceholder", {
                          key: selectedProvider.apiKeyMasked,
                        })
                      : t("settings.apiKeyPlaceholderEmpty")
                  }
                  type="password"
                />
                {!selectedProvider.hasApiKey && (
                  <p className="text-xs text-amber-700 dark:text-amber-300">
                    {t("settings.apiKeyMissing", {
                      name: t(`provider.${selectedProvider.key}` as TranslationKey),
                    })}
                  </p>
                )}
                {!selectedProvider.hasApiKey && (
                  <ApiKeyHelp provider={selectedProvider.key} />
                )}
                {selectedProvider.hasApiKey && editingKey && (
                  <p className="text-xs text-app-fg-tertiary">
                    {t("settings.apiKeyHelpStep3")}
                  </p>
                )}
              </>
            )}

            <div>
              <button
                type="button"
                onClick={() => setAdvancedOpen((v) => !v)}
                className="flex items-center gap-1.5 text-xs text-app-fg-secondary hover:text-app-fg"
              >
                <ChevronDown
                  size={13}
                  className={`transition-transform duration-[var(--motion-fast)] ${
                    advancedOpen ? "rotate-180" : ""
                  }`}
                />
                {t("settings.advanced")}
              </button>
              {advancedOpen && (
                <div className="grid grid-cols-2 gap-4 pt-2">
                  <TextField label={t("settings.model")} value={form.model} onChange={(v) => update("model", v)} />
                  <TextField
                    label={t("settings.maxTokens")}
                    value={form.maxTokens}
                    onChange={(v) => update("maxTokens", v)}
                    type="number"
                  />
                  <TextField
                    label={t("settings.baseUrl")}
                    value={form.baseUrl}
                    onChange={(v) => update("baseUrl", v)}
                    className="col-span-2"
                  />
                </div>
              )}
            </div>

            <div className="flex justify-end pt-1">
              <Button onClick={saveModel} disabled={saving}>
                <Save size={12} />
                {saving ? t("settings.saving") : t("settings.save")}
              </Button>
            </div>
          </section>
        )}

        {/* ── 体验 ──────────────────────────────────────────────── */}
        {tab === "experience" && (
          <section className={`${ui.card} p-4 space-y-4 fade-up-in`}>
            <div className="flex items-center gap-2">
              <Palette size={15} className="text-app-fg-tertiary" />
              <h3 className="text-sm font-semibold text-app-fg dark:text-slate-100">
                {t("settings.interface")}
              </h3>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <SelectField
                label={t("settings.language")}
                value={form.uiLanguage}
                onChange={(v) => {
                  update("uiLanguage", v as Language);
                  setLanguage(v);
                  void saveExperienceField({ uiLanguage: v });
                }}
                options={[
                  { value: "en-US", label: "English" },
                  { value: "zh-CN", label: "简体中文" },
                ]}
              />
              <SelectField
                label={t("settings.theme")}
                value={form.uiTheme}
                onChange={(v) => {
                  update("uiTheme", v as ThemeMode);
                  setTheme(v);
                  void saveExperienceField({ uiTheme: v });
                }}
                options={[
                  { value: "system", label: t("settings.themeSystem") },
                  { value: "light", label: t("settings.themeLight") },
                  { value: "dark", label: t("settings.themeDark") },
                ]}
              />
            </div>
            <p className="text-xs text-app-fg-tertiary">{t("settings.languageHint")}</p>
            <p className="text-xs text-app-fg-tertiary">{t("settings.themeHint")}</p>
            <label className="flex items-start gap-2 text-sm text-app-fg dark:text-slate-200 pt-1 cursor-pointer">
              <input
                type="checkbox"
                className="mt-0.5 rounded border-app-border"
                checked={form.persistThinking}
                onChange={(e) => {
                  const v = e.target.checked;
                  update("persistThinking", v);
                  void saveExperienceField({ persistThinking: v });
                }}
              />
              <span>
                <span className="font-medium">{t("settings.persistThinking")}</span>
                <span className="block text-xs text-app-fg-tertiary mt-0.5">
                  {t("settings.persistThinkingHint")}
                </span>
              </span>
            </label>
          </section>
        )}

        {/* ── 高级 ──────────────────────────────────────────────── */}
        {tab === "advanced" && (
          <section className={`${ui.card} p-4 space-y-5 fade-up-in`}>
            <div className="flex items-center gap-2">
              <SlidersHorizontal size={15} className="text-app-fg-tertiary" />
              <h3 className="text-sm font-semibold text-app-fg dark:text-slate-100">
                {t("settings.advancedTitle")}
              </h3>
            </div>

            <div className="space-y-3">
              <h4 className="text-xs font-medium text-app-fg-secondary">
                {t("settings.ritual")}
              </h4>
              <p className="text-xs text-app-fg-tertiary leading-relaxed">
                {t("settings.replayOnboardingHint")}
              </p>
              <Button
                size="sm"
                variant="accent"
                onClick={() => {
                  resetOnboarding();
                  requestOnboarding();
                  toast.success(t("toast.replayOnboarding"));
                }}
              >
                {t("settings.replayOnboardingNow")}
              </Button>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <TextField
                label={t("settings.minTurns")}
                value={form.reflectMinTurns}
                onChange={(v) => update("reflectMinTurns", v)}
                type="number"
              />
              <div className="flex items-end pb-1">
                <label className="flex items-center gap-2 text-sm text-app-fg dark:text-slate-200 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={form.reflectAutoAcceptMemories}
                    onChange={(e) =>
                      update("reflectAutoAcceptMemories", e.target.checked)
                    }
                    className="rounded border-app-border"
                  />
                  {t("settings.autoAcceptMemories")}
                </label>
              </div>
            </div>

            <TextField
              label={t("settings.modelLimit")}
              value={form.contextModelLimit}
              onChange={(v) => update("contextModelLimit", v)}
              type="number"
            />

            <div className="space-y-2">
              <p className="text-xs text-app-fg-secondary">
                {t("settings.toolPermissions")}
              </p>
              <p className="text-xs text-app-fg-tertiary">{t("settings.permissionHelp")}</p>
              <RuleList
                label={t("settings.allow")}
                tone="green"
                rules={form.permissionsAllow}
                onChange={(rules) => update("permissionsAllow", rules)}
              />
              <RuleList
                label={t("settings.deny")}
                tone="red"
                rules={form.permissionsDeny}
                onChange={(rules) => update("permissionsDeny", rules)}
              />
            </div>

            <div className="space-y-2">
              <p className="text-xs text-app-fg-secondary">{t("settings.workspace")}</p>
              <ReadOnlyField label={t("settings.root")} value={config.workspaceRoot} />
              <p className="text-[11px] text-app-fg-tertiary">{t("settings.workspaceHelp")}</p>
            </div>

            <div className="flex justify-end pt-1">
              <Button onClick={saveAdvanced} disabled={saving}>
                <Save size={12} />
                {saving ? t("settings.saving") : t("settings.save")}
              </Button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

interface TextFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: "text" | "number" | "password";
  className?: string;
}

function TextField({ label, value, onChange, placeholder, type = "text", className = "" }: TextFieldProps) {
  return (
    <div className={className}>
      <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={`${ui.input} font-mono`}
      />
    </div>
  );
}

interface SelectFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}

function SelectField({ label, value, onChange, options }: SelectFieldProps) {
  return <Select label={label} value={value} onChange={onChange} options={options} />;
}

function ReadOnlyField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      <div className="px-3 py-2 text-sm rounded-xl border border-app-border dark:border-slate-700 bg-app-muted/50 dark:bg-slate-800/60 font-mono break-all text-app-fg dark:text-slate-200">
        {value}
      </div>
    </div>
  );
}

interface RuleListProps {
  label: string;
  tone: "green" | "red";
  rules: string[];
  onChange: (rules: string[]) => void;
}

function RuleList({ label, tone, rules, onChange }: RuleListProps) {
  const [draft, setDraft] = useState("");
  const colors =
    tone === "green"
      ? "bg-emerald-100 dark:bg-emerald-900/40 text-emerald-800 dark:text-emerald-300"
      : "bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300";

  const add = () => {
    const v = draft.trim();
    if (!v) return;
    if (rules.includes(v)) {
      setDraft("");
      return;
    }
    onChange([...rules, v]);
    setDraft("");
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <span className={`text-xs px-2 py-0.5 rounded-full ${colors}`}>{label}</span>
        <div className="flex-1" />
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          className={`${ui.input} h-8 text-xs`}
        />
        <Button size="sm" variant="secondary" onClick={add}>
          <Plus size={12} />
        </Button>
      </div>
      {rules.map((rule) => (
        <div
          key={rule}
          className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-app-muted/50 dark:bg-slate-800/60 text-xs font-mono"
        >
          <span className="flex-1 truncate">{rule}</span>
          <button
            type="button"
            onClick={() => onChange(rules.filter((r) => r !== rule))}
            className="text-app-fg-tertiary hover:text-app-danger"
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
