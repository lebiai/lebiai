import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  FileText,
  Loader2,
  Pencil,
  RefreshCw,
} from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type { DisplayMessage, MessageData } from "../../types";
import { MarkdownContent } from "../common/MarkdownContent";
import { splitOnHand, splitUserTextAndAttachments } from "../../utils/attachments";
import {
  assistantPlainText,
  contextSummaryBody,
  formatDurationMs,
  isContextSummary,
  userPlainText,
} from "../../utils/displayMessages";
import {
  artifactLabel,
  artifactPathsOf,
  objectFromToolSummary,
  processHeadline,
  processKindForTool,
  summaryFromToolInput,
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
  /**
   * 组会话里这一轮开口的人。**只有组会话标**——工位的头部已经写着是谁，
   * 每条消息再标一次名字纯属噪音（规格 §2.2：组里必须看得出谁在说）。
   */
  speaker?: string | null;
}

export function MessageBubble({
  message,
  streaming,
  canRegenerate,
  onRegenerate,
  onEditUser,
  isStreaming,
  speaker,
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
        speaker={speaker}
      />
    );
  }

  if (isUser) {
    // 压缩摘要落盘时也是一条 user 消息，但它不是用户打的字 —— 渲染成说明卡，
    // 免得历史里冒出一条「用户自己从没说过的话」。
    if (isContextSummary(message)) {
      return <ContextSummaryNotice message={message} />;
    }
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
        // 落盘只留入参：把「干了哪件事」从 `input` 还原出来（写了哪个文件等）。
        summary: summaryFromToolInput(tool.name, tool.input),
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
      speaker={speaker}
    />
  );
}

/**
 * 「更早的对话已整理为摘要」。默认收起：用户需要的是知道它整理过，
 * 而不是再读一遍摘要；想看细节点一下就行。
 */
