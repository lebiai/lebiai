import { useState } from "react";
import { useUiStore } from "../../store/uiStore";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { CheckCircle2, XCircle, ChevronDown, ChevronRight, Brain } from "lucide-react";
import type { MessageData } from "../../types";

interface Props {
  message: MessageData;
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === "user";
  const t = useUiStore((s) => s.t);

  const textContent = message.content
    .filter((b) => b.type === "text")
    .map((b) => (b.type === "text" ? b.text : ""))
    .join("\n");

  const thinkingContent = message.content
    .filter((b) => b.type === "thinking")
    .map((b) => (b.type === "thinking" ? b.thinking : ""))
    .join("\n");

  const toolUses = message.content.filter((b) => b.type === "toolUse");
  const toolResults = message.content.filter((b) => b.type === "toolResult");

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] px-4 py-2 rounded-2xl bg-blue-600 text-white text-sm whitespace-pre-wrap">
          {textContent}
        </div>
      </div>
    );
  }

  return (
    <div className="flex justify-start">
      <div className="max-w-[80%] space-y-2">
        {thinkingContent && <ThinkingBlock content={thinkingContent} label={t("message.thinking")} />}

        {toolUses.map((tool) => {
          if (tool.type !== "toolUse") return null;
          const result = toolResults.find(
            (r) => r.type === "toolResult" && r.toolUseId === tool.id
          );
          return (
            <ToolCallBlock
              key={tool.id}
              name={tool.name}
              result={result?.type === "toolResult" ? result.content : undefined}
              isError={result?.type === "toolResult" ? result.isError : false}
              doneLabel={t("message.toolDone")}
              failedLabel={t("message.toolFailed")}
            />
          );
        })}

        {textContent && (
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{textContent}</ReactMarkdown>
          </div>
        )}
      </div>
    </div>
  );
}

function ThinkingBlock({ content, label }: { content: string; label: string }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
      >
        <Brain size={14} className="text-purple-400 shrink-0" />
        <span className="font-medium text-gray-500">{label}</span>
        {expanded
          ? <ChevronDown size={12} className="ml-auto text-gray-400" />
          : <ChevronRight size={12} className="ml-auto text-gray-400" />
        }
      </button>
      {expanded && (
        <div className="border-t border-gray-200 dark:border-gray-700 px-3 py-2 bg-gray-50 dark:bg-gray-800/50">
          <pre className="text-xs whitespace-pre-wrap font-mono text-gray-500 max-h-60 overflow-y-auto">
            {content}
          </pre>
        </div>
      )}
    </div>
  );
}

function ToolCallBlock({ name, result, isError, doneLabel, failedLabel }: { name: string; result?: string; isError: boolean; doneLabel: string; failedLabel: string }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <button
        onClick={() => result && setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
      >
        {isError ? (
          <XCircle size={14} className="text-red-500 shrink-0" />
        ) : (
          <CheckCircle2 size={14} className="text-green-500 shrink-0" />
        )}
        <span className="font-medium font-mono">{name}</span>
        <span className={`text-xs ${isError ? "text-red-400" : "text-green-400"}`}>
          {isError ? failedLabel : doneLabel}
        </span>
        {result && (
          expanded
            ? <ChevronDown size={12} className="ml-auto text-gray-400" />
            : <ChevronRight size={12} className="ml-auto text-gray-400" />
        )}
      </button>
      {expanded && result && (
        <div className="border-t border-gray-200 dark:border-gray-700 px-3 py-2 bg-gray-50 dark:bg-gray-800/50">
          <pre className="text-xs whitespace-pre-wrap font-mono text-gray-600 dark:text-gray-400 max-h-48 overflow-y-auto">
            {result.length > 2000 ? result.slice(0, 2000) + "\n..." : result}
          </pre>
        </div>
      )}
    </div>
  );
}
