import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileUp, Trash2 } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button, EmptyState, ui } from "../common/ui";
import { Pager, pageCount, pageSlice } from "../common/Pager";
import { ConfirmPopover } from "../common/ConfirmPopover";
import { toast } from "../../utils/toast";
import { bytesToBase64 } from "../../utils/attachments";

export type SourceRow = {
  id: string;
  title: string;
  originalName: string;
  ext: string;
  createdAt: string;
  readable: boolean;
  chars: number;
  originalMissing?: boolean;
  previous?: SourceRow | null;
};

function shortDate(iso: string): string {
  const d = iso.slice(0, 10);
  return d || iso;
}

type OutputItem = {
  relPath: string;
  name: string;
  ext: string;
  bytes: number;
  modified: string;
};

/** `day === null` → the file's date could not be read at all. */
type OutputGroup = { day: string | null; items: OutputItem[] };
type ListedOutput = OutputItem & { day: string | null };

/** "我带来的" vs "我产出的" — two different origins, two tabs, never one list. */
type MaterialsTab = "kept" | "produced";

/**
 * What the companion produced (`workspace/outputs/…`).
 *
 * Deliberately separate from the materials you brought in: this is the result,
 * not the source, so it is not fed to retrieval. Helper scripts are filtered out
 * server-side (a `.py` is process, not a deliverable).
 */
