import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Pin,
  PinOff,
  Trash2,
  Plus,
  Search,
  Brain,
  Inbox,
  ChevronDown,
  ChevronRight,
  Layers,
  FolderTree,
  Undo2,
} from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useNavStore } from "../../store/navStore";
import { Button, EmptyState, ui } from "../common/ui";
import { Pager, PAGE_SIZE, pageCount, pageSlice } from "../common/Pager";
import { Select } from "../common/Select";
import {
  companionGroup,
  memoryGroupLabel,
  type CompanionGroup,
} from "../../utils/memoryGroup";
import { ConfirmPopover } from "../common/ConfirmPopover";
import { toast } from "../../utils/toast";
import { notifyRemembered } from "../../utils/remembered";
import { PendingReviewSection } from "./PendingReviewSection";
import { errorText } from "../../utils/errorText";

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
  /** 归属：null = 全局；有值 = 一个人物或一个项目组。 */
  owner?: string | null;
  /** 归属给人看的样子（组名 / 人名 / 「全局」/「旧版角色」）。 */
  ownerName: string;
  /** 因为什么（当初教它的那句话）。老记忆没有。 */
  because?: string | null;
  /** 它取代了谁。非空 = 可以「退回上一版」。 */
  supersedes: string[];
  /** 第几版（沿取代链数出来的）。 */
  version: number;
}

interface TopicCardItem {
  id: string;
  title: string;
  summary: string;
  memberIds: string[];
  builtAt: string;
}

interface TopicCardsView {
  cards: TopicCardItem[];
  stale: boolean;
  unassigned: number;
  activeCount: number;
}

const ALL = "__all__";
/** 归属筛选里「只看全局」那一档（没有归属的记忆）。 */
const GLOBAL_ONLY = "__global__";
const PINNED = "__pinned__";
const PENDING = "__pending__";

