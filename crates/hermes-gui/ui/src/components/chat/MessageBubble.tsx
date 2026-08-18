import { useMemo, useState, type ReactNode } from "react";
import {
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  Loader2,
  Pencil,
  RefreshCw,
  XCircle,
} from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { DisplayMessage, MessageData } from "../../types";
import { MarkdownContent } from "../common/MarkdownContent";
import { splitOnHand, splitUserTextAndAttachments } from "../../utils/attachments";
import {
  assistantPlainText,
  formatDurationMs,
  userPlainText,
} from "../../utils/displayMessages";
import {
  objectFromToolSummary,
  processHeadline,
  processKindForTool,
} from "../../utils/processLabel";
import { toast } from "../../utils/toast";
import { useZaibanStore } from "../../store/zaibanStore";
import { ui } from "../common/ui";
import { AttachmentCardList } from "./AttachmentCards";

export interface ToolCallView {
  id: string;
  name: string;
  /** One-line summary of what the tool is doing (toolExecStart). */
  summary?: string;
  result?: string;
  isError?: boolean;
}

interface Props {
  message: DisplayMessage | MessageData;
  /** When set, this is the live streaming assistant turn (no bubble shell). */
  streaming?: {
    text: string;
    thinking: string;
    toolCalls: ToolCallView[];
  };
  /** Last assistant in the list — show regenerate. */
  canRegenerate?: boolean;
  onRegenerate?: () => void;
  onEditUser?: (rawStart: number, currentText: string) => void;
  isStreaming?: boolean;
}

export function MessageBubble({
  message,
  streaming,
  canRegenerate,
  onRegenerate,
  onEditUser,
  isStreaming,
}: Props) {
  const isUser = message.role === "user";

  if (streaming) {
    return (
      <AssistantCanvas
        thinking={streaming.thinking}
        tools={streaming.toolCalls}
        text={streaming.text}
        streaming
        durationMs={undefined}
        canRegenerate={false}
      />
    );
  }

  if (isUser) {
    return (
      <UserTurn
        message={message}
        onEdit={
          onEditUser && "rawStart" in message
            ? () => onEditUser(message.rawStart, userPlainText(message))
            : undefined
        }
        disabled={!!isStreaming}
      />
    );
  }

  const thinkingContent = message.content
    .filter((b) => b.type === "thinking")
    .map((b) => (b.type === "thinking" ? b.thinking : ""))
    .join("\n");

  const toolUses = message.content.filter((b) => b.type === "toolUse");
  const toolResults = message.content.filter((b) => b.type === "toolResult");
  const tools: ToolCallView[] = toolUses
    .filter((b): b is Extract<typeof b, { type: "toolUse" }> => b.type === "toolUse")
    .map((tool) => {
      const result = toolResults.find(
        (r) => r.type === "toolResult" && r.toolUseId === tool.id
      );
      return {
        id: tool.id,
        name: tool.name,
        result: result?.type === "toolResult" ? result.content : undefined,
        isError: result?.type === "toolResult" ? result.isError : false,
      };
    });

  const textContent = assistantPlainText(message);

  return (
    <AssistantCanvas
      thinking={thinkingContent}
      tools={tools}
      text={textContent}
      streaming={false}
      durationMs={message.durationMs}
      canRegenerate={!!canRegenerate && !isStreaming}
      onRegenerate={onRegenerate}
    />
  );
}

