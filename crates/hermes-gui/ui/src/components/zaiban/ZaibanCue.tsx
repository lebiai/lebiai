import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { useZaibanStore, type ZaibanItem } from "../../store/zaibanStore";
import { useWorkDrawerStore } from "../../store/workDrawerStore";
import { toast } from "../../utils/toast";
import { Button } from "../common/ui";
import { DueChips } from "./DueChips";

const OVERDUE_ASKED_KEY = "lebi.zaiban.overdueAsked";

function localDay(): string {
  const d = new Date();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

function askedOverdueToday(id: string): boolean {
  try {
    const raw = localStorage.getItem(OVERDUE_ASKED_KEY);
    if (!raw) return false;
    const parsed = JSON.parse(raw) as { day?: string; ids?: string[] };
    return parsed.day === localDay() && (parsed.ids ?? []).includes(id);
  } catch {
    return false;
  }
}

function markOverdueAsked(id: string): void {
  const day = localDay();
  let ids: string[] = [];
  try {
    const raw = localStorage.getItem(OVERDUE_ASKED_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as { day?: string; ids?: string[] };
      if (parsed.day === day) ids = parsed.ids ?? [];
    }
  } catch {
    ids = [];
  }
  if (!ids.includes(id)) ids.push(id);
  localStorage.setItem(OVERDUE_ASKED_KEY, JSON.stringify({ day, ids }));
}

/** One guaranteed line + one primary action. Not a banner wall. */
export function ZaibanCue() {
  const t = useUiStore((s) => s.t);
  const messages = useChatStore((s) => s.messages);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const {
    list,
    streamCue,
    pendingRedue,
    dismissedMerge,
    clearStreamCue,
    setPendingRedue,
    dismissMerge,
    refresh,
  } = useZaibanStore();
  const setComposerPrefill = useUiStore((s) => s.setComposerPrefill);
  const prefs = useWorkDrawerStore((s) => s.prefs);
  const refreshPrefs = useWorkDrawerStore((s) => s.refreshPrefs);
  const openTo = useWorkDrawerStore((s) => s.openTo);
  const [holdingOverdue, setHoldingOverdue] = useState<string | null>(null);

  useEffect(() => {
    void refreshPrefs();
  }, [refreshPrefs]);

  const suggested = list?.items.find((i) => i.status === "suggested");
  const owed = list?.items.filter((i) => i.status === "open" || i.status === "waiting") ?? [];
  const overdue = owed.find(
    (i) => i.overdue && (!askedOverdueToday(i.id) || holdingOverdue === i.id)
  );

  if (isStreaming) return null;
  const tightest = owed[0];
  const merge = list?.mergeHint;
  const mergeKey = merge ? `${merge.keepId}:${merge.otherId}` : "";
  const emptyThread = messages.length === 0;

  const act = async (fn: () => Promise<void>) => {
    try {
      await fn();
      await refresh();
    } catch {
      toast.error(t("zaiban.saveError"));
    }
  };

  if (pendingRedue) {
    return (
      <Cue>
        <span className="shrink-0">{t("zaiban.redueAsk", { title: pendingRedue.title })}</span>
        <RedueForm
          onPick={(phrase) =>
            void act(async () => {
              await invoke("update_commitment", {
                id: pendingRedue.id,
                title: null,
                doneWhen: null,
                softDue: phrase,
                note: null,
                waiting: null,
              });
              setPendingRedue(null);
            })
          }
        />
      </Cue>
    );
  }

  if (overdue) {
    return (
      <OverdueCue
        item={overdue}
        act={act}
        t={t}
        onShown={(id) => setHoldingOverdue(id)}
        onStillDo={() =>
          setPendingRedue({
            id: overdue.id,
            title: overdue.title,
          })
        }
      />
    );
  }

  if (streamCue?.action === "near" && streamCue.existingId) {
    return (
      <NearCue
        title={streamCue.title}
        existingId={streamCue.existingId}
        existingTitle={streamCue.existingTitle || streamCue.existingId}
        act={act}
        onDone={clearStreamCue}
        t={t}
      />
    );
  }

  if (streamCue?.action === "saved" && streamCue.title) {
    return (
      <Cue>
        <button
          type="button"
          className="text-left flex-1 min-w-0"
          onClick={() => useZaibanStore.getState().setHighlight(streamCue.id ?? null)}
        >
          {t("zaiban.noted", { title: streamCue.title })}
        </button>
        <Button size="sm" variant="ghost" onClick={() => clearStreamCue()}>
          {t("common.dismiss")}
        </Button>
      </Cue>
    );
  }

  if (suggested) {
    return (
      <SuggestedCue item={suggested} act={act} t={t} />
    );
  }

  if (prefs?.inviteDue) {
    return (
      <Cue>
        <span>{t("review.invite")}</span>
        <Button
          size="sm"
          onClick={() => {
            openTo("review");
          }}
        >
          {t("review.tab")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            void invoke("dismiss_review_invite").then(() => refreshPrefs());
          }}
        >
          {t("common.dismiss")}
        </Button>
      </Cue>
    );
  }

  if (merge && dismissedMerge !== mergeKey) {
    return (
      <Cue>
        <span>
          {t("zaiban.pairAsk", { a: merge.keepTitle, b: merge.otherTitle })}
        </span>
        <Button
          size="sm"
          onClick={() =>
            void act(async () => {
              await invoke("merge_commitments", {
                keepId: merge.keepId,
                otherId: merge.otherId,
              });
            })
          }
        >
          {t("zaiban.merge")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => dismissMerge(mergeKey)}>
          {t("zaiban.notSame")}
        </Button>
      </Cue>
    );
  }

  if (emptyThread && owed.length > 0 && tightest) {
    return (
      <Cue>
        <span>
          {t("zaiban.openSummary", { n: owed.length, title: tightest.title })}
        </span>
        <Button
          size="sm"
          onClick={() =>
            setComposerPrefill(`${t("zaiban.startPrefix")}${tightest.title}`)
          }
        >
          {t("zaiban.start")}
        </Button>
      </Cue>
    );
  }

  return null;
}