export function MemoryPanel({ embedded = false }: { embedded?: boolean }) {
  const t = useUiStore((state) => state.t);
  const highlightMemoryId = useUiStore((s) => s.highlightMemoryId);
  const pendingFocusSeq = useNavStore((s) => s.pendingFocusSeq);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
  const [activeZone, setActiveZone] = useState<string>(ALL);
  const [query, setQuery] = useState("");
  /** 归属筛选：ALL = 全部；GLOBAL_ONLY = 只看全局；其余 = 那个归属的 id。 */
  const [ownerFilter, setOwnerFilter] = useState<string>(ALL);
  const [showCreate, setShowCreate] = useState(false);
  const [newBody, setNewBody] = useState("");
  const [newTags, setNewTags] = useState("");
  const [newZone, setNewZone] = useState("general");
  const [newScope, setNewScope] = useState("User");
  const [newPinned, setNewPinned] = useState(false);
  /** 同一个确认弹层服务两个动作；`revert` 决定文案与后果，不决定按钮位置。 */
  const [confirmAction, setConfirmAction] = useState<{ id: string; revert: boolean } | null>(
    null,
  );
  const [page, setPage] = useState(1);
  const [view, setView] = useState<"list" | "topics">("list");
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
  }, [highlightMemoryId, memories, page]);

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
      // 归属筛选：全局 / 某一个人物 / 某一个项目组。**只筛，不改**——
      // 归属只由 `resolve_owner` 决定，这一页不许改它（要改得说清楚，那是另一件事）。
      if (ownerFilter !== ALL) {
        const mine = m.owner ?? null;
        if (ownerFilter === GLOBAL_ONLY) {
          if (mine !== null) return false;
        } else if (mine !== ownerFilter) {
          return false;
        }
      }
      if (!q) return true;
      return (
        m.body.toLowerCase().includes(q) ||
        m.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    });
  }, [memories, activeZone, query, ownerFilter]);

  // Newest first (the command sorts by when it was learned), 10 per page.
  // Clamp: deleting the last row of the last page must not leave a blank page.
  const currentPage = Math.min(page, pageCount(filtered.length));
  const pageItems = pageSlice(filtered, currentPage);

  // A new zone or a new search starts at the top.
  useEffect(() => {
    setPage(1);
  }, [activeZone, query, ownerFilter]);

  // Highlighted from the dialogue: turn to the page that holds it.
  useEffect(() => {
    if (!highlightMemoryId) return;
    const idx = filtered.findIndex((m) => m.id === highlightMemoryId);
    if (idx >= 0) setPage(Math.floor(idx / PAGE_SIZE) + 1);
  }, [highlightMemoryId, filtered]);

  /** 数据里真出现过的归属（排序后），用来生成筛选项——不摆没有的选项。 */
  const ownerChoices = useMemo(() => {
    const seen = new Map<string, string>();
    for (const m of memories) {
      if (m.owner) seen.set(m.owner, m.ownerName);
    }
    return [...seen.entries()].sort((a, b) => a[1].localeCompare(b[1], "zh-CN"));
  }, [memories]);

  /** 三个虚拟筛选不是「分类」，先答它们，再问 `utils/memoryGroup`。 */
  const groupLabel = (id: string) => {
    if (id === ALL) return t("memory.all");
    if (id === PINNED) return t("memory.pinned");
    if (id === PENDING) return t("memory.pendingZone");
    return memoryGroupLabel(t, id);
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
      toast.error(errorText(e));
    }
  };

  const handleDelete = async (id: string, scope: string) => {
    try {
      await invoke("delete_memory", { id, scope });
      setConfirmAction(null);
      await fetchMemories();
      toast.success(t("toast.memoryDeleted"));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const handleTogglePin = async (id: string) => {
    try {
      await invoke("toggle_pin_memory", { id });
      await fetchMemories();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const viewBtn = (id: "list" | "topics", label: string) => (
    <button
      key={id}
      type="button"
      onClick={() => setView(id)}
      className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
        view === id
          ? "bg-app-surface dark:bg-slate-800 text-app-fg dark:text-slate-100 shadow-sm"
          : "text-app-fg-secondary hover:bg-app-muted/80 dark:hover:bg-slate-800/80 hover:text-app-fg"
      }`}
    >
      {label}
    </button>
  );

  /// One row, one look — the topic view expands members with this same row,
  /// so a memory never appears in two different shapes.
  const renderRow = (mem: MemoryItem) => (
    <div
      key={mem.id}
      ref={mem.id === highlightMemoryId ? highlightRef : undefined}
      className={`${ui.card} p-3.5 space-y-2 relative ${
        mem.id === highlightMemoryId ? "mem-highlight" : ""
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <p className="text-app-body flex-1 whitespace-pre-wrap text-app-fg dark:text-slate-100">
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
            onClick={() => setConfirmAction({ id: mem.id, revert: false })}
            className="p-1.5 rounded-lg hover:bg-red-50 dark:hover:bg-red-950/40 text-app-fg-secondary hover:text-app-danger"
            title={t("memory.delete")}
          >
            <Trash2 size={14} />
          </button>
          <ConfirmPopover
            open={confirmAction?.id === mem.id}
            message={
              confirmAction?.revert
                ? t("memory.revertConfirm")
                : t("memory.deleteConfirm")
            }
            onCancel={() => setConfirmAction(null)}
            onConfirm={() => void handleDelete(mem.id, mem.scope)}
            // 按的是「退回上一版」，按钮就不许写「删除」——两件事用户会当成一件。
            confirmLabel={
              confirmAction?.revert ? t("memory.revert") : t("common.delete")
            }
          />
        </div>
      </div>
      {mem.because && (
        <p className="text-app-sub leading-relaxed text-app-fg-secondary">
          {t("memory.because", { why: mem.because })}
        </p>
      )}
      <div className="flex items-center gap-2 flex-wrap">
        {mem.pinned && (
          <span className="text-xs px-1.5 py-0.5 rounded-md bg-amber-100 dark:bg-amber-900/40 text-amber-800 dark:text-amber-300">
            {t("memory.pinnedBadge")}
          </span>
        )}
        {/* 归属带图标：它和旁边的分组标签不是一回事（一个是「在哪儿算数」，一个是「哪一类」）。 */}
        <span
          className={`inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded-md ${
            mem.owner
              ? "bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300"
              : "bg-app-muted dark:bg-slate-800 text-app-fg-secondary"
          }`}
          title={t("memory.ownerHint")}
        >
          <FolderTree size={11} strokeWidth={2} className="shrink-0" />
          {mem.ownerName}
        </span>
        {mem.version > 1 && (
          <span className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
            {t("memory.versionBadge", { n: mem.version })}
          </span>
        )}
        {/*
          「退回上一版」以前只在垃圾桶图标的 `title` 里——鼠标不悬停就不知道有这个动作。
          它是第 3 期验收要走的一步（规格 §9 第 4 条），就得写成字。
        */}
        {mem.supersedes.length > 0 && (
          <button
            type="button"
            onClick={() => setConfirmAction({ id: mem.id, revert: true })}
            className="inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded-md border border-app-border dark:border-slate-700 text-app-fg-secondary hover:text-app-primary hover:border-app-primary/40 transition-colors duration-[var(--motion-fast)]"
          >
            <Undo2 size={12} strokeWidth={1.75} />
            {t("memory.revert")}
          </button>
        )}
        <span className="text-xs text-app-fg-tertiary">
          {groupLabel(companionGroup(mem))}
        </span>
      </div>
    </div>
  );

  const showPendingBlock = activeZone === PENDING || activeZone === ALL;

  return (
    <div className={`flex-1 flex h-full ${ui.page}`}>
      {view === "list" && (
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
      )}

      <div className="flex-1 flex flex-col min-w-0">
        <header className={`${ui.header} gap-3`}>
          <div className="flex items-center gap-2 min-w-0">
            <h2 className="text-base font-semibold truncate text-app-fg dark:text-slate-100">
              {view === "topics" ? t("memory.viewTopics") : groupLabel(activeZone)}
            </h2>
            {view === "list" && (activeZone === PENDING ? (
              pendingCount > 0 ? (
                <span className="text-xs font-semibold min-w-[1.15rem] h-5 px-1.5 rounded-full bg-amber-500 text-white flex items-center justify-center tabular-nums">
                  {pendingCount > 99 ? "99+" : pendingCount}
                </span>
              ) : (
                <span className="text-xs text-app-fg-tertiary">0</span>
              )
            ) : (
              <span className="text-xs text-app-fg-tertiary">{filtered.length}</span>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <div className="inline-flex gap-0.5 p-0.5 rounded-xl bg-app-muted dark:bg-slate-800/80">
              {viewBtn("list", t("memory.viewList"))}
              {viewBtn("topics", t("memory.viewTopics"))}
            </div>
            {view === "list" && (
            <>
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
            {/* 归属筛选（§5.4 管理可见）：教的那句话落在谁名下，得能挑出来看 */}
            <select
              value={ownerFilter}
              onChange={(e) => setOwnerFilter(e.target.value)}
              className="py-1.5 pl-2 pr-1.5 text-xs rounded-lg border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 text-app-fg-secondary focus:outline-none focus:ring-2 focus:ring-app-primary/30"
              title={t("memory.ownerFilter")}
            >
              <option value={ALL}>{t("memory.ownerAll")}</option>
              <option value={GLOBAL_ONLY}>{t("memory.ownerGlobal")}</option>
              {ownerChoices.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
            <Button size="sm" onClick={() => setShowCreate(!showCreate)}>
              <Plus size={14} />
              {t("memory.new")}
            </Button>
            </>
            )}
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
                  <label className="block text-xs uppercase tracking-wide text-app-fg-tertiary">
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
              <p className="text-app-sub text-app-fg-secondary leading-snug max-w-2xl">
                {t("memory.scopeHint")}
              </p>
              <p className="text-app-sub text-app-fg-secondary">
                {newScope === "Project" ? t("scope.projectHint") : t("scope.userHint")}
              </p>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4 space-y-3">
          {view === "topics" ? (
            <TopicCardsSection memories={memories} renderRow={renderRow} />
          ) : (
            <>
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
          {activeZone !== PENDING && pageItems.map((mem) => renderRow(mem))}
          {activeZone !== PENDING && (
            <Pager page={currentPage} total={filtered.length} onPage={setPage} />
          )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/// Topic cards — the distilled view of the same memories.
///
/// Cards are derived: building or regrouping never edits a memory, so pressing
/// the button *is* the nod and there is no second confirmation.
function TopicCardsSection({
  memories,
  renderRow,
}: {
  memories: MemoryItem[];
  renderRow: (m: MemoryItem) => ReactNode;
}) {
  const t = useUiStore((state) => state.t);
  const [cards, setCards] = useState<TopicCardsView | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const byId = useMemo(() => {
    const map = new Map<string, MemoryItem>();
    for (const m of memories) map.set(m.id, m);
    return map;
  }, [memories]);

  const reload = async () => {
    try {
      setCards(await invoke<TopicCardsView>("list_topic_cards"));
      setFailed(null);
    } catch (e) {
      setFailed(String(e));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const build = async (rebuild: boolean) => {
    setBusy(true);
    setFailed(null);
    try {
      setCards(await invoke<TopicCardsView>("build_topic_cards", { rebuild }));
      setExpanded(null);
    } catch (e) {
      setFailed(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (busy) {
    return (
      <p className="text-sm text-app-fg-tertiary">
        {t("memory.topicsBuilding", { count: cards?.activeCount ?? memories.length })}
      </p>
    );
  }

  if (failed) {
    return (
      <div className="space-y-2">
        <p className="text-sm text-app-danger">
          {t("memory.topicsFailed", { error: failed })}
        </p>
        <Button size="sm" variant="secondary" onClick={() => void reload()}>
          {t("common.retry")}
        </Button>
      </div>
    );
  }

  if (!cards) {
    return <p className="text-sm text-app-fg-tertiary">{t("common.loading")}</p>;
  }

  if (cards.cards.length === 0) {
    return (
      <EmptyState
        icon={<Layers size={22} strokeWidth={1.75} />}
        title={t("memory.topicsEmptyTitle")}
        description={t("memory.topicsEmpty")}
        action={
          <Button size="sm" onClick={() => void build(true)}>
            {t("memory.topicsBuild")}
          </Button>
        }
      />
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-app-fg-tertiary">
          {t("memory.topicsSummary", {
            cards: cards.cards.length,
            count: cards.activeCount,
          })}
        </span>
        <Button size="sm" variant="secondary" onClick={() => void build(false)}>
          {t("memory.topicsRebuild")}
        </Button>
      </div>

      {cards.stale && (
        <div className="flex items-center justify-between gap-2 rounded-lg border border-app-border dark:border-slate-700 px-3 py-2">
          <span className="text-xs text-app-fg-secondary">{t("memory.topicsStale")}</span>
          <Button size="sm" variant="ghost" onClick={() => void build(false)}>
            {t("memory.topicsRebuild")}
          </Button>
        </div>
      )}

      {cards.cards.map((card) => {
        const members = card.memberIds
          .map((id) => byId.get(id))
          .filter((m): m is MemoryItem => Boolean(m));
        const open = expanded === card.id;
        return (
          <div key={card.id} className={`${ui.card} p-3.5 space-y-2`}>
            <button
              type="button"
              onClick={() => setExpanded(open ? null : card.id)}
              className="flex items-center justify-between gap-2 w-full text-left"
            >
              <span className="flex items-center gap-2 min-w-0">
                {open ? (
                  <ChevronDown size={14} className="shrink-0 text-app-fg-tertiary" />
                ) : (
                  <ChevronRight size={14} className="shrink-0 text-app-fg-tertiary" />
                )}
                <span className="text-app-body font-medium truncate text-app-fg dark:text-slate-100">
                  {card.title}
                </span>
              </span>
              <span className="text-xs text-app-fg-tertiary shrink-0">
                {t("memory.topicsMembers", { count: members.length })}
              </span>
            </button>
            {card.summary && (
              <p className="text-xs whitespace-pre-wrap text-app-fg-secondary pl-[22px]">
                {card.summary}
              </p>
            )}
            {open &&
              (members.length === 0 ? (
                <p className="text-xs text-app-fg-tertiary pl-[22px]">
                  {t("memory.topicsNoMembers")}
                </p>
              ) : (
                <div className="space-y-2 pt-1">{members.map((m) => renderRow(m))}</div>
              ))}
          </div>
        );
      })}
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
        <span className="ml-2 text-xs font-semibold min-w-[1.15rem] h-4 px-1 rounded-full bg-amber-500 text-white flex items-center justify-center tabular-nums shrink-0">
          {count > 99 ? "99+" : count}
        </span>
      ) : (
        <span className="text-xs text-app-fg-tertiary ml-2 tabular-nums">{count}</span>
      )}
    </div>
  );
}