function UserTurn({
  message,
  onEdit,
  disabled,
}: {
  message: MessageData;
  onEdit?: () => void;
  disabled?: boolean;
}) {
  const t = useUiStore((s) => s.t);
  const textContent = userPlainText(message);
  const userParsed = useMemo(
    () => splitUserTextAndAttachments(textContent),
    [textContent]
  );
  const onHand = useMemo(
    () => splitOnHand(userParsed.body),
    [userParsed.body]
  );
  const body = onHand.body;
  const attachments = userParsed.attachments;
  if (!body.trim() && attachments.length === 0 && onHand.titles.length === 0) {
    return null;
  }

  const copyBody = async () => {
    try {
      await navigator.clipboard.writeText(body.trim() || textContent);
      toast.success(t("toast.copied"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex justify-end group/msg">
      <div className="max-w-[min(85%,42rem)] flex flex-col items-end gap-1.5 min-w-0">
        {attachments.length > 0 && (
          <AttachmentCardList items={attachments} variant="message" />
        )}
        {onHand.titles.length > 0 || textContent.includes("[on-hand]") ? (
          <div className={`w-full max-w-sm ${ui.card} px-3 py-2 text-left`}>
            <p className="text-[11px] text-app-fg-tertiary mb-1">
              {t("materials.onHand")}
            </p>
            {onHand.titles.length === 0 ? (
              <p className="text-xs text-app-fg-secondary">{t("materials.onHandEmpty")}</p>
            ) : (
              <ul className="space-y-0.5">
                {onHand.titles.map((title) => (
                  <li key={title} className="text-sm text-app-fg">
                    《{title}》
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : null}
        {body.trim() ? (
          <div className="px-4 py-2.5 rounded-2xl rounded-br-md bg-app-user-bubble text-white text-sm shadow-sm leading-relaxed whitespace-pre-wrap">
            {body}
          </div>
        ) : null}
        <div className="flex items-center gap-0.5 opacity-0 group-hover/msg:opacity-100 focus-within:opacity-100 transition-opacity">
          <IconBtn label={t("common.copy")} onClick={() => void copyBody()}>
            <Copy size={14} />
          </IconBtn>
          {onEdit && (
            <IconBtn
              label={t("message.edit")}
              onClick={onEdit}
              disabled={disabled}
            >
              <Pencil size={14} />
            </IconBtn>
          )}
        </div>
      </div>
    </div>
  );
}

function AssistantCanvas({
  thinking,
  tools,
  text,
  streaming,
  durationMs,
  canRegenerate,
  onRegenerate,
}: {
  thinking: string;
  tools: ToolCallView[];
  text: string;
  streaming: boolean;
  durationMs?: number;
  canRegenerate: boolean;
  onRegenerate?: () => void;
}) {
  const t = useUiStore((s) => s.t);
  const hasProcess = !!thinking.trim() || tools.length > 0;
  const empty = !text && !hasProcess && streaming;

  return (
    <div className="flex justify-start group/msg">
      <div className="w-full max-w-3xl min-w-0 space-y-2">
        {hasProcess && (
          <ProcessGroup
            thinking={thinking}
            tools={tools}
            streaming={streaming}
          />
        )}

        {hasProcess && !!text.trim() && (
          <div
            className="h-px bg-app-border/80 dark:bg-slate-700/70"
            role="separator"
            aria-hidden
          />
        )}

        {text ? (
          <div className="min-w-0 text-app-fg dark:text-slate-100 transition-opacity duration-[var(--motion-fast)]">
            <MarkdownContent content={text} />
            {streaming && (
              <span className="inline-block w-1.5 h-4 bg-app-primary/70 animate-pulse ml-0.5 align-middle rounded-sm motion-safe-only" />
            )}
          </div>
        ) : null}

        {empty && (
          <div className="flex items-center gap-2 text-sm text-app-fg-secondary dark:text-slate-400 py-1">
            <Loader2
              size={14}
              className="animate-spin text-app-primary motion-safe-only"
            />
            <span>{t("message.responding")}</span>
          </div>
        )}

        {/* Footer is for the *answer* (and turn meta) — not for process-only shells.
            Copy only when there is assistant text; otherwise thinking/tools sat above
            a stray copy icon with nothing to copy. */}
        {!streaming &&
          (() => {
            const hasText = text.trim().length > 0;
            const showFooter = hasText || canRegenerate;
            if (!showFooter) return null;
            return (
              <MessageFooter
                copyText={text}
                durationMs={durationMs}
                canRegenerate={canRegenerate}
                onRegenerate={onRegenerate}
              />
            );
          })()}
      </div>
    </div>
  );
}

function ProcessGroup({
  thinking,
  tools,
  streaming,
}: {
  thinking: string;
  tools: ToolCallView[];
  streaming: boolean;
}) {
  const t = useUiStore((s) => s.t);
  // Streaming: expanded so user sees progress; finished: collapsed by default.
  const [expanded, setExpanded] = useState(streaming);
  const running = streaming && tools.some((tc) => tc.result === undefined);
  const anyError = tools.some((tc) => tc.isError);
  const summary = processHeadline(
    tools.map((tc) => tc.name),
    thinking,
    streaming,
    running,
    (key, params) => t(key as Parameters<typeof t>[0], params)
  );

  return (
    <div className="rounded-lg border border-app-border/80 dark:border-slate-700/60 bg-app-muted/30 dark:bg-slate-800/25 overflow-hidden transition-[border-color,background-color] duration-[var(--motion-fast)]">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 text-xs text-left hover:bg-app-muted/60 dark:hover:bg-slate-800/50 transition-colors duration-[var(--motion-fast)]"
        aria-expanded={expanded}
      >
        {running ? (
          <Loader2 size={13} className="animate-spin text-app-primary shrink-0 motion-safe-only" />
        ) : anyError ? (
          <XCircle size={13} className="text-app-danger shrink-0" />
        ) : streaming && thinking ? (
          <Brain size={13} className="text-app-accent shrink-0" />
        ) : (
          <CheckCircle2 size={13} className="text-app-success shrink-0" />
        )}
        <span className="font-medium text-app-fg-secondary dark:text-slate-300 truncate">
          {summary}
        </span>
        <span
          className={`ml-auto text-app-fg-tertiary shrink-0 transition-transform duration-[var(--motion-fast)] ${
            expanded ? "rotate-0" : ""
          }`}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </span>
      </button>
      <div className="fold-panel" data-open={expanded ? "true" : "false"}>
        <div className="fold-panel-inner">
          <div className="border-t border-app-border/70 dark:border-slate-700/50 px-2.5 py-2 space-y-2">
            {tools.map((tc) => (
              <ToolRow key={tc.id} tc={tc} streaming={streaming} />
            ))}
            {thinking.trim() && (
              <div>
                <div className="text-[11px] text-app-fg-tertiary mb-1 flex items-center gap-1">
                  <Brain size={11} />
                  {t("process.thought")}
                </div>
                <p className="text-xs whitespace-pre-wrap text-app-fg-secondary dark:text-slate-400 max-h-36 overflow-y-auto leading-relaxed">
                  {streaming && thinking.length > 800
                    ? "…" + thinking.slice(-800)
                    : thinking}
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function ToolRow({ tc, streaming }: { tc: ToolCallView; streaming: boolean }) {
  const t = useUiStore((s) => s.t);
  const [open, setOpen] = useState(false);
  const isRunning = streaming && tc.result === undefined;

  return (
    <div className="rounded-md border border-app-border/60 dark:border-slate-700/50 overflow-hidden transition-[border-color] duration-[var(--motion-fast)]">
      <button
        type="button"
        onClick={() => {
          if (tc.name.startsWith("commitment_") && tc.result && !tc.isError) {
            const m = tc.result.match(/\[(cmt_[^\]]+)\]/);
            useZaibanStore.getState().setHighlight(m?.[1] ?? null);
          }
          if (tc.result !== undefined) setOpen((o) => !o);
        }}
        className="w-full flex items-center gap-2 px-2 py-1.5 text-xs hover:bg-app-muted/50 dark:hover:bg-slate-800/40 transition-colors duration-[var(--motion-fast)]"
        disabled={tc.result === undefined}
        aria-expanded={open}
      >
        {isRunning ? (
          <Loader2 size={12} className="animate-spin text-app-primary shrink-0 motion-safe-only" />
        ) : tc.isError ? (
          <XCircle size={12} className="text-app-danger shrink-0" />
        ) : (
          <CheckCircle2 size={12} className="text-app-success shrink-0" />
        )}
        <span className="font-medium text-app-fg dark:text-slate-200 truncate">
          {toolRowLabel(tc, t, streaming)}
        </span>
        <span
          className={`text-[11px] transition-colors duration-[var(--motion-fast)] ${
            tc.isError
              ? "text-red-500"
              : isRunning
                ? "text-app-fg-tertiary"
                : "text-emerald-600 dark:text-emerald-400"
          }`}
        >
          {isRunning
            ? t("message.toolRunning")
            : tc.isError
              ? t("message.toolFailed")
              : t("message.toolDone")}
        </span>
        {tc.result !== undefined &&
          (open ? (
            <ChevronDown size={11} className="ml-auto text-app-fg-tertiary" />
          ) : (
            <ChevronRight size={11} className="ml-auto text-app-fg-tertiary" />
          ))}
      </button>
      <div
        className="fold-panel"
        data-open={open && tc.result !== undefined ? "true" : "false"}
      >
        <div className="fold-panel-inner">
          {tc.result !== undefined && (
            <pre className="border-t border-app-border/60 dark:border-slate-700/50 px-2 py-1.5 text-xs whitespace-pre-wrap font-mono text-app-fg-secondary dark:text-slate-400 max-h-40 overflow-y-auto bg-app-surface/50 dark:bg-slate-900/40">
              {tc.result.length > 2000
                ? tc.result.slice(0, 2000) + "\n..."
                : tc.result}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

function toolRowLabel(
  tc: ToolCallView,
  t: ReturnType<typeof useUiStore.getState>["t"],
  streaming: boolean
): string {
  if (tc.name === "commitment_save" && tc.result && !tc.isError) {
    const titled = tc.result.split("]: ")[1]?.trim();
    if (titled) return t("zaiban.noted", { title: titled });
  }
  const kind = processKindForTool(tc.name);
  const done = !streaming || tc.result !== undefined;
  const verbKey = (
    done ? `process.${kind}` : `process.${kind}Doing`
  ) as Parameters<typeof t>[0];
  const verb = t(verbKey);
  const object = objectFromToolSummary(tc.summary, tc.name);
  return object ? `${verb}：${object}` : verb;
}

function MessageFooter({
  copyText,
  durationMs,
  canRegenerate,
  onRegenerate,
}: {
  copyText: string;
  durationMs?: number;
  canRegenerate: boolean;
  onRegenerate?: () => void;
}) {
  const t = useUiStore((s) => s.t);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (!copyText.trim()) return;
    try {
      await navigator.clipboard.writeText(copyText);
      setCopied(true);
      toast.success(t("toast.copied"));
      window.setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const canCopy = copyText.trim().length > 0;

  return (
    <div className="flex flex-wrap items-center gap-1 pt-0.5 text-app-fg-tertiary dark:text-slate-500">
      {canCopy && (
        <IconBtn label={t("common.copy")} onClick={() => void copy()}>
          {copied ? <Check size={14} className="text-app-success" /> : <Copy size={14} />}
        </IconBtn>
      )}
      {canRegenerate && onRegenerate && (
        <IconBtn label={t("message.regenerate")} onClick={onRegenerate}>
          <RefreshCw size={14} />
        </IconBtn>
      )}
      {durationMs !== undefined && durationMs > 0 && (
        <span className="text-[11px] tabular-nums px-1.5">
          {t("message.duration", { time: formatDurationMs(durationMs) })}
        </span>
      )}
    </div>
  );
}

function IconBtn({
  children,
  label,
  onClick,
  disabled,
}: {
  children: ReactNode;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className="p-1.5 rounded-md hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-tertiary hover:text-app-fg dark:hover:text-slate-200 disabled:opacity-40 disabled:pointer-events-none transition-colors"
    >
      {children}
    </button>
  );
}
