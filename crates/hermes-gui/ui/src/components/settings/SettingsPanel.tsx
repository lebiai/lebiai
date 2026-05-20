import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Save, RefreshCw, Plus, X } from "lucide-react";

interface ConfigView {
  defaultProvider: string;
  model: string;
  maxTokens: number;
  apiKeyMasked: string;
  baseUrl: string;
  reflectMinTurns: number;
  reflectAutoAcceptMemories: boolean;
  contextModelLimit: number;
  permissionsAllow: string[];
  permissionsDeny: string[];
  workspaceRoot: string;
}

interface Form {
  model: string;
  maxTokens: string;
  baseUrl: string;
  apiKey: string;
  reflectMinTurns: string;
  reflectAutoAcceptMemories: boolean;
  contextModelLimit: string;
  permissionsAllow: string[];
  permissionsDeny: string[];
}

function toForm(c: ConfigView): Form {
  return {
    model: c.model,
    maxTokens: String(c.maxTokens),
    baseUrl: c.baseUrl,
    apiKey: "",
    reflectMinTurns: String(c.reflectMinTurns),
    reflectAutoAcceptMemories: c.reflectAutoAcceptMemories,
    contextModelLimit: String(c.contextModelLimit),
    permissionsAllow: [...c.permissionsAllow],
    permissionsDeny: [...c.permissionsDeny],
  };
}