function OutputsSection({ query }: { query: string }) {
  const t = useUiStore((s) => s.t);
  const [groups, setGroups] = useState<OutputGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [page, setPage] = useState(1);

  useEffect(() => {
    let alive = true;
    void invoke<OutputGroup[]>("list_outputs")
      .then((rows) => {
        if (!alive) return;
        setGroups(rows);
        setFailed(false);
      })
      .catch(() => {
        if (alive) setFailed(true);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    setPage(1);
  }, [query]);

  const open = async (relPath: string) => {
    try {
      await invoke("open_output", { path: relPath });
    } catch {
      toast.error(t("materials.openError"));
    }
  };

  // One flat, newest-first list: the server already ordered day-then-time, and
  // the day is printed only where it changes on this page.
  const rows = useMemo<ListedOutput[]>(() => {
    const needle = query.toLowerCase();
    return groups.flatMap((g) =>
      g.items
        .filter((it) => !needle || it.name.toLowerCase().includes(needle))
        .map((it) => ({ ...it, day: g.day }))
    );
  }, [groups, query]);

  const shown = pageSlice(rows, page);

  if (loading) {
    return <p className="text-sm text-app-fg-tertiary">{t("common.loading")}</p>;
  }
  if (failed) {
    return <p className="text-sm text-app-fg-tertiary">{t("materials.outputsError")}</p>;
  }
  if (rows.length === 0) {
    return (
      <p className="text-sm text-app-fg-tertiary">
        {query ? t("materials.noMatch") : t("materials.outputsEmpty")}
      </p>
    );
  }

  return (
    <>
      <ul className="space-y-1.5">
        {shown.map((it, i) => {
          const prevDay = i > 0 ? shown[i - 1].day : null;
          const startsDay = it.day !== null && it.day !== prevDay;
          return (
            <li key={it.relPath}>
              {startsDay && (
                <p className="text-[11px] text-app-fg-tertiary mb-1">{it.day}</p>
              )}
              <div className={`${ui.card} px-3 py-2 flex items-center gap-3`}>
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-app-fg truncate">{it.name}</p>
                  <p className="text-[11px] text-app-fg-tertiary mt-0.5">
                    {it.modified}
                    {it.ext ? ` · ${it.ext.toUpperCase()}` : ""}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void open(it.relPath)}
                >
                  {t("materials.open")}
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
      <Pager page={page} total={rows.length} onPage={setPage} />
    </>
  );
}

export function MaterialsPanel() {
  const t = useUiStore((s) => s.t);
  const [items, setItems] = useState<SourceRow[]>([]);
  const [query, setQuery] = useState("");
  const [tab, setTab] = useState<MaterialsTab>("kept");
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [preview, setPreview] = useState<Record<string, string>>({});
  const fileRef = useRef<HTMLInputElement>(null);
  const folderRef = useRef<HTMLInputElement>(null);

  const reload = useCallback(async (q: string) => {
    try {
      const rows = await invoke<SourceRow[]>("list_sources", { query: q });
      setItems(rows);
      setLoadFailed(false);
    } catch {
      setLoadFailed(true);
      toast.error(t("materials.loadError"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      void reload(query.trim());
    }, 180);
    return () => window.clearTimeout(handle);
  }, [query, reload]);

  // A new tab or a new search starts at the top of its own list.
  useEffect(() => {
    setPage(1);
  }, [tab, query]);

  const onDelete = async (id: string) => {
    try {
      await invoke("delete_source", { id });
      setItems((prev) => prev.filter((x) => x.id !== id));
      setConfirmId(null);
    } catch {
      toast.error(t("materials.deleteError"));
    }
  };

  const onOpen = async (id: string) => {
    try {
      await invoke("open_source", { id });
    } catch {
      toast.error(t("materials.openError"));
    }
  };

  const onTogglePreview = async (id: string) => {
    if (openId === id) {
      setOpenId(null);
      return;
    }
    setOpenId(id);
    if (preview[id]) return;
    try {
      const text = await invoke<string>("preview_source", { id });
      setPreview((p) => ({ ...p, [id]: text }));
    } catch {
      setPreview((p) => ({ ...p, [id]: t("materials.previewError") }));
    }
  };

  const onPick = async (files: FileList | null) => {
    if (!files?.length) return;
    for (const file of Array.from(files)) {
      try {
        const buf = new Uint8Array(await file.arrayBuffer());
        const bytesBase64 = bytesToBase64(buf);
        const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
        if (ext === "pdf" || ext === "doc" || ext === "docx" || ext === "xlsx") {
          await invoke("import_document", {
            request: {
              sessionId: "__materials",
              fileName: file.name,
              bytesBase64,
              deleteOriginal: false,
            },
          });
        } else {
          const bodyMd =
            ext === "txt" || ext === "md"
              ? new TextDecoder().decode(buf)
              : undefined;
          await invoke("keep_source", {
            request: { fileName: file.name, bytesBase64, bodyMd },
          });
        }
      } catch {
        toast.error(t("chat.attachFailed"));
      }
    }
    if (fileRef.current) fileRef.current.value = "";
    // What you just dropped is a material — show it where it landed.
    setTab("kept");
    await reload(query.trim());
  };

  // Clamp: deleting the last row of the last page must not leave a blank page.
  const current = Math.min(page, pageCount(items.length));
  const shown = pageSlice(items, current);

  const tabBtn = (id: MaterialsTab, label: string) => (
    <button
      key={id}
      type="button"
      onClick={() => setTab(id)}
      className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
        tab === id
          ? "bg-app-surface dark:bg-slate-800 text-app-fg dark:text-slate-100 shadow-sm"
          : "text-app-fg-secondary hover:bg-app-muted/80 dark:hover:bg-slate-800/80 hover:text-app-fg"
      }`}
    >
      {label}
    </button>
  );

  return (
    <div
      className="flex-1 flex flex-col min-h-0"
      onDragEnter={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={(e) => {
        if (e.currentTarget.contains(e.relatedTarget as Node)) return;
        setDragOver(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        void onPick(e.dataTransfer.files);
      }}
    >
      <div className="shrink-0 px-5 py-3 flex flex-col gap-2.5">
        <div className="flex items-center gap-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("materials.search")}
            className={`${ui.input} flex-1`}
          />
          <input
            ref={fileRef}
            type="file"
            className="hidden"
            multiple
            accept=".pdf,.doc,.docx,.xlsx,.csv,.txt,.md"
            onChange={(e) => void onPick(e.target.files)}
          />
          <input
            ref={folderRef}
            type="file"
            className="hidden"
            multiple
            // @ts-expect-error webkitdirectory is not in React's input types
            webkitdirectory=""
            onChange={(e) => void onPick(e.target.files)}
          />
          {tab === "kept" && (
            <>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => fileRef.current?.click()}
              >
                <FileUp size={14} className="mr-1.5" />
                {t("materials.add")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => folderRef.current?.click()}
              >
                {t("materials.addFolder")}
              </Button>
            </>
          )}
        </div>
        <div className="inline-flex gap-0.5 p-0.5 rounded-xl bg-app-muted dark:bg-slate-800/80 self-start">
          {tabBtn("kept", t("materials.sectionKept"))}
          {tabBtn("produced", t("materials.sectionProduced"))}
        </div>
      </div>
      <div className={`flex-1 overflow-y-auto px-5 pb-6 ${dragOver ? "ring-2 ring-inset ring-app-primary/30 rounded-xl" : ""}`}>
        {tab === "produced" ? (
          <OutputsSection query={query.trim()} />
        ) : loading ? (
          <p className="text-sm text-app-fg-tertiary">{t("common.loading")}</p>
        ) : loadFailed ? (
          <EmptyState
            title={t("materials.loadError")}
            action={
              <Button size="sm" variant="secondary" onClick={() => void reload(query.trim())}>
                {t("common.retry")}
              </Button>
            }
          />
        ) : items.length === 0 && query.trim() ? (
          <p className="text-sm text-app-fg-tertiary">{t("materials.noMatch")}</p>
        ) : items.length === 0 ? (
          <EmptyState
            title={t("materials.empty")}
            action={
              <Button size="sm" variant="secondary" onClick={() => fileRef.current?.click()}>
                <FileUp size={14} className="mr-1.5" />
                {t("materials.add")}
              </Button>
            }
          />
        ) : (
          <>
            <ul className="space-y-2">
              {shown.map((row) => (
                <li
                  key={row.id}
                  className={`${ui.card} relative px-3 py-2.5 flex items-start gap-3`}
                >
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium text-app-fg truncate">
                      {row.title}
                    </p>
                    <p className="text-[11px] text-app-fg-tertiary mt-0.5">
                      {shortDate(row.createdAt)}
                      {row.ext ? ` · ${row.ext.toUpperCase()}` : ""}
                      {row.readable ? "" : ` · ${t("materials.unread")}`}
                      {row.originalMissing ? ` · ${t("materials.missingOriginal")}` : ""}
                    </p>
                    <button
                      type="button"
                      className="mt-1 text-[11px] text-app-primary hover:underline"
                      onClick={() => void onTogglePreview(row.id)}
                    >
                      {t("materials.preview")}
                    </button>
                    {openId === row.id && preview[row.id] !== undefined ? (
                      <p className="mt-2 text-xs leading-relaxed text-app-fg-secondary whitespace-pre-wrap max-h-40 overflow-y-auto">
                        {preview[row.id]}
                      </p>
                    ) : null}
                    {row.previous ? (
                      <button
                        type="button"
                        className="mt-1 text-[11px] text-app-fg-secondary hover:text-app-fg underline-offset-2 hover:underline"
                        onClick={() => void onOpen(row.previous!.id)}
                      >
                        {t("materials.previous")} · {row.previous.title} ·{" "}
                        {t("materials.openPrevious")}
                      </button>
                    ) : null}
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => void onOpen(row.id)}
                    >
                      {t("materials.open")}
                    </Button>
                    <button
                      type="button"
                      className="p-1.5 rounded-lg text-app-fg-tertiary hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950/40"
                      aria-label={t("materials.delete")}
                      onClick={() => setConfirmId(row.id)}
                    >
                      <Trash2 size={16} />
                    </button>
                    <ConfirmPopover
                      open={confirmId === row.id}
                      message={t("materials.deleteAsk")}
                      confirmLabel={t("materials.delete")}
                      danger
                      onCancel={() => setConfirmId(null)}
                      onConfirm={() => void onDelete(row.id)}
                    />
                  </div>
                </li>
              ))}
            </ul>
            <Pager page={current} total={items.length} onPage={setPage} />
          </>
        )}
      </div>
    </div>
  );
}
