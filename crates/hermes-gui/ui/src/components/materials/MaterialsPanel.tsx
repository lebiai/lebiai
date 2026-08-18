import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileUp, Trash2 } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button, EmptyState, ui } from "../common/ui";
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

export function MaterialsPanel() {
  const t = useUiStore((s) => s.t);
  const [items, setItems] = useState<SourceRow[]>([]);
  const [query, setQuery] = useState("");
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
    await reload(query.trim());
  };

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
      <div className="shrink-0 px-5 py-3 flex items-center gap-2">
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
      </div>
      <div className={`flex-1 overflow-y-auto px-5 pb-6 ${dragOver ? "ring-2 ring-inset ring-app-primary/30 rounded-xl" : ""}`}>
        {loading ? (
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
          <ul className="space-y-2">
            {items.map((row) => (
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
        )}
      </div>
    </div>
  );
}
