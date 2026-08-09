import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Pin, PinOff, Trash2, Plus, Search, Brain } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button, EmptyState, ui } from "../common/ui";
import { ConfirmPopover } from "../common/ConfirmPopover";
import { toast } from "../../utils/toast";
import { notifyRemembered } from "../../utils/remembered";
import { PendingReviewSection } from "./PendingReviewSection";

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
  const highlightMemoryId = useUiStore((s) => s.highlightMemoryId);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [activeZone, setActiveZone] = useState<string>(ALL);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [newBody, setNewBody] = useState("");
  const [newTags, setNewTags] = useState("");
  const [newZone, setNewZone] = useState("general");
  const [newScope, setNewScope] = useState("User");
  const [newPinned, setNewPinned] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const highlightRef = useRef<HTMLDivElement | null>(null);

  const fetchMemories = async () => {
    const items = await invoke<MemoryItem[]>("list_memories");
    setMemories(items);
  };

  useEffect(() => {
    fetchMemories();
  }, []);

  /** When a write lands (chat tool or create), refresh list so the new card can pulse. */
  useEffect(() => {
    if (!highlightMemoryId) return;
    void fetchMemories();
  }, [highlightMemoryId]);

  useEffect(() => {
    if (!highlightMemoryId || !highlightRef.current) return;
    highlightRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, [highlightMemoryId, memories]);

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
        m.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    });
  }, [memories, activeZone, query]);

  const handleCreate = async () => {
    if (!newBody.trim()) return;
    try {
      const item = await invoke<MemoryItem>("create_memory", {
        body: newBody,
        tags: newTags.split(",").map((x) => x.trim()).filter(Boolean),
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
      // Seal + toast + pulse (notifyRemembered also sets highlightMemoryId).
      notifyRemembered(item.id);
      await fetchMemories();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: string, scope: string) => {
    try {
      await invoke("delete_memory", { id, scope });
      setConfirmDeleteId(null);
      await fetchMemories();
      toast.success(t("toast.memoryDeleted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleTogglePin = async (id: string) => {
    try {
      await invoke("toggle_pin_memory", { id });
      await fetchMemories();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className={`flex-1 flex h-full ${ui.page}`}>
      <aside className="w-56 border-r border-app-border dark:border-slate-800 flex flex-col bg-app-sidebar dark:bg-slate-900/50">
        <header className="px-4 py-3 border-b border-app-border dark:border-slate-800 shrink-0">
          <h2 className={ui.sectionLabel}>{t("memory.zones")}</h2>
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
          <div className="my-1.5 border-t border-app-border dark:border-slate-800" />
          {zones.length === 0 && (
            <p className="text-xs text-app-fg-tertiary px-3 py-2">{t("memory.noZones")}</p>
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

      <div className="flex-1 flex flex-col min-w-0">
        <header className={`${ui.header} gap-3`}>
          <div className="flex items-center gap-2 min-w-0">
            <h2 className="text-base font-semibold truncate text-app-fg dark:text-slate-100">
              {activeZone === ALL
                ? t("memory.title")
                : activeZone === PINNED
                  ? t("memory.pinned")
                  : activeZone}
            </h2>
            <span className="text-xs text-app-fg-tertiary">{filtered.length}</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="relative">
              <Search
                size={12}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-app-fg-tertiary"
              />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("memory.search")}
                className="pl-7 pr-2.5 py-1.5 text-xs rounded-lg border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 focus:outline-none focus:ring-2 focus:ring-app-primary/30 w-40"
              />
            </div>
            <Button size="sm" onClick={() => setShowCreate(!showCreate)}>
              <Plus size={14} />
              {t("memory.new")}
            </Button>
          </div>
        </header>

        {showCreate && (
          <div className="p-4 border-b border-app-border dark:border-slate-800 space-y-3 bg-app-surface/50 dark:bg-slate-900/40">
            <textarea
              value={newBody}
              onChange={(e) => setNewBody(e.target.value)}
              placeholder={t("memory.contentPlaceholder")}
              className={`${ui.input} resize-none`}
              rows={3}
            />
            <div className="grid grid-cols-2 gap-3">
              <input
                value={newTags}
                onChange={(e) => setNewTags(e.target.value)}
                placeholder={t("memory.tagsPlaceholder")}
                className={ui.input}
              />
              <input
                value={newZone}
                onChange={(e) => setNewZone(e.target.value)}
                placeholder={t("memory.zonePlaceholder")}
                className={ui.input}
              />
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              <select
                value={newScope}
                onChange={(e) => setNewScope(e.target.value)}
                className={ui.input}
              >
                <option value="User">{t("scope.user")}</option>
                <option value="Project">{t("scope.project")}</option>
              </select>
              <label className="flex items-center gap-2 text-sm text-app-fg dark:text-slate-200">
                <input
                  type="checkbox"
                  checked={newPinned}
                  onChange={(e) => setNewPinned(e.target.checked)}
                />
                {t("memory.pinned")}
              </label>
              <div className="flex-1" />
              <Button size="sm" variant="secondary" onClick={() => setShowCreate(false)}>
                {t("memory.cancel")}
              </Button>
              <Button size="sm" onClick={handleCreate} disabled={!newBody.trim()}>
                {t("memory.save")}
              </Button>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4 space-y-3">
          <PendingReviewSection onAccepted={() => void fetchMemories()} />
          {filtered.length === 0 && (
            <EmptyState
              icon={<Brain size={22} strokeWidth={1.75} />}
              title={
                memories.length === 0
                  ? t("memory.emptyTitle")
                  : t("memory.noMatchTitle")
              }
              description={
                memories.length === 0
                  ? t("memory.empty")
                  : t("memory.noMatch")
              }
              action={
                memories.length === 0 ? (
                  <Button size="sm" onClick={() => setShowCreate(true)}>
                    <Plus size={14} />
                    {t("memory.new")}
                  </Button>
                ) : undefined
              }
            />
          )}
          {filtered.map((mem) => (
            <div
              key={mem.id}
              ref={mem.id === highlightMemoryId ? highlightRef : undefined}
              className={`${ui.card} p-3.5 space-y-2 relative ${
                mem.id === highlightMemoryId ? "mem-highlight" : ""
              }`}
            >
              <div className="flex items-start justify-between gap-2">
                <p className="text-sm flex-1 whitespace-pre-wrap text-app-fg dark:text-slate-100">
                  {mem.body}
                </p>
                <div className="flex items-center gap-1 shrink-0 relative">
                  <button
                    type="button"
                    onClick={() => handleTogglePin(mem.id)}
                    className="p-1.5 rounded-lg hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-secondary"
                    title={mem.pinned ? t("memory.unpin") : t("memory.pin")}
                  >
                    {mem.pinned ? <PinOff size={14} /> : <Pin size={14} />}
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmDeleteId(mem.id)}
                    className="p-1.5 rounded-lg hover:bg-red-50 dark:hover:bg-red-950/40 text-app-fg-secondary hover:text-app-danger"
                    title={t("memory.delete")}
                  >
                    <Trash2 size={14} />
                  </button>
                  <ConfirmPopover
                    open={confirmDeleteId === mem.id}
                    message={t("memory.deleteConfirm")}
                    onCancel={() => setConfirmDeleteId(null)}
                    onConfirm={() => void handleDelete(mem.id, mem.scope)}
                  />
                </div>
              </div>
              <div className="flex items-center gap-2 flex-wrap">
                {mem.pinned && (
                  <span className="text-xs px-1.5 py-0.5 rounded-md bg-amber-100 dark:bg-amber-900/40 text-amber-800 dark:text-amber-300">
                    {t("memory.pinnedBadge")}
                  </span>
                )}
                <span className="text-xs text-app-fg-tertiary">{displayScope(mem.scope, t)}</span>
                <span className="text-xs text-app-fg-tertiary">
                  {displayConfidence(mem.confidence, t)}
                </span>
                <span className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary dark:text-slate-300">
                  {mem.zone}
                </span>
                {mem.tags.map((tag) => (
                  <span
                    key={tag}
                    className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary"
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

function ZoneRow({
  label,
  count,
  active,
  onClick,
  highlight,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  highlight?: boolean;
}) {
  return (
    <div
      onClick={onClick}
      className={`flex items-center justify-between px-3 py-1.5 rounded-lg cursor-pointer text-sm transition-colors ${
        active ? ui.navItemActive : ui.navItemIdle
      }`}
    >
      <span className={`truncate ${highlight ? "text-amber-700 dark:text-amber-300" : ""}`}>
        {label}
      </span>
      <span className="text-[11px] text-app-fg-tertiary ml-2">{count}</span>
    </div>
  );
}

function displayScope(scope: string, t: ReturnType<typeof useUiStore.getState>["t"]) {
  return scope === "Project" ? t("scope.project") : t("scope.user");
}

function displayConfidence(
  confidence: string,
  t: ReturnType<typeof useUiStore.getState>["t"]
) {
  const normalized = confidence.toLowerCase();
  if (normalized === "high") return t("confidence.high");
  if (normalized === "medium") return t("confidence.medium");
  if (normalized === "low") return t("confidence.low");
  return confidence;
}
