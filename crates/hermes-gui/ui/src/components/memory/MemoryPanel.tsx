import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Pin, PinOff, Trash2, Plus, Search, Brain, Inbox } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { Button, EmptyState, ui } from "../common/ui";
import { Select } from "../common/Select";
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
const PENDING = "__pending__";

type CompanionGroup = "preferences" | "standards" | "work" | "other";

function companionGroup(mem: MemoryItem): CompanionGroup {
  const z = (mem.zone || "").toLowerCase();
  const tags = mem.tags.map((tag) => tag.toLowerCase());
  if (
    z === "preferences" ||
    z === "preference" ||
    z === "core" ||
    tags.includes("preference")
  ) {
    return "preferences";
  }
  if (z === "standards" || z === "standard" || tags.includes("standard")) {
    return "standards";
  }
  if (
    z === "work" ||
    z === "episode" ||
    z === "work-episode" ||
    tags.includes("work-episode")
  ) {
    return "work";
  }
  return "other";
}

export function MemoryPanel({ embedded = false }: { embedded?: boolean }) {
  const t = useUiStore((state) => state.t);
  const highlightMemoryId = useUiStore((s) => s.highlightMemoryId);
  const pendingFocusSeq = useNavStore((s) => s.pendingFocusSeq);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
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

  const fetchPendingCount = async () => {
    try {
      const n = await invoke<number>("count_pending_review");
      setPendingCount(n);
    } catch {
      setPendingCount(0);
    }
  };

  useEffect(() => {
    void fetchMemories();
    void fetchPendingCount();
    const onInbox = () => void fetchPendingCount();
    window.addEventListener("hermes:inbox-changed", onInbox);
    return () => window.removeEventListener("hermes:inbox-changed", onInbox);
  }, []);

  useEffect(() => {
    if (pendingFocusSeq > 0) {
      setActiveZone(PENDING);
    }
  }, [pendingFocusSeq]);

  /** When a write lands (chat tool or create), refresh list so the new card can pulse. */
  useEffect(() => {
    if (!highlightMemoryId) return;
    void fetchMemories();
  }, [highlightMemoryId]);

  useEffect(() => {
    if (!highlightMemoryId || !highlightRef.current) return;
    highlightRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, [highlightMemoryId, memories]);

  const groupCounts = useMemo(() => {
    const counts: Record<CompanionGroup, number> = {
      preferences: 0,
      standards: 0,
      work: 0,
      other: 0,
    };
    for (const m of memories) {
      counts[companionGroup(m)] += 1;
    }
    return counts;
  }, [memories]);

  const pinnedCount = useMemo(() => memories.filter((m) => m.pinned).length, [memories]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return memories.filter((m) => {
      if (activeZone === PINNED && !m.pinned) return false;
      if (
        activeZone === "preferences" ||
        activeZone === "standards" ||
        activeZone === "work" ||
        activeZone === "other"
      ) {
        if (companionGroup(m) !== activeZone) return false;
      } else if (
        activeZone !== ALL &&
        activeZone !== PINNED &&
        activeZone !== PENDING
      ) {
        return false;
      }
      if (!q) return true;
      return (
        m.body.toLowerCase().includes(q) ||
        m.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    });
  }, [memories, activeZone, query]);

  const groupLabel = (id: string) => {
    if (id === ALL) return t("memory.all");
    if (id === PINNED) return t("memory.pinned");
    if (id === PENDING) return t("memory.pendingZone");
    if (id === "preferences") return t("memory.groupPreferences");
    if (id === "standards") return t("memory.groupStandards");
    if (id === "work") return t("memory.groupWork");
    if (id === "other") return t("memory.groupOther");
    return id;
  };

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

  const showPendingBlock = activeZone === PENDING || activeZone === ALL;

  return (
    <div className={`flex-1 flex h-full ${ui.page}`}>
      <aside className="w-56 border-r border-app-border dark:border-slate-800 flex flex-col bg-app-sidebar dark:bg-slate-900/50 select-none">
        {!embedded && (
          <header className="px-4 py-3 border-b border-app-border dark:border-slate-800 shrink-0">
            <h2 className={ui.sectionLabel}>{t("know.tabYou")}</h2>
          </header>
        )}
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          <ZoneRow
            label={t("memory.pendingZone")}
            count={pendingCount}
            active={activeZone === PENDING}
            onClick={() => setActiveZone(PENDING)}
            highlight
            badge={pendingCount > 0}
          />
          <ZoneRow
            label={t("memory.pinned")}
            count={pinnedCount}
            active={activeZone === PINNED}
            onClick={() => setActiveZone(PINNED)}
            highlight
          />
          <div className="my-1.5 border-t border-app-border dark:border-slate-800" />
          {(
            [
              ["preferences", groupCounts.preferences],
              ["standards", groupCounts.standards],
              ["work", groupCounts.work],
              ["other", groupCounts.other],
            ] as const
          ).map(([id, count]) => (
            <ZoneRow
              key={id}
              label={groupLabel(id)}
              count={count}
              active={activeZone === id}
              onClick={() => setActiveZone(id)}
            />
          ))}
          <div className="my-1.5 border-t border-app-border dark:border-slate-800" />
          <ZoneRow
            label={t("memory.all")}
            count={memories.length}
            active={activeZone === ALL}
            onClick={() => setActiveZone(ALL)}
          />
        </div>
      </aside>

      <div className="flex-1 flex flex-col min-w-0">
        <header className={`${ui.header} gap-3`}>
          <div className="flex items-center gap-2 min-w-0">
            <h2 className="text-base font-semibold truncate text-app-fg dark:text-slate-100">
              {groupLabel(activeZone)}
            </h2>
            {activeZone === PENDING ? (
              pendingCount > 0 ? (
                <span className="text-[10px] font-semibold min-w-[1.15rem] h-5 px-1.5 rounded-full bg-amber-500 text-white flex items-center justify-center tabular-nums">
                  {pendingCount > 99 ? "99+" : pendingCount}
                </span>
              ) : (
                <span className="text-xs text-app-fg-tertiary">0</span>
              )
            ) : (
              <span className="text-xs text-app-fg-tertiary">{filtered.length}</span>
            )}
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
              <Select
                value={newZone}
                onChange={setNewZone}
                options={[
                  { value: "preferences", label: t("memory.groupPreferences") },
                  { value: "standards", label: t("memory.groupStandards") },
                  { value: "work", label: t("memory.groupWork") },
                  { value: "general", label: t("memory.groupOther") },
                ]}
              />
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center gap-4 flex-wrap">
                <div className="space-y-1">
                  <label className="block text-[10px] uppercase tracking-wide text-app-fg-tertiary">
                    {t("memory.scopeLabel")}
                  </label>
                  <Select
                    value={newScope}
                    onChange={setNewScope}
                    options={[
                      { value: "User", label: t("scope.user") },
                      { value: "Project", label: t("scope.project") },
                    ]}
                    className="w-36"
                  />
                </div>
                <label className="flex items-center gap-2 text-sm text-app-fg dark:text-slate-200 mt-4">
                  <input
                    type="checkbox"
                    checked={newPinned}
                    onChange={(e) => setNewPinned(e.target.checked)}
                  />
                  {t("memory.pinned")}
                </label>
                <div className="flex-1" />
                <div className="flex gap-2 mt-4">
                  <Button size="sm" variant="secondary" onClick={() => setShowCreate(false)}>
                    {t("memory.cancel")}
                  </Button>
                  <Button size="sm" onClick={handleCreate} disabled={!newBody.trim()}>
                    {t("memory.save")}
                  </Button>
                </div>
              </div>
              <p className="text-[11px] text-app-fg-tertiary leading-snug max-w-2xl">
                {t("memory.scopeHint")}
              </p>
              <p className="text-[11px] text-app-fg-tertiary">
                {newScope === "Project" ? t("scope.projectHint") : t("scope.userHint")}
              </p>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4 space-y-3">
          {showPendingBlock && (
            <PendingReviewSection
              onAccepted={() => {
                void fetchMemories();
                void fetchPendingCount();
              }}
              onChanged={() => void fetchPendingCount()}
            />
          )}

          {activeZone === PENDING && pendingCount === 0 && (
            <EmptyState
              icon={<Inbox size={22} strokeWidth={1.75} />}
              title={t("memory.pendingEmptyTitle")}
              description={t("memory.pendingEmpty")}
            />
          )}

          {activeZone !== PENDING && filtered.length === 0 && pendingCount === 0 && (
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
          {activeZone !== PENDING && filtered.map((mem) => (
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
                <span className="text-xs text-app-fg-tertiary">
                  {groupLabel(companionGroup(mem))}
                </span>
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
  badge,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  highlight?: boolean;
  /** Amber pill like the sidebar inbox badge */
  badge?: boolean;
}) {
  return (
    <div
      onClick={onClick}
      className={`flex items-center justify-between px-3 py-1.5 rounded-lg cursor-pointer text-sm transition-colors ${
        active
          ? badge
            ? "bg-amber-100/90 dark:bg-amber-950/40 text-amber-950 dark:text-amber-100 font-medium ring-1 ring-amber-300/80 dark:ring-amber-700/60"
            : ui.navItemActive
          : ui.navItemIdle
      }`}
    >
      <span
        className={`truncate ${
          highlight && !active ? "text-amber-700 dark:text-amber-300" : ""
        }`}
      >
        {label}
      </span>
      {badge && count > 0 ? (
        <span className="ml-2 text-[10px] font-semibold min-w-[1.15rem] h-4 px-1 rounded-full bg-amber-500 text-white flex items-center justify-center tabular-nums shrink-0">
          {count > 99 ? "99+" : count}
        </span>
      ) : (
        <span className="text-[11px] text-app-fg-tertiary ml-2 tabular-nums">{count}</span>
      )}
    </div>
  );
}


