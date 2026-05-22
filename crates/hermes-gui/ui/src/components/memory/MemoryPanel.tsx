import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Pin, PinOff, Trash2, Plus, Search } from "lucide-react";
import { useUiStore } from "../../store/uiStore";

interface MemoryItem {
  id: string;
  body: string;
  scope: string;
  pinned: boolean;
  confidence: string;
  tags: string[];
  zone: string;
  createdAt: string;
  source: string;
}

const ALL = "__all__";
const PINNED = "__pinned__";

export function MemoryPanel() {
  const t = useUiStore((state) => state.t);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [activeZone, setActiveZone] = useState<string>(ALL);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [newBody, setNewBody] = useState("");
  const [newTags, setNewTags] = useState("");
  const [newZone, setNewZone] = useState("general");
  const [newScope, setNewScope] = useState("User");
  const [newPinned, setNewPinned] = useState(false);

  const fetchMemories = async () => {
    const items = await invoke<MemoryItem[]>("list_memories");
    setMemories(items);
  };

  useEffect(() => {
    fetchMemories();
  }, []);

  const zones = useMemo(() => {
    const counts = new Map<string, number>();
    for (const m of memories) {
      counts.set(m.zone, (counts.get(m.zone) ?? 0) + 1);
    }
    return Array.from(counts.entries()).sort(([a], [b]) => a.localeCompare(b));
  }, [memories]);

  const pinnedCount = useMemo(() => memories.filter((m) => m.pinned).length, [memories]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return memories.filter((m) => {
      if (activeZone === PINNED && !m.pinned) return false;
      if (activeZone !== ALL && activeZone !== PINNED && m.zone !== activeZone) return false;
      if (!q) return true;
      return (
        m.body.toLowerCase().includes(q) ||
        m.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [memories, activeZone, query]);

  const handleCreate = async () => {
    if (!newBody.trim()) return;
    await invoke("create_memory", {
      body: newBody,
      tags: newTags.split(",").map((t) => t.trim()).filter(Boolean),
      scope: newScope,
      zone: newZone.trim() || null,
      pinned: newPinned,
    });
    setNewBody("");
    setNewTags("");
    setNewZone("general");
    setNewScope("User");
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
    <div className="flex-1 flex h-full">
      {/* Zone sidebar */}
      <aside className="w-56 border-r border-gray-200 dark:border-gray-700 flex flex-col">
        <header className="px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-gray-500">
            {t("memory.zones")}
          </h2>
        </header>
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          <ZoneRow
            label={t("memory.all")}
            count={memories.length}
            active={activeZone === ALL}
            onClick={() => setActiveZone(ALL)}
          />
          <ZoneRow
            label={t("memory.pinned")}
            count={pinnedCount}
            active={activeZone === PINNED}
            onClick={() => setActiveZone(PINNED)}
            highlight
          />
          <div className="my-1.5 border-t border-gray-200 dark:border-gray-700" />
          {zones.length === 0 && (
            <p className="text-xs text-gray-400 px-3 py-2">{t("memory.noZones")}</p>
          )}
          {zones.map(([zone, count]) => (
            <ZoneRow
              key={zone}
              label={zone}
              count={count}
              active={activeZone === zone}
              onClick={() => setActiveZone(zone)}
            />
          ))}
        </div>
      </aside>

      {/* Memory list */}
      <div className="flex-1 flex flex-col min-w-0">
        <header className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700 gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <h2 className="text-lg font-semibold truncate">
              {activeZone === ALL
                ? t("memory.title")
                : activeZone === PINNED
                ? t("memory.pinned")
                : activeZone}
            </h2>
            <span className="text-xs text-gray-400">{filtered.length}</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="relative">
              <Search size={12} className="absolute left-2 top-1/2 -translate-y-1/2 text-gray-400" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("memory.search")}
                className="pl-7 pr-2 py-1.5 text-xs rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500 w-40"
              />
            </div>
            <button
              onClick={() => setShowCreate(!showCreate)}
              className="flex items-center gap-1 px-3 py-1.5 text-sm rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
            >
              <Plus size={14} />
              {t("memory.new")}
            </button>
          </div>
        </header>

        {showCreate && (
          <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
            <textarea
              value={newBody}
              onChange={(e) => setNewBody(e.target.value)}
              placeholder={t("memory.contentPlaceholder")}
              className="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 resize-none"
              rows={3}
            />
            <div className="grid grid-cols-2 gap-3">
              <input
                value={newTags}
                onChange={(e) => setNewTags(e.target.value)}
                placeholder={t("memory.tagsPlaceholder")}
                className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
              />
              <input
                value={newZone}
                onChange={(e) => setNewZone(e.target.value)}
                placeholder={t("memory.zonePlaceholder")}
                className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
              />
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              <select
                value={newScope}
                onChange={(e) => setNewScope(e.target.value)}
                className="px-2.5 py-1.5 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
              >
                <option value="User">{t("scope.user")}</option>
                <option value="Project">{t("scope.project")}</option>
              </select>
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={newPinned}
                  onChange={(e) => setNewPinned(e.target.checked)}
                />
                {t("memory.pinned")}
              </label>
              <div className="flex-1" />
              <button
                onClick={() => setShowCreate(false)}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700"
              >
                {t("memory.cancel")}
              </button>
              <button
                onClick={handleCreate}
                disabled={!newBody.trim()}
                className="px-3 py-1.5 text-sm rounded-lg bg-green-600 text-white hover:bg-green-700 disabled:opacity-50"
              >
                {t("memory.save")}
              </button>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4 space-y-3">
          {filtered.length === 0 && (
            <p className="text-sm text-gray-500 text-center mt-8">
              {memories.length === 0
                ? t("memory.empty")
                : t("memory.noMatch")}
            </p>
          )}
          {filtered.map((mem) => (
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
                    title={mem.pinned ? t("memory.unpin") : t("memory.pin")}
                  >
                    {mem.pinned ? <PinOff size={14} /> : <Pin size={14} />}
                  </button>
                  <button
                    onClick={() => handleDelete(mem.id, mem.scope)}
                    className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-red-500"
                    title={t("memory.delete")}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
              <div className="flex items-center gap-2 flex-wrap">
                {mem.pinned && (
                  <span className="text-xs px-1.5 py-0.5 rounded bg-yellow-100 dark:bg-yellow-900 text-yellow-700 dark:text-yellow-300">
                    {t("memory.pinnedBadge")}
                  </span>
                )}
                <span className="text-xs text-gray-400">{displayScope(mem.scope, t)}</span>
                <span className="text-xs text-gray-400">{displayConfidence(mem.confidence, t)}</span>
                <span className="text-xs px-1.5 py-0.5 rounded bg-purple-100 dark:bg-purple-900/40 text-purple-700 dark:text-purple-300">
                  {mem.zone}
                </span>
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
    </div>
  );
}

interface ZoneRowProps {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  highlight?: boolean;
}

function ZoneRow({ label, count, active, onClick, highlight }: ZoneRowProps) {
  const base = "flex items-center justify-between px-3 py-1.5 rounded-md cursor-pointer text-sm";
  const activeCls = active
    ? "bg-gray-200 dark:bg-gray-700 font-medium"
    : "hover:bg-gray-100 dark:hover:bg-gray-700/50";
  const labelCls = highlight ? "text-yellow-700 dark:text-yellow-300" : "";
  return (
    <div onClick={onClick} className={`${base} ${activeCls}`}>
      <span className={`truncate ${labelCls}`}>{label}</span>
      <span className="text-[11px] text-gray-400 ml-2">{count}</span>
    </div>
  );
}

function displayScope(scope: string, t: ReturnType<typeof useUiStore.getState>["t"]) {
  return scope === "Project" ? t("scope.project") : t("scope.user");
}

function displayConfidence(confidence: string, t: ReturnType<typeof useUiStore.getState>["t"]) {
  const normalized = confidence.toLowerCase();
  if (normalized === "high") return t("confidence.high");
  if (normalized === "medium") return t("confidence.medium");
  if (normalized === "low") return t("confidence.low");
  return confidence;
}
