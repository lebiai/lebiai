import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ConfigView {
  defaultProvider: string;
  model: string;
  maxTokens: number;
  apiKeyMasked: string;
  baseUrl: string;
  reflectMinTurns: number;
  contextModelLimit: number;
}

export function SettingsPanel() {
  const [config, setConfig] = useState<ConfigView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ConfigView>("get_config")
      .then(setConfig)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-red-500">{error}</p>
      </div>
    );
  }

  if (!config) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-sm text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 max-w-2xl mx-auto space-y-6">
      <h2 className="text-lg font-semibold">Settings</h2>

      <section className="space-y-3">
        <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">Provider</h3>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Provider" value={config.defaultProvider} />
          <Field label="Model" value={config.model} />
          <Field label="Max Tokens" value={String(config.maxTokens)} />
          <Field label="API Key" value={config.apiKeyMasked} />
          <Field label="Base URL" value={config.baseUrl} className="col-span-2" />
        </div>
      </section>

      <section className="space-y-3">
        <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">Reflection</h3>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Min Turns" value={String(config.reflectMinTurns)} />
        </div>
      </section>

      <section className="space-y-3">
        <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">Context</h3>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Model Limit" value={String(config.contextModelLimit)} />
        </div>
      </section>

      <p className="text-xs text-gray-400 mt-4">
        Edit ~/.small-rust-hermes/config.toml to change settings. Restart the app after changes.
      </p>
    </div>
  );
}

function Field({ label, value, className = "" }: { label: string; value: string; className?: string }) {
  return (
    <div className={className}>
      <label className="block text-xs text-gray-500 mb-1">{label}</label>
      <div className="px-3 py-2 text-sm rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 font-mono">
        {value}
      </div>
    </div>
  );
}
