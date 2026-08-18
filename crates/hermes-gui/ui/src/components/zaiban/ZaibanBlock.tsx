import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { useNavStore } from "../../store/navStore";
import { useUiStore } from "../../store/uiStore";
import {
  bindZaibanListener,
  useZaibanStore,
  type ZaibanItem,
} from "../../store/zaibanStore";
import { toast } from "../../utils/toast";
import { DueChips, dueLabel } from "./DueChips";

export function ZaibanBlock() {
  const t = useUiStore((s) => s.t);
  const setPanel = useNavStore((s) => s.setPanel);
  const loadSession = useChatStore((s) => s.loadSession);
  const activeSessionId = useChatStore((s) => s.activeSessionId);
  const { list, error, highlightId, refresh, setPendingStart } = useZaibanStore();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [due, setDue] = useState("");
  const [near, setNear] = useState<ZaibanItem | null>(null);
  const [busy, setBusy] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editDue, setEditDue] = useState("");
  const [acceptDue, setAcceptDue] = useState<Record<string, string>>({});
  const [confirmDrop, setConfirmDrop] = useState<string | null>(null);
  const [mode, setMode] = useState<"idle" | "edit">("idle");

  useEffect(() => {
    bindZaibanListener();
    void refresh();
  }, [refresh]);

  const suggested = list?.items.filter((i) => i.status === "suggested") ?? [];
  const owed = list?.items.filter((i) => i.status === "open" || i.status === "waiting") ?? [];
  const recentDone = list?.recentDone ?? [];

  const fail = (e: unknown) => {
    const msg = String(e);
    if (msg.includes("due_vague")) toast.error(t("zaiban.dueVague"));
    else if (msg.includes("due_required")) toast.error(t("zaiban.dueNeed"));
    else toast.error(t("zaiban.saveError"));
  };

  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
      await refresh();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const create = async (opts?: { mergeInto?: string; forceNew?: boolean }) => {
    const title = draft.trim();
    if (!title) return;
    if (!due.trim()) {
      toast.error(t("zaiban.dueNeed"));
      return;
    }
    await act(async () => {
      const out = await invoke<{ status: string; existing?: ZaibanItem }>(
        "create_commitment",
        {
          title,
          mergeInto: opts?.mergeInto ?? null,
          forceNew: opts?.forceNew ?? false,
          sessionId: activeSessionId,
          softDue: due,
        }
      );
      if (out.status === "near" && out.existing) {
        setNear(out.existing);
        return;
      }
      setDraft("");
      setAdding(false);
      setNear(null);
    });
  };

  const start = (item: ZaibanItem) => {
    setPanel("chat");
    if (!item.doneWhen) {
      setPendingStart({ id: item.id, title: item.title });
    } else {
      useUiStore.getState().setComposerPrefill(`${t("zaiban.startPrefix")}${item.title}`);
    }
  };

  const openSource = async (sessionId?: string | null) => {
    if (!sessionId) return;
    try {
      const path = await invoke<string | null>("find_session_path", { sessionId });
      if (path) {
        setPanel("chat");
        await loadSession(path);
      } else {
        toast.info(t("zaiban.noSession"));
      }
    } catch {
      toast.info(t("zaiban.noSession"));
    }
  };

  return (
    <div className="px-5 pt-1 pb-5 shrink-0">
      <div className="flex items-center gap-1 px-0.5 mb-2">
        <span className="text-[13px] text-app-fg-secondary">{t("zaiban.drawerLead")}</span>
        <button
          type="button"
          className="ml-auto text-[12px] font-medium text-app-primary hover:opacity-80 px-1.5 py-0.5 rounded-md"
          onClick={() => {
            setAdding((v) => !v);
            setNear(null);
          }}
        >
          {t("zaiban.add")}
        </button>
      </div>

      {error && (
        <p className="px-1 text-[11px] text-app-fg-tertiary">{t("zaiban.loadError")}</p>
      )}

      {list && !error && suggested.length === 0 && owed.length === 0 && !adding && (
        <div className="px-0.5 space-y-2">
          <p className="text-[13px] text-app-fg-tertiary leading-relaxed">{t("zaiban.empty")}</p>
          <DueChips
            value={due}
            onChange={(v) => {
              setDue(v);
              setAdding(true);
            }}
          />
        </div>
      )}

      {adding && (
        <div className="px-0.5 mb-3 space-y-2">
          <input
            autoFocus
            value={draft}
            disabled={busy}
            onChange={(e) => {
              setDraft(e.target.value);
              setNear(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void create();
              }
              if (e.key === "Escape") setAdding(false);
            }}
            placeholder={t("zaiban.addPlaceholder")}
            className="w-full px-2 py-1.5 text-xs rounded-lg border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800 text-app-fg select-text focus:outline-none focus:ring-2 focus:ring-app-primary/30"
          />
          <DueChips value={due} onChange={setDue} />
          {near && (
            <div className="text-[11px] text-app-fg-secondary leading-snug">
              {t("zaiban.mergeAsk", { title: near.title })}
              <div className="mt-1 flex gap-2">
                <button
                  type="button"
                  className="text-app-primary font-medium"
                  onClick={() => void create({ mergeInto: near.id })}
                >
                  {t("zaiban.merge")}
                </button>
                <button type="button" onClick={() => void create({ forceNew: true })}>
                  {t("zaiban.stillNew")}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {suggested.map((item) => (
        <div key={item.id} className="px-0.5 py-2 space-y-1.5 text-[12px] text-app-fg-secondary">
          <p className="leading-snug opacity-80">{item.title}</p>
          <DueChips
            value={acceptDue[item.id] ?? item.softDue ?? ""}
            onChange={(v) => setAcceptDue((m) => ({ ...m, [item.id]: v }))}
          />
          <div className="flex gap-2">
            <button
              type="button"
              className="text-app-primary font-medium"
              disabled={busy}
              onClick={() => {
                const picked = (acceptDue[item.id] ?? item.softDue ?? "").trim();
                if (!picked) {
                  toast.error(t("zaiban.dueNeed"));
                  return;
                }
                void act(async () =>
                  invoke("accept_commitment", {
                    id: item.id,
                    softDue: picked,
                  })
                );
              }}
            >
              {t("zaiban.accept")}
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => void act(async () => invoke("reject_commitment", { id: item.id }))}
            >
              {t("zaiban.reject")}
            </button>
          </div>
        </div>
      ))}

      {owed.map((item) => {
        const editing = openId === item.id && mode === "edit";
        const hi = highlightId === item.id;
        const label = dueLabel(item, t);
        return (
          <div
            key={item.id}
            className={`rounded-xl border border-app-border/80 dark:border-slate-800 px-3 py-2.5 mb-2 ${
              hi ? "ring-1 ring-app-primary/40" : ""
            } ${editing ? "bg-app-muted/40 dark:bg-slate-800/40" : "bg-app-surface dark:bg-slate-900"}`}
          >
            <button type="button" className="w-full text-left" onClick={() => start(item)}>
              <span className="block text-[14px] leading-snug text-app-fg dark:text-slate-100">
                {item.status === "waiting" && (
                  <span className="text-app-fg-tertiary mr-1">{t("zaiban.waiting")}</span>
                )}
                {item.title}
              </span>
              {label && (
                <span
                  className={`block text-[12px] mt-1 ${
                    item.overdue
                      ? "text-amber-700 dark:text-amber-400"
                      : item.dueToday
                        ? "text-app-fg font-medium"
                        : "text-app-fg-secondary"
                  }`}
                >
                  {label}
                </span>
              )}
            </button>
            <div className="mt-2.5 flex flex-wrap gap-1.5">
              <RowBtn onClick={() => void act(async () => invoke("close_commitment", { id: item.id, dropped: false }))}>
                {t("zaiban.done")}
              </RowBtn>
              <RowBtn
                onClick={() => {
                  setOpenId(editing ? null : item.id);
                  setMode("edit");
                  setEditTitle(item.title);
                  setEditDue(item.softDue || item.dueDate || "");
                }}
              >
                {t("zaiban.edit")}
              </RowBtn>
              {confirmDrop === item.id ? (
                <>
                  <RowBtn
                    danger
                    onClick={() =>
                      void act(async () => {
                        await invoke("close_commitment", { id: item.id, dropped: true });
                        setConfirmDrop(null);
                      })
                    }
                  >
                    {t("zaiban.dropConfirm")}
                  </RowBtn>
                  <RowBtn onClick={() => setConfirmDrop(null)}>{t("common.cancel")}</RowBtn>
                </>
              ) : (
                <RowBtn onClick={() => setConfirmDrop(item.id)}>{t("zaiban.drop")}</RowBtn>
              )}
            </div>
            {editing && (
              <div className="mt-3 space-y-2 pt-3 border-t border-app-border/70 dark:border-slate-800">
                <input
                  value={editTitle}
                  autoFocus
                  onChange={(e) => setEditTitle(e.target.value)}
                  className="w-full px-2 py-1.5 text-[13px] rounded-lg border border-app-border dark:border-slate-700 bg-app-bg dark:bg-slate-950 text-app-fg select-text"
                />
                <DueChips value={editDue} onChange={setEditDue} />
                <button
                  type="button"
                  className="text-[13px] font-medium text-app-primary"
                  onClick={() =>
                    void act(async () => {
                      if (editTitle.trim() && editTitle.trim() !== item.title) {
                        await invoke("update_commitment", {
                          id: item.id,
                          title: editTitle.trim(),
                          doneWhen: null,
                          softDue: null,
                          note: null,
                          waiting: null,
                        });
                      }
                      if (editDue.trim()) {
                        await invoke("update_commitment", {
                          id: item.id,
                          title: null,
                          doneWhen: null,
                          softDue: editDue,
                          note: null,
                          waiting: null,
                        });
                      }
                      setMode("idle");
                      setOpenId(null);
                    })
                  }
                >
                  {t("zaiban.saveEdit")}
                </button>
              </div>
            )}
            {item.sessionId && (
              <button
                type="button"
                className="mt-2 text-[11px] text-app-fg-tertiary hover:text-app-fg-secondary"
                onClick={() => void openSource(item.sessionId)}
              >
                {t("zaiban.source")}
              </button>
            )}
          </div>
        );
      })}

      {list?.crowded && (
        <p className="px-1 text-[10px] text-amber-700 dark:text-amber-400">{t("zaiban.crowded")}</p>
      )}
      {recentDone.length > 0 && (
        <details className="mt-4 px-0.5">
          <summary className="text-[11px] text-app-fg-tertiary cursor-pointer select-none">
            {t("zaiban.recentDone")}
          </summary>
          <ul className="mt-2 space-y-1.5">
            {recentDone.map((item) => (
              <li
                key={item.id}
                className="text-[12px] text-app-fg-tertiary line-through decoration-app-border"
              >
                {item.title}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

function RowBtn({
  children,
  onClick,
  danger,
}: {
  children: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={`px-2.5 py-1 rounded-lg text-[12px] border ${
        danger
          ? "border-app-danger/40 text-app-danger"
          : "border-app-border dark:border-slate-700 text-app-fg-secondary hover:text-app-fg hover:border-app-fg/20"
      }`}
    >
      {children}
    </button>
  );
}