function DropConfirm({
  act,
  id,
  t,
}: {
  act: (fn: () => Promise<void>) => Promise<void>;
  id: string;
  t: (k: "zaiban.drop" | "zaiban.dropAsk" | "common.cancel") => string;
}) {
  const [ask, setAsk] = useState(false);
  if (!ask) {
    return (
      <Button size="sm" variant="ghost" onClick={() => setAsk(true)}>
        {t("zaiban.drop")}
      </Button>
    );
  }
  return (
    <span className="inline-flex items-center gap-1">
      <span className="text-xs text-app-fg-secondary">{t("zaiban.dropAsk")}</span>
      <Button
        size="sm"
        variant="danger"
        onClick={() => void act(async () => invoke("close_commitment", { id, dropped: true }))}
      >
        {t("zaiban.drop")}
      </Button>
      <Button size="sm" variant="ghost" onClick={() => setAsk(false)}>
        {t("common.cancel")}
      </Button>
    </span>
  );
}

function OverdueCue({
  item,
  act,
  t,
  onShown,
  onStillDo,
}: {
  item: ZaibanItem;
  act: (fn: () => Promise<void>) => Promise<void>;
  t: (
    k:
      | "zaiban.overdueAsk"
      | "zaiban.overdueWaitAsk"
      | "zaiban.stillDo"
      | "zaiban.done"
      | "zaiban.drop"
      | "zaiban.dropAsk"
      | "common.cancel",
    p?: { title: string }
  ) => string;
  onShown: (id: string) => void;
  onStillDo: () => void;
}) {
  useEffect(() => {
    markOverdueAsked(item.id);
    onShown(item.id);
    // Only when this overdue row is actually on screen — not when a higher cue is showing.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- onShown is setState
  }, [item.id]);

  return (
    <Cue>
      <span>
        {t(item.status === "waiting" ? "zaiban.overdueWaitAsk" : "zaiban.overdueAsk", {
          title: item.title,
        })}
      </span>
      <Button size="sm" onClick={onStillDo}>
        {t("zaiban.stillDo")}
      </Button>
      <Button
        size="sm"
        variant="secondary"
        onClick={() =>
          void act(async () => invoke("close_commitment", { id: item.id, dropped: false }))
        }
      >
        {t("zaiban.done")}
      </Button>
      <DropConfirm
        act={act}
        id={item.id}
        t={t}
      />
    </Cue>
  );
}

