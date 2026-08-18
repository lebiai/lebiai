import { useState, useRef, useCallback, useEffect } from "react";
import { Send, Square, Paperclip, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { Button } from "../common/ui";
import { toast } from "../../utils/toast";
import {
  isFirstKeepTipPending,
  markFirstKeepTipSeen,
} from "../../utils/materialsTip";
import type { TranslationKey } from "../../i18n";
import type { FileImportResult } from "../../types/upload";
import {
  bytesToBase64,
  formatAttachmentsBlock,
  humanizeImportError,
  importErrorCode,
  isAcceptedDocumentFile,
  wantsSpokenKeep,
} from "../../utils/attachments";
import { filesFromDataTransfer, isLikelyFolderDummy } from "../../utils/dropFiles";
import { AttachmentChip } from "./AttachmentCards";

const ACCEPT =
  ".pdf,.doc,.docx,.xlsx,.csv,.txt,.md,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,text/csv,text/plain,text/markdown";

type PendingChip = {
  localId: string;
  fileName: string;
  status: "pending" | "ready" | "error";
  result?: FileImportResult;
  error?: string;
  bytesBase64?: string;
};

function announceKept(
  result: FileImportResult,
  t: (key: TranslationKey, vars?: Record<string, string | number>) => string,
  unread = false,
) {
  const kept = result.kept;
  if (!kept?.item?.id) return;
  const title = kept.item.title || result.originalName;
  if (kept.kind === "duplicate") {
    toast.info(t("materials.already"));
    return;
  }

  const undo = {
    label: t("materials.keptUndo"),
    onClick: () => {
      void invoke("undo_source", { id: kept.item.id })
        .then(() => toast.info(t("materials.undone")))
        .catch(() => toast.error(t("materials.deleteError")));
    },
  };

  if (unread || kept.item.readable === false) {
    toast.success(t("materials.unread"), 8000, undo);
    markFirstKeepTipSeen();
    return;
  }
  if (kept.kind === "new_version") {
    toast.success(t("materials.newVersion", { title }), 8000, undo);
    markFirstKeepTipSeen();
    return;
  }
  if (!isFirstKeepTipPending()) {
    return;
  }
  markFirstKeepTipSeen();
  toast.success(t("materials.keptFirst", { title }), 8000, undo);
}

export function InputArea() {
  const [input, setInput] = useState("");
  const [chips, setChips] = useState<PendingChip[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const dragDepth = useRef(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const { isStreaming, sendMessage, cancelStream, activeSessionId, sessions, activeReadOnly } =
    useChatStore();
  const readOnly =
    activeReadOnly || !!sessions.find((s) => s.id === activeSessionId)?.readOnly;
  const t = useUiStore((s) => s.t);
  const composerPrefill = useUiStore((s) => s.composerPrefill);
  const clearComposerPrefill = useUiStore((s) => s.clearComposerPrefill);

  const readyAttachments = chips
    .filter((c) => c.status === "ready" && c.result)
    .map((c) => c.result!);
  const importing = chips.some((c) => c.status === "pending");

  useEffect(() => {
    setChips([]);
    setInput("");
    setDragOver(false);
    dragDepth.current = 0;
  }, [activeSessionId]);

  useEffect(() => {
    if (composerPrefill == null) return;
    setInput(composerPrefill);
    clearComposerPrefill();
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 200) + "px";
      el.setSelectionRange(el.value.length, el.value.length);
    });
  }, [composerPrefill, clearComposerPrefill]);

  const canSend =
    !!activeSessionId &&
    !readOnly &&
    !isStreaming &&
    !importing &&
    (input.trim().length > 0 || readyAttachments.length > 0);

  const handleSubmit = useCallback(async () => {
    if (!canSend || !activeSessionId) return;
    const trimmed = input.trim();
    if (wantsSpokenKeep(trimmed)) {
      for (const c of chips) {
        if (c.status !== "ready" || !c.result || c.result.kept || !c.bytesBase64) {
          continue;
        }
        try {
          const kept = await invoke<NonNullable<FileImportResult["kept"]>>("keep_source", {
            request: {
              fileName: c.fileName,
              bytesBase64: c.bytesBase64,
              mdRelPath: c.result.mdRelPath,
            },
          });
          announceKept({ ...c.result, kept }, t);
        } catch (err) {
          const msg = String(err);
          toast.error(msg.includes("太多") ? t("materials.quota") : humanizeImportError(err));
        }
      }
    }
    const body = trimmed + formatAttachmentsBlock(readyAttachments);
    if (!body.trim()) return;
    setInput("");
    setChips([]);
    void sendMessage(body);
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [canSend, activeSessionId, input, chips, readyAttachments, sendMessage, t]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // IME composition (Chinese/Japanese/etc. or English candidate list):
    // Enter selects a candidate — must NOT send the message.
    if (e.nativeEvent.isComposing || e.keyCode === 229) {
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const el = e.target;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 200) + "px";
  };

  const importFiles = async (files: FileList | File[]) => {
    if (!activeSessionId) {
      toast.info(t("chat.attachNeedSession"));
      return;
    }
    const incoming = Array.from(files);
    const list = incoming.filter((f) => {
      if (isLikelyFolderDummy(f)) {
        toast.info(t("materials.folderNeedPicker"));
        return false;
      }
      if (isAcceptedDocumentFile(f)) return true;
      toast.error(`${t("chat.attachUnsupported")}: ${f.name}`);
      return false;
    });
    const skippedKind = incoming.length - list.length;
    if (list.length === 0) return;
    let failed = 0;

    const locals: PendingChip[] = list.map((f) => ({
      localId: `${f.name}-${f.size}-${f.lastModified}-${Math.random().toString(36).slice(2, 7)}`,
      fileName: f.name,
      status: "pending" as const,
    }));
    setChips((prev) => [...prev, ...locals]);

    for (let i = 0; i < list.length; i++) {
      const file = list[i];
      const localId = locals[i].localId;
      try {
        const buf = new Uint8Array(await file.arrayBuffer());
        const bytesBase64 = bytesToBase64(buf);
        const result = await invoke<FileImportResult>("import_document", {
          request: {
            sessionId: activeSessionId,
            fileName: file.name,
            bytesBase64,
            mimeType: file.type || undefined,
            // Keep original office files alongside Markdown conversion.
            deleteOriginal: false,
          },
        });
        if (result.ok) {
          setChips((prev) =>
            prev.map((c) =>
              c.localId === localId
                ? { ...c, status: "ready", result, bytesBase64 }
                : c
            )
          );
          announceKept(result, t);
        } else if (result.kept) {
          setChips((prev) =>
            prev.map((c) =>
              c.localId === localId
                ? { ...c, status: "error", error: result.warning || t("materials.unread") }
                : c
            )
          );
          announceKept(result, t, true);
        } else {
          failed += 1;
          setChips((prev) =>
            prev.map((c) =>
              c.localId === localId
                ? { ...c, status: "error", error: t("materials.errUnsupported") }
                : c
            )
          );
        }
      } catch (err) {
        failed += 1;
        const code = importErrorCode(err);
        const msg =
          code === "too_large"
            ? t("materials.errTooLarge")
            : code === "unsupported_type"
              ? t("materials.errUnsupported")
              : String(err).includes("太多")
                ? t("materials.quota")
                : /encrypt|password/i.test(String(err))
                  ? t("materials.errEncrypted")
                  : humanizeImportError(err);
        toast.error(`${t("chat.attachFailed")}: ${msg}`);
        setChips((prev) =>
          prev.map((c) =>
            c.localId === localId ? { ...c, status: "error", error: msg } : c
          )
        );
      }
    }

    if (fileRef.current) fileRef.current.value = "";
    const skipped = skippedKind + failed;
    if (incoming.length >= 3 || skipped > 0) {
      toast.info(
        t("materials.folderSummary", {
          ok: Math.max(0, list.length - failed),
          skip: skipped,
        }),
      );
    }
  };

  const onPickClick = () => {
    if (!activeSessionId) {
      toast.info(t("chat.attachNeedSession"));
      return;
    }
    if (isStreaming || importing) return;
    fileRef.current?.click();
  };

  const removeChip = (localId: string) => {
    setChips((prev) => prev.filter((c) => c.localId !== localId));
  };

  /** Tauri intercepts OS file drops unless dragDropEnabled=false; HTML5 needs preventDefault. */
  const hasFilePayload = (dt: DataTransfer | null): boolean => {
    if (!dt) return false;
    if (dt.files && dt.files.length > 0) return true;
    if (dt.items && Array.from(dt.items).some((it) => it.kind === "file")) return true;
    const types = Array.from(dt.types ?? []);
    return types.some(
      (ty) =>
        ty === "Files" ||
        ty === "application/x-moz-file" ||
        ty === "public.file-url" ||
        ty === "text/uri-list"
    );
  };

  const onDragEnter = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current += 1;
    if (hasFilePayload(e.dataTransfer)) setDragOver(true);
  };

  const onDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current -= 1;
    if (dragDepth.current <= 0) {
      dragDepth.current = 0;
      setDragOver(false);
    }
  };

  const onDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (hasFilePayload(e.dataTransfer)) {
      e.dataTransfer.dropEffect = "copy";
      setDragOver(true);
    }
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current = 0;
    setDragOver(false);
    if (isStreaming) {
      toast.info(t("toast.streamingBusy"));
      return;
    }
    void filesFromDataTransfer(e.dataTransfer).then((files) => {
      if (files.length) void importFiles(files);
    });
  };

  if (readOnly) {
    return (
      <div className="px-4 pb-4 pt-1">
        <div className="max-w-3xl mx-auto text-center text-sm text-app-fg-secondary dark:text-slate-400 py-3 border-t border-app-border/70 dark:border-slate-800">
          {t("chat.wechatReadOnly")}
        </div>
      </div>
    );
  }

  return (
    <div
      className="px-4 pb-4 pt-1"
      onDragEnter={onDragEnter}
      onDragLeave={onDragLeave}
      onDragOver={onDragOver}
      onDrop={onDrop}
    >
      <div className="max-w-3xl mx-auto">
        <div
          className={`relative rounded-2xl border bg-app-surface dark:bg-slate-900 px-3 py-2.5 shadow-[var(--shadow-app-composer)] transition-shadow ${
            dragOver
              ? "border-app-primary border-dashed ring-2 ring-app-primary/30"
              : "border-app-border dark:border-slate-700 focus-within:ring-2 focus-within:ring-app-primary/25 focus-within:border-app-primary/40"
          }`}
        >
          {dragOver && (
            <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-2xl bg-app-primary/10 dark:bg-blue-950/40">
              <p className="text-sm font-medium text-app-primary dark:text-blue-300">
                {t("chat.dropHint")}
              </p>
            </div>
          )}

          {chips.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-1 pb-2">
              {chips.map((c) => (
                <AttachmentChip
                  key={c.localId}
                  originalName={
                    c.status === "ready" && c.result?.kept
                      ? `${c.fileName} · ${t("materials.keptChip")}`
                      : c.fileName
                  }
                  mdRelPath={c.result?.mdRelPath}
                  chars={c.result?.chars}
                  status={c.status}
                  errorLabel={c.status === "error" ? c.error : undefined}
                  onRemove={() => removeChip(c.localId)}
                  removeLabel={t("chat.attachRemove")}
                />
              ))}
            </div>
          )}

          <div className="flex items-end gap-2">
            <input
              ref={fileRef}
              type="file"
              className="hidden"
              accept={ACCEPT}
              multiple
              onChange={(e) => {
                if (e.target.files) void importFiles(e.target.files);
              }}
            />
            <Button
              variant="ghost"
              size="icon"
              onClick={onPickClick}
              disabled={!activeSessionId || isStreaming || importing}
              aria-label={t("chat.attach")}
              title={t("chat.attach")}
              className="rounded-xl shrink-0"
            >
              {importing ? (
                <Loader2 size={15} className="animate-spin" />
              ) : (
                <Paperclip size={15} />
              )}
            </Button>
            <textarea
              ref={textareaRef}
              value={input}
              onChange={handleInput}
              onKeyDown={handleKeyDown}
              placeholder={
                importing ? t("chat.attachImporting") : t("chat.placeholder")
              }
              rows={1}
              disabled={!activeSessionId || importing}
              className="flex-1 resize-none bg-transparent px-1.5 py-1.5 text-sm text-app-fg dark:text-slate-100 placeholder:text-app-fg-tertiary focus:outline-none disabled:opacity-50 max-h-[200px]"
            />
            {isStreaming ? (
              <Button
                variant="danger"
                size="icon"
                onClick={cancelStream}
                aria-label={t("chat.stop")}
                title={t("chat.stop")}
                className="rounded-xl shrink-0 btn-press"
              >
                <Square size={15} fill="currentColor" />
              </Button>
            ) : (
              <Button
                variant="primary"
                size="icon"
                onClick={handleSubmit}
                disabled={!canSend}
                aria-label={t("chat.send")}
                title={t("chat.send")}
                className={`rounded-xl shrink-0 btn-press ${
                  canSend ? "shadow-md" : ""
                }`}
              >
                <Send
                  size={15}
                  className={
                    canSend
                      ? "transition-transform duration-[var(--motion-fast)]"
                      : undefined
                  }
                />
              </Button>
            )}
          </div>
        </div>
        <p className="text-center text-[10px] text-app-fg-tertiary dark:text-slate-600 mt-2">
          {t("chat.inputHint")}
        </p>
      </div>
    </div>
  );
}