export function SettingsPanel() {
  const [config, setConfig] = useState<ConfigView | null>(null);
  const [form, setForm] = useState<Form | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);

  const load = async () => {
    try {
      const c = await invoke<ConfigView>("get_config");
      setConfig(c);
      setForm(toForm(c));
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
  };

  const save = async () => {
    if (!form) return;
    setSaving(true);
    setError(null);
    try {
      const maxTokens = Number(form.maxTokens);
      const minTurns = Number(form.reflectMinTurns);
      const ctxLimit = Number(form.contextModelLimit);
      if (!Number.isFinite(maxTokens) || maxTokens <= 0) {
        throw new Error("Max tokens must be a positive number.");
      }
      if (!Number.isFinite(minTurns) || minTurns < 0) {
        throw new Error("Reflect min turns must be ≥ 0.");
      }
      if (!Number.isFinite(ctxLimit) || ctxLimit <= 0) {
        throw new Error("Context model limit must be a positive number.");
      }
      await invoke("update_config", {
        update: {
          model: form.model,
          maxTokens,
          baseUrl: form.baseUrl,
          apiKey: form.apiKey.trim() ? form.apiKey : null,
          reflectMinTurns: minTurns,
          reflectAutoAcceptMemories: form.reflectAutoAcceptMemories,
          contextModelLimit: ctxLimit,
          permissionsAllow: form.permissionsAllow,
          permissionsDeny: form.permissionsDeny,
        },
      });
      setSavedAt(Date.now());
      update("apiKey", "");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setSaving(false);
    }
  };

  if (error && !config) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-red-500">{error}</p>
      </div>
    );
  }
  if (!config || !form) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-2xl mx-auto p-6 space-y-6">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Settings</h2>
          <div className="flex items-center gap-2">
            <button
              onClick={load}
              className="inline-flex items-center gap-1 text-xs px-2.5 py-1.5 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
              title="Reload from disk"
            >
              <RefreshCw size={12} />
              Reload
            </button>
            <button
              onClick={save}
              disabled={saving}
              className="inline-flex items-center gap-1 text-xs px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50"
            >
              <Save size={12} />
              {saving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>

        {error && (
          <p className="text-sm text-red-500 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded px-3 py-2">
            {error}
          </p>
        )}
        {savedAt && !error && (
          <p className="text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-900/30 border border-amber-200 dark:border-amber-800 rounded px-3 py-2">
            Saved to ~/.small-rust-hermes/config.toml. Restart the app for changes to take effect.
          </p>
        )}

        <section className="space-y-3">
          <h3 className="text-xs font-medium text-gray-500 uppercase tracking-wide">
            Provider — {config.defaultProvider}
          </h3>
          <div className="grid grid-cols-2 gap-4">
            <TextField label="Model" value={form.model} onChange={(v) => update("model", v)} />
            <TextField
              label="Max Tokens"
              value={form.maxTokens}
              onChange={(v) => update("maxTokens", v)}
              type="number"
            />
            <TextField
              label="Base URL"
              value={form.baseUrl}
              onChange={(v) => update("baseUrl", v)}
              className="col-span-2"
            />
            <TextField
              label="API Key"
              value={form.apiKey}
              onChange={(v) => update("apiKey", v)}
              placeholder={`current: ${config.apiKeyMasked} — leave blank to keep`}
              type="password"
              className="col-span-2"
            />
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-medium text-gray-500 uppercase tracking-wide">Reflection</h3>
          <div className="grid grid-cols-2 gap-4">
            <TextField
              label="Min Turns"
              value={form.reflectMinTurns}
              onChange={(v) => update("reflectMinTurns", v)}
              type="number"
            />
            <div className="flex items-end pb-1">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={form.reflectAutoAcceptMemories}
                  onChange={(e) => update("reflectAutoAcceptMemories", e.target.checked)}
                />
                Auto-accept low-risk memories
              </label>
            </div>
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-medium text-gray-500 uppercase tracking-wide">Context</h3>
          <div className="grid grid-cols-2 gap-4">
            <TextField
              label="Model Limit (tokens)"
              value={form.contextModelLimit}
              onChange={(v) => update("contextModelLimit", v)}
              type="number"
            />
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-medium text-gray-500 uppercase tracking-wide">
            Tool Permissions
          </h3>
          <p className="text-xs text-gray-400">
            Format: <code className="font-mono">tool</code> or{" "}
            <code className="font-mono">tool:glob</code>. Examples:{" "}
            <code className="font-mono">read</code>,{" "}
            <code className="font-mono">bash:git *</code>.
          </p>
          <RuleList
            label="Allow"
            tone="green"
            rules={form.permissionsAllow}
            onChange={(rules) => update("permissionsAllow", rules)}
          />
          <RuleList
            label="Deny"
            tone="red"
            rules={form.permissionsDeny}
            onChange={(rules) => update("permissionsDeny", rules)}
          />
        </section>

        <section className="space-y-2">
          <h3 className="text-xs font-medium text-gray-500 uppercase tracking-wide">Workspace</h3>
          <ReadOnlyField label="Root" value={config.workspaceRoot} />
          <p className="text-[11px] text-gray-400">
            Workspace root is set at startup and can't be changed here. Edit{" "}
            <code className="font-mono">[workspace] root</code> in config.toml and restart.
          </p>
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
      <label className="block text-xs text-gray-500 mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full px-3 py-1.5 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
    </div>
  );
}

function ReadOnlyField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <label className="block text-xs text-gray-500 mb-1">{label}</label>
      <div className="px-3 py-1.5 text-sm rounded-md border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/60 font-mono break-all">
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
      ? "bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300"
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
      <label className="block text-xs text-gray-500 mb-1">{label}</label>
      <div className="space-y-1.5">
        {rules.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {rules.map((r) => (
              <span
                key={r}
                className={`inline-flex items-center gap-1 text-xs font-mono px-2 py-0.5 rounded ${colors}`}
              >
                {r}
                <button
                  onClick={() => remove(r)}
                  className="opacity-60 hover:opacity-100"
                  title="Remove"
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
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                add();
              }
            }}
            placeholder={`e.g. ${tone === "green" ? "read" : "bash:rm *"}`}
            className="flex-1 px-2.5 py-1 text-xs rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          <button
            onClick={add}
            disabled={!draft.trim()}
            className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-50"
          >
            <Plus size={11} />
            Add
          </button>
        </div>
      </div>
    </div>
  );
}
