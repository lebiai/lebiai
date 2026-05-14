import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Pin, PinOff, Trash2, Plus } from "lucide-react";

interface MemoryItem {
  id: string;
  body: string;
  scope: string;
  pinned: boolean;
  confidence: string;
  tags: string[];
  createdAt: string;
  source: string;
}

export function MemoryPanel() {
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [showCreate, setShowCreate] = useState(false);
  const [newBody, setNewBody] = useState("");
  const [newTags, setNewTags] = useState("");
  const [newPinned, setNewPinned] = useState(false);

  const fetchMemories = async () => {
    const items = await invoke<MemoryItem[]>("list_memories");
    setMemories(items);
  };

  useEffect(() => {
    fetchMemories();
  }, []);

  const handleCreate = async () => {
    if (!newBody.trim()) return;
    await invoke("create_memory", {
      body: newBody,
      tags: newTags.split(",").map((t) => t.trim()).filter(Boolean),
      scope: "User",
      pinned: newPinned,
    });
    setNewBody("");
    setNewTags("");
    setNewPinned(false);
    setShowCreate(false);
    fetchMemories();
  };

  const handleDelete = async (id: string, scope: string) => {
    await invoke("delete_memory", { id, scope });
    fetchMemories();
  };

  const handleTogglePin = async (id: string) => {
    await invoke("toggle_pin_memory", { id });
    fetchMemories();
  };

  return (
    <div className="flex-1 flex flex-col h-full">
      <header className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
        <h2 className="text-lg font-semibold">Memories</h2>
        <button
          onClick={() => setShowCreate(!showCreate)}
          className="flex items-center gap-1 px-3 py-1.5 text-sm rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
        >
          <Plus size={14} />
          New
        </button>
      </header>

      {showCreate && (
        <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
          <textarea
            value={newBody}
            onChange={(e) => setNewBody(e.target.value)}
            placeholder="Memory content..."
            className="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 resize-none"
            rows={3}
          />
          <input
            value={newTags}
            onChange={(e) => setNewTags(e.target.value)}
            placeholder="Tags (comma-separated)"
            className="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
          />
          <div className="flex items-center gap-4">
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={newPinned}
                onChange={(e) => setNewPinned(e.target.checked)}
              />
              Pinned
            </label>
            <button
              onClick={handleCreate}
              className="px-3 py-1.5 text-sm rounded-lg bg-green-600 text-white hover:bg-green-700"
            >
              Save
            </button>
            <button
              onClick={() => setShowCreate(false)}
              className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {memories.length === 0 && (
          <p className="text-sm text-gray-500 text-center mt-8">No memories yet.</p>
        )}
        {memories.map((mem) => (
          <div
            key={mem.id}
            className="p-3 rounded-lg border border-gray-200 dark:border-gray-700 space-y-2"
          >
            <div className="flex items-start justify-between gap-2">
              <p className="text-sm flex-1 whitespace-pre-wrap">{mem.body}</p>
              <div className="flex items-center gap-1 shrink-0">
                <button
                  onClick={() => handleTogglePin(mem.id)}
                  className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700"
                  title={mem.pinned ? "Unpin" : "Pin"}
                >
                  {mem.pinned ? <PinOff size={14} /> : <Pin size={14} />}
                </button>
                <button
                  onClick={() => handleDelete(mem.id, mem.scope)}
                  className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-red-500"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
            <div className="flex items-center gap-2 flex-wrap">
              {mem.pinned && (
                <span className="text-xs px-1.5 py-0.5 rounded bg-yellow-100 dark:bg-yellow-900 text-yellow-700 dark:text-yellow-300">
                  pinned
                </span>
              )}
              <span className="text-xs text-gray-400">{mem.scope}</span>
              <span className="text-xs text-gray-400">{mem.confidence}</span>
              {mem.tags.map((tag) => (
                <span
                  key={tag}
                  className="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-700"
                >
                  {tag}
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
