import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Save, RefreshCw, Plus, X, ChevronDown } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { Language, TranslationKey } from "../../i18n";
import type { ThemeMode } from "../../utils/theme";
import { Button, ui } from "../common/ui";
import { toast } from "../../utils/toast";
import { resetOnboarding } from "../../utils/onboarding";
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
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const t = useUiStore((s) => s.t);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTheme = useUiStore((s) => s.setTheme);
  const setHasApiKey = useUiStore((s) => s.setHasApiKey);
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
    } catch (e) {
      setError(String(e));
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
  };

  const selectedProvider =
    config?.providers.find((p) => p.key === form?.provider) ?? config?.providers[0] ?? null;

  const save = async () => {
    if (!form) return;
    setSaving(true);
    setError(null);
    try {
      const maxTokens = Number(form.maxTokens);
      const minTurns = Number(form.reflectMinTurns);
      const ctxLimit = Number(form.contextModelLimit);
      if (!Number.isFinite(maxTokens) || maxTokens <= 0) {
        throw new Error(t("settings.error.maxTokens"));
      }
      if (!Number.isFinite(minTurns) || minTurns < 0) {
        throw new Error(t("settings.error.minTurns"));
      }
      if (!Number.isFinite(ctxLimit) || ctxLimit <= 0) {
        throw new Error(t("settings.error.contextLimit"));
      }
      // Explicit boolean so IPC never drops the field.
      const persistThinking = form.persistThinking === true;
      await invoke("update_config", {
        update: {
          defaultProvider: form.provider,
          model: form.model,
          maxTokens,
          baseUrl: form.baseUrl,
          apiKey: form.apiKey.trim() ? form.apiKey : null,
          reflectMinTurns: minTurns,
          reflectAutoAcceptMemories: form.reflectAutoAcceptMemories,
          contextModelLimit: ctxLimit,
          permissionsAllow: form.permissionsAllow,
          permissionsDeny: form.permissionsDeny,
          uiLanguage: form.uiLanguage,
          uiTheme: form.uiTheme,
          persistThinking,
        },
      });
      setSavedAt(Date.now());
      setLanguage(form.uiLanguage);
      setTheme(form.uiTheme);
      // Re-read from disk so checkbox reflects what was actually written.
      const reloaded = await invoke<ConfigView>("get_config");
      setConfig(reloaded);
      setForm(toForm(reloaded));
      setHasApiKey(!!reloaded.hasApiKey);
      toast.success(t("toast.settingsSaved"));
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      setError(msg);
      toast.error(msg);
    } finally {
      setSaving(false);
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

  return (
    <div className={`flex-1 overflow-y-auto ${ui.page}`}>
      <div className="max-w-2xl mx-auto p-6 space-y-6">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold text-app-fg dark:text-slate-100">
            {t("settings.title")}
          </h2>
          <div className="flex items-center gap-2">
            <Button size="sm" variant="secondary" onClick={load} title={t("settings.reloadTitle")}>
              <RefreshCw size={12} />
              {t("settings.reload")}
            </Button>
            <Button size="sm" onClick={save} disabled={saving}>
              <Save size={12} />
              {saving ? t("settings.saving") : t("settings.save")}
            </Button>
          </div>
        </div>

        {error && (
          <p className="text-sm text-red-600 dark:text-red-300 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded-xl px-3 py-2">
            {error}
          </p>
        )}
        {savedAt && !error && (
          <p className="text-xs text-amber-800 dark:text-amber-200 bg-amber-50 dark:bg-amber-900/30 border border-amber-200 dark:border-amber-800 rounded-xl px-3 py-2">
            {t("settings.saved")}
          </p>
        )}

        <WechatConnectCard />

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.interface")}
          </h3>
          <div className="grid grid-cols-2 gap-4">
            <SelectField
              label={t("settings.language")}
              value={form.uiLanguage}
              onChange={(v) => update("uiLanguage", v as Language)}
              options={[
                { value: "en-US", label: "English" },
                { value: "zh-CN", label: "简体中文" },
              ]}
            />
            <SelectField
              label={t("settings.theme")}
              value={form.uiTheme}
              onChange={(v) => update("uiTheme", v as ThemeMode)}
              options={[
                { value: "system", label: t("settings.themeSystem") },
                { value: "light", label: t("settings.themeLight") },
                { value: "dark", label: t("settings.themeDark") },
              ]}
            />
          </div>
          <p className="text-xs text-app-fg-tertiary">{t("settings.languageHint")}</p>
          <p className="text-xs text-app-fg-tertiary">{t("settings.themeHint")}</p>
          <label className="flex items-start gap-2 text-sm text-app-fg dark:text-slate-200 pt-1">
            <input
              type="checkbox"
              className="mt-0.5 rounded border-app-border"
              checked={form.persistThinking}
              onChange={(e) => update("persistThinking", e.target.checked)}
            />
            <span>
              <span className="font-medium">{t("settings.persistThinking")}</span>
              <span className="block text-xs text-app-fg-tertiary mt-0.5">
                {t("settings.persistThinkingHint")}
              </span>
            </span>
          </label>
        </section>

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.ritual")}
          </h3>
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
        </section>

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.provider")}
          </h3>
          {selectedProvider && (
            <>
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
              <TextField
                label={t("settings.apiKey")}
                value={form.apiKey}
                onChange={(v) => update("apiKey", v)}
                placeholder={
                  selectedProvider.hasApiKey
                    ? t("settings.apiKeyPlaceholder", { key: selectedProvider.apiKeyMasked })
                    : t("settings.apiKeyPlaceholderEmpty")
                }
                type="password"
              />
              {!selectedProvider.hasApiKey && (
                <p className="text-xs text-amber-700 dark:text-amber-300">
                  {t("settings.apiKeyMissing", { name: t(`provider.${selectedProvider.key}` as TranslationKey) })}
                </p>
              )}
              {!selectedProvider.hasApiKey && <ApiKeyHelp provider={selectedProvider.key} />}

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
            </>
          )}
        </section>

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.reflection")}
          </h3>
          <div className="grid grid-cols-2 gap-4">
            <TextField
              label={t("settings.minTurns")}
              value={form.reflectMinTurns}
              onChange={(v) => update("reflectMinTurns", v)}
              type="number"
            />
            <div className="flex items-end pb-1">
              <label className="flex items-center gap-2 text-sm text-app-fg dark:text-slate-200">
                <input
                  type="checkbox"
                  checked={form.reflectAutoAcceptMemories}
                  onChange={(e) => update("reflectAutoAcceptMemories", e.target.checked)}
                  className="rounded border-app-border"
                />
                {t("settings.autoAcceptMemories")}
              </label>
            </div>
          </div>
        </section>

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.context")}
          </h3>
          <div className="grid grid-cols-2 gap-4">
            <TextField
              label={t("settings.modelLimit")}
              value={form.contextModelLimit}
              onChange={(v) => update("contextModelLimit", v)}
              type="number"
            />
          </div>
        </section>

        <section className={`${ui.card} p-4 space-y-3`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.toolPermissions")}
          </h3>
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
        </section>

        <section className={`${ui.card} p-4 space-y-2`}>
          <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
            {t("settings.workspace")}
          </h3>
          <ReadOnlyField label={t("settings.root")} value={config.workspaceRoot} />
          <p className="text-[11px] text-app-fg-tertiary">{t("settings.workspaceHelp")}</p>
        </section>
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
  return (
    <div>
      <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={ui.input}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
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
  const t = useUiStore((s) => s.t);
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

  const remove = (rule: string) => onChange(rules.filter((r) => r !== rule));

  return (
    <div>
      <label className="block text-xs text-app-fg-secondary mb-1">{label}</label>
      <div className="space-y-1.5">
        {rules.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {rules.map((r) => (
              <span
                key={r}
                className={`inline-flex items-center gap-1 text-xs font-mono px-2 py-0.5 rounded-lg ${colors}`}
              >
                {r}
                <button
                  type="button"
                  onClick={() => remove(r)}
                  className="opacity-60 hover:opacity-100"
                  title={t("settings.remove")}
                >
                  <X size={10} />
                </button>
              </span>
            ))}
          </div>
        )}
        <div className="flex gap-2">
          <input
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e: React.KeyboardEvent<HTMLInputElement>) => {
              if (e.key === "Enter") {
                e.preventDefault();
                add();
              }
            }}
            placeholder={`e.g. ${tone === "green" ? "read" : "bash:rm *"}`}
            className={`flex-1 ${ui.input} py-1.5 text-xs font-mono`}
          />
          <Button size="sm" variant="secondary" onClick={add} disabled={!draft.trim()}>
            <Plus size={11} />
            {t("settings.add")}
          </Button>
        </div>
      </div>
    </div>
  );
}
