import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Save,
  ChevronDown,
  CheckCircle2,
  FolderOpen,
  LayoutDashboard,
  MessageSquare,
  Palette,
  Link2,
  MoreHorizontal,
  ChevronRight,
} from "lucide-react";
import { useUiStore, refreshProviderLabel } from "../../store/uiStore";
import type { Language, TranslationKey } from "../../i18n";
import type { ThemeMode } from "../../utils/theme";
import { Button, ui } from "../common/ui";
import { Select } from "../common/Select";
import { toast } from "../../utils/toast";
import { resetOnboarding } from "../../utils/onboarding";
import { timeGreetingPart } from "../../utils/greeting";
import { WechatConnectCard } from "./WechatConnectCard";
import { ApiKeyHelp } from "./ApiKeyHelp";
import { LicenseSettingsCard } from "../license/LicenseSettingsCard";
import {
  useLicenseStore,
  formatExpiresAt,
  formatRemaining,
} from "../../store/licenseStore";
import {
  useSettingsNavStore,
  type SettingsTab,
} from "../../store/settingsNavStore";
import { StatusRow } from "./StatusRow";
import { VersionStatusRow } from "./VersionStatusRow";
import { useAppUpdate } from "./useAppUpdate";

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

const TABS: {
  id: SettingsTab;
  icon: typeof LayoutDashboard;
  labelKey: TranslationKey;
}[] = [
  { id: "overview", icon: LayoutDashboard, labelKey: "settings.tabOverview" },
  { id: "dialogue", icon: MessageSquare, labelKey: "settings.tabDialogue" },
  { id: "appearance", icon: Palette, labelKey: "settings.tabAppearance" },
  { id: "connections", icon: Link2, labelKey: "settings.tabConnections" },
  { id: "more", icon: MoreHorizontal, labelKey: "settings.tabMore" },
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
  const [modelAdvancedOpen, setModelAdvancedOpen] = useState(false);
  const [displayNameDraft, setDisplayNameDraft] = useState("");
  const [seedScenarios, setSeedScenarios] = useState<string[]>([]);
  const [migrating, setMigrating] = useState(false);
  const [migratedHint, setMigratedHint] = useState(false);
  const [editingKey, setEditingKey] = useState(false);
  const [clearingKey, setClearingKey] = useState(false);
  const [wechatState, setWechatState] = useState<string>("stopped");
  /** Accordion open keys on「更多」 */
  const [moreOpen, setMoreOpen] = useState<Record<string, boolean>>({
    license: true,
    data: false,
    evolve: false,
    tools: false,
    workspace: false,
    ritual: false,
    dev: true,
  });
  const [devTools, setDevTools] = useState(false);
  const [devHasBackup, setDevHasBackup] = useState(false);
  const [devBusy, setDevBusy] = useState(false);
  const refreshLicense = useLicenseStore((s) => s.refresh);

  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTheme = useUiStore((s) => s.setTheme);
  const setHasApiKey = useUiStore((s) => s.setHasApiKey);
  const setDisplayName = useUiStore((s) => s.setDisplayName);
  const displayName = useUiStore((s) => s.displayName);
  const providerLabel = useUiStore((s) => s.providerLabel);
  const requestOnboarding = useUiStore((s) => s.requestOnboarding);

  const tab = useSettingsNavStore((s) => s.tab);
  const setTab = useSettingsNavStore((s) => s.setTab);
  const navRequestId = useSettingsNavStore((s) => s.navRequestId);
  const focus = useSettingsNavStore((s) => s.focus);
  const openTo = useSettingsNavStore((s) => s.openTo);

  const licenseStatus = useLicenseStore((s) => s.status);
  const appUpdate = useAppUpdate(tab === "overview");

  /**
   * Deep-link: openTo("more","license") once → expand accordion only.
   * Do NOT call requestLicenseFocus here (that re-bumps ids and loops with openTo).
   * LicenseSettingsCard watches settingsNav focus for paste expand + scroll.
   */
  useEffect(() => {
    if (navRequestId <= 0) return;
    if (focus === "license") {
      setMoreOpen((m) => ({ ...m, license: true }));
    }
  }, [navRequestId, focus]);

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
    try {
      const w = await invoke<{ state: string }>("wechat_status");
      setWechatState(w.state ?? "stopped");
    } catch {
      setWechatState("stopped");
    }
    try {
      const on = await invoke<boolean>("license_dev_tools_enabled");
      setDevTools(!!on);
      if (on) {
        setDevHasBackup(await invoke<boolean>("license_dev_has_backup"));
      }
    } catch {
      setDevTools(false);
    }
  };

  const devSimulateExpired = async () => {
    setDevBusy(true);
    try {
      await invoke("license_dev_simulate_expired");
      setDevHasBackup(true);
      await refreshLicense();
      toast.success(t("settings.devSimulated"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDevBusy(false);
    }
  };

  const devRestore = async () => {
    setDevBusy(true);
    try {
      await invoke("license_dev_restore_backup");
      setDevHasBackup(await invoke<boolean>("license_dev_has_backup"));
      await refreshLicense();
      toast.success(t("settings.devRestored"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDevBusy(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const update = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm((f) => (f ? { ...f, [k]: v } : f));
    if (k === "uiTheme" && typeof v === "string") {
      setTheme(v);
    }
  };

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
    config?.providers.find((p) => p.key === form?.provider) ??
    config?.providers[0] ??
    null;

  const reloadAfterWrite = async () => {
    const reloaded = await invoke<ConfigView>("get_config");
    setConfig(reloaded);
    setForm(toForm(reloaded));
    setHasApiKey(!!reloaded.hasApiKey);
  };

  const saveInstant = async (patch: Record<string, unknown>) => {
    try {
      await invoke("update_config", { update: patch });
      setConfig((c) => (c ? { ...c, ...patch } : c));
      setSavedAt(Date.now());
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    }
  };

  const saveDisplayNameInstant = async (name: string) => {
    try {
      await invoke("onboarding_seed_set", {
        displayName: name.trim(),
        scenarios: seedScenarios,
      });
      setDisplayName(name.trim() || null);
      setSavedAt(Date.now());
    } catch (e) {
      toast.error(String(e instanceof Error ? e.message : e));
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

  const wechatLabel =
    wechatState === "listening"
      ? t("settings.wechatListening")
      : wechatState === "token_expired"
        ? t("settings.wechatTokenExpired")
        : wechatState === "error"
          ? t("settings.wechatError")
          : t("settings.wechatNotConnected");

  const licenseLine = (() => {
    if (!licenseStatus) return "—";
    if (licenseStatus.phase === "locked") return t("settings.licenseLineExpired");
    if (licenseStatus.onTrial) {
      return t("settings.licenseLineTrial", {
        remaining: formatRemaining(licenseStatus.remainingSecs, t as never),
      });
    }
    return t("settings.licenseLineOk", {
      date: formatExpiresAt(licenseStatus.expiresAt, language),
    });
  })();

  const needKey = !config.hasApiKey;
  const licenseNeedsAttention =
    !!licenseStatus &&
    (licenseStatus.phase === "locked" ||
      licenseStatus.urgency === "expiring" ||
      licenseStatus.onTrial);

  const pageTitle =
    tab === "dialogue"
      ? t("settings.pageDialogue")
      : tab === "appearance"
        ? t("settings.pageAppearance")
        : tab === "connections"
          ? t("settings.pageConnections")
          : tab === "more"
            ? t("settings.pageMore")
            : null;

  return (
    <div className={`flex-1 overflow-y-auto ${ui.page}`}>
      <div className="max-w-2xl mx-auto p-6 space-y-5">
        {/* Tabs */}
        <div className="flex gap-1 flex-wrap border-b border-app-border dark:border-slate-800 pb-2">
          {TABS.map(({ id, icon: Icon, labelKey }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
                tab === id
                  ? "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300"
                  : "text-app-fg-secondary hover:bg-app-muted dark:hover:bg-slate-800"
              }`}
            >
              <Icon size={14} strokeWidth={1.75} />
              {t(labelKey)}
            </button>
          ))}
        </div>

        {pageTitle && (
          <h2 className="text-base font-semibold text-app-fg dark:text-slate-100">
            {pageTitle}
          </h2>
        )}

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

        {/* ── 概览 ─────────────────────────────────────────────── */}
        {tab === "overview" && (
          <div className="space-y-4 fade-up-in">
            <section className={`${ui.card} p-4 space-y-4`}>
              <div className="flex items-start gap-3">
                <div className="h-10 w-10 shrink-0 rounded-full bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 flex items-center justify-center font-semibold">
                  {greetName.slice(0, 1).toUpperCase()}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold text-app-fg dark:text-slate-100 truncate">
                    {t("settings.greetTitle", { name: greetName })}
                  </p>
                  <p className="text-xs text-app-fg-secondary">{t(greetKey)}</p>
                  <p className="text-[11px] text-app-fg-tertiary mt-1">
                    {t("settings.currentModel")}: {providerLabel ?? "—"}
                  </p>
                </div>
              </div>

              <StatusRow
                tone={needKey ? "warn" : "ok"}
                title={needKey ? t("settings.dialogueNeedKey") : t("settings.dialogueReady")}
                subtitle={needKey ? t("settings.dialogueNeedKeyHint") : providerLabel ?? undefined}
                action={
                  needKey ? (
                    <Button size="sm" onClick={() => setTab("dialogue")}>
                      {t("settings.ctaConfigureModel")}
                    </Button>
                  ) : undefined
                }
              />

              <StatusRow
                tone={
                  licenseStatus?.phase === "locked"
                    ? "danger"
                    : licenseNeedsAttention
                      ? "warn"
                      : "ok"
                }
                title={licenseLine}
                action={
                  licenseStatus?.phase === "locked" ||
                  licenseStatus?.urgency === "expiring" ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => openTo("more", "license")}
                    >
                      {t("settings.ctaRenew")}
                    </Button>
                  ) : (
                    <button
                      type="button"
                      className="text-xs text-app-primary hover:underline"
                      onClick={() => openTo("more", "license")}
                    >
                      {t("settings.tabMore")}
                    </button>
                  )
                }
              />

              <StatusRow
                tone={wechatState === "listening" ? "ok" : "neutral"}
                title={t("settings.wechatLine", { state: wechatLabel })}
                action={
                  <button
                    type="button"
                    className="text-xs text-app-primary hover:underline"
                    onClick={() => setTab("connections")}
                  >
                    {t("settings.ctaConnections")}
                  </button>
                }
              />

              <VersionStatusRow
                phase={appUpdate.phase}
                onCheck={() => void appUpdate.inspect()}
                onApply={() => void appUpdate.apply()}
              />
            </section>

            <section className={`${ui.card} divide-y divide-app-border dark:divide-slate-800`}>
              <QuickLink
                label={t("settings.ctaAppearance")}
                onClick={() => setTab("appearance")}
              />
              {!needKey && (
                <QuickLink
                  label={t("settings.tabDialogue")}
                  onClick={() => setTab("dialogue")}
                />
              )}
              <QuickLink
                label={t("settings.tabConnections")}
                onClick={() => setTab("connections")}
              />
            </section>
          </div>
        )}

        {/* ── 对话 ─────────────────────────────────────────────── */}
        {tab === "dialogue" && selectedProvider && (
          <section className={`${ui.card} p-4 space-y-4 fade-up-in`}>
            <p className="text-xs text-app-fg-tertiary leading-relaxed">
              {t("settings.providerHint")}
            </p>
            <p className="text-[11px] text-app-fg-tertiary">{t("settings.savePolicyHint")}</p>

            <SelectField
              label={t("settings.providerSelect")}
              value={form.provider}
              onChange={switchProvider}
              options={config.providers.map((p) => ({
                value: p.key,
                label: t(`provider.${p.key}` as TranslationKey),
              }))}
            />

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
                    onClick={() => void clearKey()}
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
              </>
            )}

            <div>
              <button
                type="button"
                onClick={() => setModelAdvancedOpen((v) => !v)}
                className="flex items-center gap-1.5 text-xs text-app-fg-secondary hover:text-app-fg"
              >
                <ChevronDown
                  size={13}
                  className={`transition-transform ${modelAdvancedOpen ? "rotate-180" : ""}`}
                />
                {t("settings.advanced")}
              </button>
              <p className="text-[11px] text-app-fg-tertiary mt-1">
                {t("settings.modelAdvancedHint")}
              </p>
              {modelAdvancedOpen && (
                <div className="grid grid-cols-2 gap-4 pt-2">
                  <TextField
                    label={t("settings.model")}
                    value={form.model}
                    onChange={(v) => update("model", v)}
                  />
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
              <Button onClick={() => void saveModel()} disabled={saving}>
                <Save size={12} />
                {saving ? t("settings.saving") : t("settings.save")}
              </Button>
            </div>
          </section>
        )}

        {/* ── 外观 ─────────────────────────────────────────────── */}
        {tab === "appearance" && (
          <section className={`${ui.card} p-4 space-y-5 fade-up-in`}>
            <SettingsRow
              label={t("settings.displayName")}
              hint={t("settings.displayNameHint")}
            >
              <input
                value={displayNameDraft}
                onChange={(e) => setDisplayNameDraft(e.target.value)}
                onBlur={() => void saveDisplayNameInstant(displayNameDraft)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    (e.target as HTMLInputElement).blur();
                  }
                }}
                placeholder={t("settings.displayNamePlaceholder")}
                className={`${ui.input} font-normal`}
              />
            </SettingsRow>

            <div className="grid grid-cols-2 gap-4">
              <SelectField
                label={t("settings.language")}
                value={form.uiLanguage}
                onChange={(v) => {
                  update("uiLanguage", v as Language);
                  setLanguage(v);
                  void saveInstant({ uiLanguage: v });
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
                  void saveInstant({ uiTheme: v });
                }}
                options={[
                  { value: "system", label: t("settings.themeSystem") },
                  { value: "light", label: t("settings.themeLight") },
                  { value: "dark", label: t("settings.themeDark") },
                ]}
              />
            </div>
            <p className="text-xs text-app-fg-tertiary">{t("settings.themeHint")}</p>

            <label className="flex items-start gap-2 text-sm text-app-fg dark:text-slate-200 cursor-pointer">
              <input
                type="checkbox"
                className="mt-0.5 rounded border-app-border"
                checked={form.persistThinking}
                onChange={(e) => {
                  const v = e.target.checked;
                  update("persistThinking", v);
                  void saveInstant({ persistThinking: v });
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

        {/* ── 连接 ─────────────────────────────────────────────── */}
        {tab === "connections" && (
          <div className="space-y-4 fade-up-in">
            <p className="text-sm text-app-fg-secondary leading-relaxed">
              {t("settings.connectionsIntro")}
            </p>
            <WechatConnectCard />
          </div>
        )}

        {/* ── 更多 ─────────────────────────────────────────────── */}
        {tab === "more" && (
          <div className="space-y-3 fade-up-in">
            <p className="text-[11px] text-app-fg-tertiary">{t("settings.savePolicyHint")}</p>

            <Accordion
              title={t("settings.sectionLicense")}
              open={moreOpen.license}
              onToggle={() => setMoreOpen((m) => ({ ...m, license: !m.license }))}
            >
              <LicenseSettingsCard />
            </Accordion>

            <Accordion
              title={t("settings.sectionData")}
              open={moreOpen.data}
              onToggle={() => setMoreOpen((m) => ({ ...m, data: !m.data }))}
            >
              <div className="space-y-2 p-1">
                <div className="px-3 py-2 text-sm rounded-xl border border-app-border dark:border-slate-700 bg-app-muted/50 dark:bg-slate-800/60 font-mono break-all">
                  {config.dataDir}
                </div>
                <p className="text-xs text-app-fg-tertiary">{t("settings.dataDirHint")}</p>
                <div className="flex items-center gap-2">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => void pickDataDir()}
                    disabled={migrating}
                  >
                    <FolderOpen size={13} />
                    {t("settings.dataDirChoose")}
                  </Button>
                  <button
                    type="button"
                    onClick={() => void resetDataDir()}
                    className="text-xs text-app-fg-tertiary hover:text-app-fg-secondary underline"
                  >
                    {t("settings.dataDirReset")}
                  </button>
                </div>
                {migrating && (
                  <p className="text-xs text-app-fg-secondary">
                    {t("settings.dataDirMigrating")}
                  </p>
                )}
                {migratedHint && (
                  <p className="text-xs text-emerald-700 dark:text-emerald-300">
                    {t("settings.dataDirRestartHint")}
                  </p>
                )}
              </div>
            </Accordion>

            <Accordion
              title={t("settings.sectionEvolve")}
              open={moreOpen.evolve}
              onToggle={() => setMoreOpen((m) => ({ ...m, evolve: !m.evolve }))}
            >
              <div className="space-y-3 p-1">
                <p className="text-xs text-app-fg-secondary leading-relaxed">
                  {t("settings.evolveQuietHint")}
                </p>
                <TextField
                  label={t("settings.minTurns")}
                  value={form.reflectMinTurns}
                  onChange={(v) => update("reflectMinTurns", v)}
                  type="number"
                />
                <label className="flex items-start gap-2 text-sm text-app-fg cursor-pointer">
                  <input
                    type="checkbox"
                    className="mt-0.5 rounded border-app-border"
                    checked={form.reflectAutoAcceptMemories}
                    onChange={(e) =>
                      update("reflectAutoAcceptMemories", e.target.checked)
                    }
                  />
                  <span>
                    <span className="font-medium">{t("settings.autoAcceptMemories")}</span>
                    <span className="block text-xs text-app-fg-tertiary mt-0.5">
                      {t("settings.autoAcceptHint")}
                    </span>
                  </span>
                </label>
              </div>
            </Accordion>

            <Accordion
              title={t("settings.sectionTools")}
              open={moreOpen.tools}
              onToggle={() => setMoreOpen((m) => ({ ...m, tools: !m.tools }))}
            >
              <div className="space-y-3 p-1">
                <TextField
                  label={t("settings.modelLimit")}
                  value={form.contextModelLimit}
                  onChange={(v) => update("contextModelLimit", v)}
                  type="number"
                />
                <p className="text-[11px] text-app-fg-tertiary">
                  {t("settings.permissionHelp")}
                </p>
                <PermList
                  label={t("settings.allow")}
                  items={form.permissionsAllow}
                  onChange={(items) => update("permissionsAllow", items)}
                  addLabel={t("settings.add")}
                />
                <PermList
                  label={t("settings.deny")}
                  items={form.permissionsDeny}
                  onChange={(items) => update("permissionsDeny", items)}
                  addLabel={t("settings.add")}
                />
              </div>
            </Accordion>

            <Accordion
              title={t("settings.sectionWorkspace")}
              open={moreOpen.workspace}
              onToggle={() =>
                setMoreOpen((m) => ({ ...m, workspace: !m.workspace }))
              }
            >
              <div className="space-y-2 p-1">
                <ReadOnlyField label={t("settings.root")} value={config.workspaceRoot} />
                <p className="text-[11px] text-app-fg-tertiary">
                  {t("settings.workspaceHelp")}
                </p>
              </div>
            </Accordion>

            <Accordion
              title={t("settings.sectionRitual")}
              open={moreOpen.ritual}
              onToggle={() => setMoreOpen((m) => ({ ...m, ritual: !m.ritual }))}
            >
              <div className="space-y-2 p-1">
                <p className="text-xs text-app-fg-tertiary leading-relaxed">
                  {t("settings.replayOnboardingHint")}
                </p>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    resetOnboarding();
                    requestOnboarding();
                    toast.success(t("toast.replayOnboarding"));
                  }}
                >
                  {t("settings.replayOnboardingNow")}
                </Button>
              </div>
            </Accordion>

            {/* Owner-only: debug builds / LEBI_DEV_TOOLS — release packages hide this */}
            {devTools && (
              <Accordion
                title={t("settings.sectionDev")}
                open={moreOpen.dev}
                onToggle={() => setMoreOpen((m) => ({ ...m, dev: !m.dev }))}
              >
                <div className="space-y-3 p-1">
                  <p className="text-xs text-amber-800 dark:text-amber-200/90 leading-relaxed rounded-lg border border-amber-300/60 dark:border-amber-700/50 bg-amber-50/80 dark:bg-amber-950/30 px-2.5 py-2">
                    {t("settings.devHint")}
                  </p>
                  <p className="text-xs text-app-fg-tertiary leading-relaxed">
                    {t("settings.devSimulateHint")}
                  </p>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="danger"
                      disabled={devBusy}
                      onClick={() => void devSimulateExpired()}
                    >
                      {t("settings.devSimulateExpired")}
                    </Button>
                    {devHasBackup && (
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={devBusy}
                        onClick={() => void devRestore()}
                      >
                        {t("settings.devRestore")}
                      </Button>
                    )}
                  </div>
                </div>
              </Accordion>
            )}

            <div className="flex justify-end pt-2">
              <Button onClick={() => void saveAdvanced()} disabled={saving}>
                <Save size={12} />
                {saving ? t("settings.saving") : t("settings.save")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/* ── shared bits ───────────────────────────────────────────────── */

function QuickLink({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full flex items-center justify-between px-4 py-3 text-sm text-app-fg hover:bg-app-muted/60 dark:hover:bg-slate-800/50 transition-colors"
    >
      <span>{label}</span>
      <ChevronRight size={16} className="text-app-fg-tertiary" />
    </button>
  );
}

function Accordion({
  title,
  open,
  onToggle,
  children,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className={`${ui.card} overflow-hidden`}>
      <button
        type="button"
        onClick={onToggle}
        className="w-full flex items-center justify-between px-4 py-3 text-left text-sm font-medium text-app-fg dark:text-slate-100 hover:bg-app-muted/40 dark:hover:bg-slate-800/40"
      >
        <span>{title}</span>
        <ChevronDown
          size={16}
          className={`text-app-fg-tertiary transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && <div className="px-3 pb-3">{children}</div>}
    </div>
  );
}

function SettingsRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <label className="block text-xs text-app-fg-secondary">{label}</label>
      {children}
      {hint && <p className="text-[11px] text-app-fg-tertiary">{hint}</p>}
    </div>
  );
}

function TextField({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
  className = "",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: "text" | "number" | "password";
  className?: string;
}) {
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

function SelectField({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return <Select label={label} value={value} onChange={onChange} options={options} />;
}

function PermList({
  label,
  items,
  onChange,
  addLabel,
}: {
  label: string;
  items: string[];
  onChange: (next: string[]) => void;
  addLabel: string;
}) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const v = draft.trim();
    if (!v || items.includes(v)) return;
    onChange([...items, v]);
    setDraft("");
  };
  return (
    <div className="space-y-1.5">
      <p className="text-xs text-app-fg-secondary">{label}</p>
      {items.length > 0 && (
        <ul className="space-y-1">
          {items.map((item) => (
            <li
              key={item}
              className="flex items-center gap-2 text-[12px] font-mono text-app-fg"
            >
              <span className="flex-1 min-w-0 truncate">{item}</span>
              <button
                type="button"
                className="text-app-fg-tertiary hover:text-app-danger"
                onClick={() => onChange(items.filter((x) => x !== item))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          className={`${ui.input} py-1.5 text-xs font-mono`}
        />
        <Button size="sm" variant="secondary" onClick={add}>
          {addLabel}
        </Button>
      </div>
    </div>
  );
}

function ReadOnlyField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      <div className="px-3 py-2 text-sm rounded-xl border border-app-border dark:border-slate-700 bg-app-muted/50 dark:bg-slate-800/60 font-mono break-all">
        {value}
      </div>
    </div>
  );
}