function ContextSummaryNotice({ message }: { message: DisplayMessage | MessageData }) {
  const t = useUiStore((s) => s.t);
  return (
    <div className="flex justify-center">
      <details className="group max-w-[min(620px,100%)] rounded-xl border border-app-border/70 dark:border-zinc-800 bg-app-surface/50 dark:bg-zinc-900/30 px-3.5 py-2">
        <summary className="cursor-pointer select-none list-none text-app-sub leading-5 text-app-fg-secondary dark:text-zinc-500 marker:content-none">
          <span className="group-open:hidden">{t("chat.contextSummaryClosed")}</span>
          <span className="hidden group-open:inline">{t("chat.contextSummaryOpen")}</span>
        </summary>
        <div className="mt-1.5 whitespace-pre-wrap text-xs leading-relaxed text-app-fg-secondary dark:text-zinc-400">
          {contextSummaryBody(message)}
        </div>
      </details>
    </div>
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
            <p className="text-app-sub text-app-fg-secondary mb-1">
              {t("materials.onHand")}
            </p>
            {onHand.titles.length === 0 ? (
              <p className="text-xs text-app-fg-secondary">{t("materials.onHandEmpty")}</p>
            ) : (
              <ul className="space-y-0.5">
                {onHand.titles.map((title) => (
                  <li key={title} className="text-app-body text-app-fg">
                    《{title}》
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : null}
        {body.trim() ? (
          <div className="px-4 py-2.5 rounded-2xl rounded-br-md bg-app-user-bubble text-white text-app-body shadow-sm leading-relaxed whitespace-pre-wrap">
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
  speaker,
}: {
  thinking: string;
  tools: ToolCallView[];
  text: string;
  streaming: boolean;
  durationMs?: number;
  canRegenerate: boolean;
  onRegenerate?: () => void;
  speaker?: string | null;
}) {
  const t = useUiStore((s) => s.t);
  const hasProcess = !!thinking.trim() || tools.length > 0;
  const empty = !text && !hasProcess && streaming;

  return (
    <div className="flex justify-start group/msg">
      <div className="w-full max-w-3xl min-w-0 space-y-2">
        {speaker && (
          <div className="text-app-sub font-medium text-app-fg-secondary dark:text-slate-400">
            {speaker}
          </div>
        )}

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
  // Collapsed by default, streaming or not: the headline already carries the
  // live state (verb + spinner, or a failure icon). Detail is on demand — a
  // long task used to unfold every single tool call as it ran.
  const [expanded, setExpanded] = useState(false);
  const running = streaming && tools.some((tc) => tc.result === undefined);
  const runningTool = tools.find((tc) => tc.result === undefined);
  const runningObject = runningTool
    ? objectFromToolSummary(runningTool.summary, runningTool.name)
    : undefined;
  const errorCount = tools.filter((tc) => tc.isError).length;
  // 这一组落了哪些文件。**折叠行也带出来**，而且是能点开的：用户原话是「看不出写了
  // 什么」，光有文件名还得再去材料面板翻一遍。最多摊三枚，多的写 +N。
  const artifacts = artifactPathsOf(tools);
  const summary = processHeadline(
    tools.map((tc) => tc.name),
    thinking,
    streaming,
    running,
    (key, params) => t(key as Parameters<typeof t>[0], params),
    runningObject
  );

  useEffect(() => {
    if (running) setExpanded(true);
  }, [running]);

  return (
    <div className="rounded-lg border border-app-border/80 dark:border-slate-700/60 bg-app-muted/30 dark:bg-slate-800/25 overflow-hidden transition-[border-color,background-color] duration-[var(--motion-fast)]">
      {/* 折叠开关与产出标签是**两个并排的按钮**：小标签本身要能点（打开产出），
          塞进那个开关里就成了按钮套按钮。 */}
      <div className="flex items-center">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="flex-1 min-w-0 flex items-center gap-2 px-2.5 py-1.5 text-xs text-left hover:bg-app-muted/60 dark:hover:bg-slate-800/50 transition-colors duration-[var(--motion-fast)]"
        aria-expanded={expanded}
      >
        {running ? (
          <Loader2 size={13} className="animate-spin text-app-primary shrink-0 motion-safe-only" />
        ) : streaming && thinking ? (
          <Brain size={13} className="text-app-accent shrink-0" />
        ) : errorCount > 0 ? null : (
          <CheckCircle2 size={13} className="text-app-success shrink-0" />
        )}
        <span className="font-medium text-app-fg-secondary dark:text-slate-300 truncate">
          {summary}
        </span>
        {!running && errorCount > 0 && (
          <span className="shrink-0 text-app-sub text-app-fg-secondary">
            {t("message.toolStepsFailed", { n: errorCount })}
          </span>
        )}
        <span
          className={`ml-auto text-app-fg-tertiary shrink-0 transition-transform duration-[var(--motion-fast)] ${
            expanded ? "rotate-0" : ""
          }`}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </span>
      </button>
      {artifacts.length > 0 && (
        <div className="shrink-0 flex items-center gap-1 pl-1 pr-2">
          {artifacts.slice(0, 3).map((path) => (
            <ArtifactChip key={path} path={path} />
          ))}
          {artifacts.length > 3 && (
            <span className="text-app-sub text-app-fg-tertiary">
              {t("message.artifactMore", { n: artifacts.length - 3 })}
            </span>
          )}
        </div>
      )}
      </div>
      <div className="fold-panel" data-open={expanded ? "true" : "false"}>
        <div className="fold-panel-inner">
          <div className="border-t border-app-border/70 dark:border-slate-700/50 px-2.5 py-2 space-y-2">
            {tools.map((tc) => (
              <ToolRow key={tc.id} tc={tc} streaming={streaming} />
            ))}
            {thinking.trim() && (
              <div>
                <div className="text-app-sub text-app-fg-secondary mb-1 flex items-center gap-1">
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

/**
 * 产出小标签：点开就是这份文件（复用「我的材料」那条 `open_output`，越界/软链都在
 * 引擎侧挡过了）。浏览器里预览（非 Tauri 壳）时打不开，会老实地报一句。
 */
function ArtifactChip({ path }: { path: string }) {
  const t = useUiStore((s) => s.t);
  const [busy, setBusy] = useState(false);
  return (
    <button
      type="button"
      title={path}
      aria-label={t("message.artifactOpen", { name: path })}
      disabled={busy}
      onClick={() => {
        setBusy(true);
        void invoke("open_output", { path })
          .catch((e) => toast.error(t("message.artifactFailed", { error: String(e) })))
          .finally(() => setBusy(false));
      }}
      className="inline-flex items-center gap-1 max-w-[16rem] px-2 py-0.5 rounded-full border border-app-border dark:border-blue-900/60 bg-app-primary-soft dark:bg-blue-950/40 text-app-primary dark:text-blue-300 text-xs hover:border-app-primary dark:hover:border-blue-500 transition-colors duration-[var(--motion-fast)]"
    >
      {busy ? (
        <Loader2 size={11} className="animate-spin shrink-0 motion-safe-only" />
      ) : (
        <FileText size={11} className="shrink-0" />
      )}
      <span className="truncate">{artifactLabel(path)}</span>
    </button>
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
        ) : tc.isError ? null : (
          <CheckCircle2 size={12} className="text-app-success shrink-0" />
        )}
        <span className="font-medium text-app-fg dark:text-slate-200 truncate">
          {toolRowLabel(tc, t, streaming)}
        </span>
        <span
          className={`text-app-sub transition-colors duration-[var(--motion-fast)] ${
            tc.isError
              ? "text-app-fg-tertiary"
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
        <span className="text-xs tabular-nums px-1.5">
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
