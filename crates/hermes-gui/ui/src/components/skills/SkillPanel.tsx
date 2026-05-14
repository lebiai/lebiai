import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Trash2 } from "lucide-react";

interface SkillItem {
  name: string;
  description: string;
  triggers: string[];
  scope: string;
  body: string;
}

export function SkillPanel() {
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [selected, setSelected] = useState<SkillItem | null>(null);

  const fetchSkills = async () => {
    const items = await invoke<SkillItem[]>("list_skills");
    setSkills(items);
  };

  useEffect(() => {
    fetchSkills();
  }, []);

  const handleDelete = async (name: string, scope: string) => {
    await invoke("delete_skill", { name, scope });
    if (selected?.name === name) setSelected(null);
    fetchSkills();
  };

  return (
    <div className="flex-1 flex h-full">
      <div className="w-64 border-r border-gray-200 dark:border-gray-700 flex flex-col">
        <header className="px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-lg font-semibold">Skills</h2>
        </header>
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {skills.length === 0 && (
            <p className="text-sm text-gray-500 text-center mt-8">No skills.</p>
          )}
          {skills.map((skill) => (
            <div
              key={skill.name}
              className={`group flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm ${
                selected?.name === skill.name
                  ? "bg-gray-200 dark:bg-gray-700"
                  : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
              }`}
              onClick={() => setSelected(skill)}
            >
              <span className="truncate flex-1 font-mono">{skill.name}</span>
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
        {selected ? (
          <div className="space-y-4">
            <div>
              <h3 className="text-lg font-semibold font-mono">{selected.name}</h3>
              <p className="text-sm text-gray-500 mt-1">{selected.description}</p>
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
        ) : (
          <p className="text-sm text-gray-500 text-center mt-8">
            Select a skill to view its details.
          </p>
        )}
      </div>
    </div>
  );
}