function NearCue({
  title,
  existingId,
  existingTitle,
  act,
  onDone,
  t,
}: {
  title?: string | null;
  existingId: string;
  existingTitle: string;
  act: (fn: () => Promise<void>) => Promise<void>;
  onDone: () => void;
  t: (k: "zaiban.mergeAsk" | "zaiban.sameOne" | "zaiban.stillNew" | "zaiban.dueNeed", p?: { title: string }) => string;
}) {
  const [due, setDue] = useState("");
  const [needDue, setNeedDue] = useState(false);
  return (
    <Cue>
      <span>{t("zaiban.mergeAsk", { title: existingTitle })}</span>
      <Button
        size="sm"
        onClick={() =>
          void act(async () => {
            if (title) {
              await invoke("create_commitment", {
                title,
                mergeInto: existingId,
                forceNew: false,
                sessionId: null,
              });
            }
            onDone();
          })
        }
      >
        {t("zaiban.sameOne")}
      </Button>
      {needDue ? (
        <div className="flex flex-wrap items-center gap-2 w-full">
          <DueChips value={due} onChange={setDue} />
          <Button
            size="sm"
            onClick={() => {
              if (!due.trim()) {
                toast.error(t("zaiban.dueNeed"));
                return;
              }
              void act(async () => {
                if (title) {
                  await invoke("create_commitment", {
                    title,
                    mergeInto: null,
                    forceNew: true,
                    sessionId: null,
                    softDue: due,
                  });
                }
                onDone();
              });
            }}
          >
            {t("zaiban.stillNew")}
          </Button>
        </div>
      ) : (
        <Button size="sm" variant="ghost" onClick={() => setNeedDue(true)}>
          {t("zaiban.stillNew")}
        </Button>
      )}
    </Cue>
  );
}

function SuggestedCue({
  item,
  act,
  t,
}: {
  item: ZaibanItem;
  act: (fn: () => Promise<void>) => Promise<void>;
  t: (
    k:
      | "zaiban.suggestAsk"
      | "zaiban.accept"
      | "zaiban.reject"
      | "zaiban.dueNeed",
    p?: { title: string }
  ) => string;
}) {
  const [due, setDue] = useState(item.softDue || item.dueDate || "");
  const needsDay = !item.softDue && !item.dueDate;
  return (
    <Cue>
      <span>{t("zaiban.suggestAsk", { title: item.title })}</span>
      {needsDay && <DueChips value={due} onChange={setDue} />}
      <Button
        size="sm"
        onClick={() => {
          if (needsDay && !due.trim()) {
            toast.error(t("zaiban.dueNeed"));
            return;
          }
          void act(async () =>
            invoke("accept_commitment", {
              id: item.id,
              softDue: due,
            })
          );
        }}
      >
        {t("zaiban.accept")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        onClick={() => void act(async () => invoke("reject_commitment", { id: item.id }))}
      >
        {t("zaiban.reject")}
      </Button>
    </Cue>
  );
}

function RedueForm({ onPick }: { onPick: (phrase: string) => void }) {
  const [due, setDue] = useState("");
  const t = useUiStore((s) => s.t);
  return (
    <div className="flex flex-wrap items-center gap-2 w-full">
      <DueChips value={due} onChange={setDue} />
      <Button
        size="sm"
        onClick={() => {
          if (!due.trim()) {
            toast.error(t("zaiban.dueNeed"));
            return;
          }
          onPick(due);
        }}
      >
        {t("zaiban.saveDue")}
      </Button>
    </div>
  );
}

function Cue({ children }: { children: ReactNode }) {
  return (
    <div className="shrink-0 mx-4 mt-2 mb-1 px-3 py-2 rounded-xl border border-app-border dark:border-slate-700 bg-app-surface/90 dark:bg-slate-900/80 text-[12px] text-app-fg-secondary dark:text-slate-300 flex flex-wrap items-center gap-2">
      {children}
    </div>
  );
}
