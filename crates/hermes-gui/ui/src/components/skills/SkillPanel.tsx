import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Trash2, Plus, Pencil, Save, X } from "lucide-react";

interface SkillItem {
  name: string;
  description: string;
  triggers: string[];
  scope: string;
  body: string;
}

type Mode = "view" | "edit" | "create";

interface DraftSkill {
  name: string;
  description: string;
  triggers: string;
  body: string;
  scope: string;
}

const EMPTY_DRAFT: DraftSkill = {
  name: "",
  description: "",
  triggers: "",
  body: "",
  scope: "User",
};

function toDraft(s: SkillItem): DraftSkill {
  return {
    name: s.name,
    description: s.description,
    triggers: s.triggers.join(", "),
    body: s.body,
    scope: s.scope,
  };
}

export function SkillPanel() {
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [selected, setSelected] = useState<SkillItem | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [draft, setDraft] = useState<DraftSkill>(EMPTY_DRAFT);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const fetchSkills = async () => {
    const items = await invoke<SkillItem[]>("list_skills");
    setSkills(items);
  };

  useEffect(() => {
    fetchSkills();
  }, []);

  const select = (s: SkillItem) => {
    setSelected(s);
    setMode("view");
    setError(null);
  };

  const startCreate = () => {
    setSelected(null);
    setDraft(EMPTY_DRAFT);
    setMode("create");
    setError(null);
  };

  const startEdit = () => {
    if (!selected) return;
    setDraft(toDraft(selected));
    setMode("edit");
    setError(null);
  };

  const cancel = () => {
    setMode("view");
    setError(null);
  };

  const save = async () => {
    setError(null);
    const triggers = draft.triggers
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    if (!draft.name.trim()) {
      setError("Name is required.");
      return;
    }
    setBusy(true);
    try {
      await invoke("save_skill", {
        name: draft.name.trim(),
        description: draft.description.trim(),
        triggers,
        body: draft.body,
        scope: draft.scope,
      });
      await fetchSkills();
      const next = await invoke<SkillItem | null>("get_skill", { name: draft.name.trim() });
      if (next) setSelected(next);
      setMode("view");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (name: string, scope: string) => {
    await invoke("delete_skill", { name, scope });
    if (selected?.name === name) {
      setSelected(null);
      setMode("view");
    }
    fetchSkills();
  };

  return (
    <div className="flex-1 flex h-full">
      <div className="w-64 border-r border-gray-200 dark:border-gray-700 flex flex-col">
        <header className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-lg font-semibold">Skills</h2>
          <button
            onClick={startCreate}
            className="p-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-300"
            title="New skill"
          >
            <Plus size={16} />
          </button>
        </header>
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {skills.length === 0 && (
            <p className="text-sm text-gray-500 text-center mt-8">No skills.</p>
          )}
          {skills.map((skill) => (
            <div
              key={`${skill.scope}/${skill.name}`}
              className={`group flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm ${
                selected?.name === skill.name && mode === "view"
                  ? "bg-gray-200 dark:bg-gray-700"
                  : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
              }`}
              onClick={() => select(skill)}
            >
              <span className="truncate flex-1 font-mono">{skill.name}</span>
              <span className="text-[10px] uppercase text-gray-400">{skill.scope[0]}</span>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(skill.name, skill.scope);
                }}
                className="opacity-0 group-hover:opacity-100 p-1 hover:text-red-500"
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {mode === "view" && selected && (
          <div className="space-y-4">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h3 className="text-lg font-semibold font-mono truncate">{selected.name}</h3>
                <p className="text-sm text-gray-500 mt-1">{selected.description}</p>
              </div>
              <button
                onClick={startEdit}
                className="shrink-0 inline-flex items-center gap-1 text-xs px-2.5 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
              >
                <Pencil size={12} />
                Edit
              </button>
            </div>
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-xs text-gray-400">{selected.scope}</span>
              {selected.triggers.map((t) => (
                <span
                  key={t}
                  className="text-xs px-1.5 py-0.5 rounded bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300"
                >
                  {t}
                </span>
              ))}
            </div>
            <pre className="text-sm whitespace-pre-wrap font-mono p-3 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 overflow-y-auto max-h-[60vh]">
              {selected.body}
            </pre>
          </div>
        )}

        {mode === "view" && !selected && (
          <p className="text-sm text-gray-500 text-center mt-8">
            Select a skill to view its details, or click + to create a new one.
          </p>
        )}

        {(mode === "edit" || mode === "create") && (
          <SkillEditor
            draft={draft}
            onChange={setDraft}
            mode={mode}
            busy={busy}
            error={error}
            onSave={save}
            onCancel={cancel}
          />
        )}
      </div>
    </div>
  );
}

interface SkillEditorProps {
  draft: DraftSkill;
  onChange: (d: DraftSkill) => void;
  mode: Mode;
  busy: boolean;
  error: string | null;
  onSave: () => void;
  onCancel: () => void;
}

function SkillEditor({ draft, onChange, mode, busy, error, onSave, onCancel }: SkillEditorProps) {
  const set = <K extends keyof DraftSkill>(k: K, v: DraftSkill[K]) =>
    onChange({ ...draft, [k]: v });

  return (
    <div className="space-y-4 max-w-2xl">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold">
          {mode === "create" ? "New skill" : "Edit skill"}
        </h3>
        <div className="flex gap-2">
          <button
            onClick={onCancel}
            disabled={busy}
            className="inline-flex items-center gap-1 text-xs px-2.5 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            <X size={12} />
            Cancel
          </button>
          <button
            onClick={onSave}
            disabled={busy}
            className="inline-flex items-center gap-1 text-xs px-2.5 py-1 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50"
          >
            <Save size={12} />
            {busy ? "Saving..." : "Save"}
          </button>
        </div>
      </div>

      {error && (
        <p className="text-sm text-red-500 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded px-3 py-2">
          {error}
        </p>
      )}

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-gray-500">Name</label>
        <input
          type="text"
          value={draft.name}
          onChange={(e) => set("name", e.target.value)}
          disabled={mode === "edit"}
          placeholder="e.g. python-test-fixtures"
          className="w-full font-mono rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-60"
        />
        <p className="text-[11px] text-gray-400">
          Lowercase, ASCII alphanumeric, dash and underscore. Cannot be renamed after creation.
        </p>
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-gray-500">Description</label>
        <input
          type="text"
          value={draft.description}
          onChange={(e) => set("description", e.target.value)}
          placeholder="Short one-liner shown in the picker"
          className="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-gray-500">
          Triggers (comma-separated)
        </label>
        <input
          type="text"
          value={draft.triggers}
          onChange={(e) => set("triggers", e.target.value)}
          placeholder="pytest, fixture, mock"
          className="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-gray-500">Scope</label>
        <select
          value={draft.scope}
          onChange={(e) => set("scope", e.target.value)}
          disabled={mode === "edit"}
          className="rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-60"
        >
          <option value="User">User (~/.small-rust-hermes/skills)</option>
          <option value="Project">Project (./.small-rust-hermes/skills)</option>
        </select>
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-gray-500">Body (Markdown)</label>
        <textarea
          value={draft.body}
          onChange={(e) => set("body", e.target.value)}
          rows={16}
          className="w-full resize-y rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </div>
    </div>
  );
}
