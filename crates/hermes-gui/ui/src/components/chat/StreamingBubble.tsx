import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Loader2, CheckCircle2, XCircle, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";

interface ToolCall {
  id: string;
  name: string;
  result?: string;
  isError?: boolean;
}

interface Props {
  text: string;
  thinking: string;
  toolCalls: ToolCall[];
}

function ToolCallCard({ tc }: { tc: ToolCall }) {
  const [expanded, setExpanded] = useState(false);
  const isRunning = tc.result === undefined;

  return (
    <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <button
        onClick={() => !isRunning && setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
      >
        {isRunning ? (
          <Loader2 size={14} className="text-blue-500 animate-spin shrink-0" />
        ) : tc.isError ? (
          <XCircle size={14} className="text-red-500 shrink-0" />
        ) : (
          <CheckCircle2 size={14} className="text-green-500 shrink-0" />
        )}
        <span className="font-medium font-mono">{tc.name}</span>
        {isRunning && (
          <span className="text-gray-400 ml-auto">running...</span>
        )}
        {!isRunning && tc.result && (
          expanded
            ? <ChevronDown size={12} className="ml-auto text-gray-400" />
            : <ChevronRight size={12} className="ml-auto text-gray-400" />
        )}
      </button>
      {expanded && tc.result && (
        <div className="border-t border-gray-200 dark:border-gray-700 px-3 py-2 bg-gray-50 dark:bg-gray-800/50">
          <pre className="text-xs whitespace-pre-wrap font-mono text-gray-600 dark:text-gray-400 max-h-48 overflow-y-auto">
            {tc.result.length > 2000 ? tc.result.slice(0, 2000) + "\n..." : tc.result}
          </pre>
        </div>
      )}
    </div>
  );
}

export function StreamingBubble({ text, thinking, toolCalls }: Props) {
  return (
    <div className="flex justify-start">
      <div className="max-w-[80%] space-y-2">
        {thinking && (
          <div className="text-xs text-gray-400 border border-gray-200 dark:border-gray-700 rounded-lg p-2">
            <div className="flex items-center gap-1.5 mb-1">
              <Loader2 size={12} className="animate-spin" />
              <span className="font-medium">Thinking</span>
            </div>
            <pre className="whitespace-pre-wrap font-mono text-gray-500 max-h-32 overflow-y-auto">
              {thinking.length > 500 ? "..." + thinking.slice(-500) : thinking}
            </pre>
          </div>
        )}

        {toolCalls.map((tc) => (
          <ToolCallCard key={tc.id} tc={tc} />
        ))}

        {text && (
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
            <span className="inline-block w-1.5 h-4 bg-gray-400 animate-pulse ml-0.5" />
          </div>
        )}

        {!text && !thinking && toolCalls.length === 0 && (
          <div className="flex items-center gap-1 text-gray-400 text-sm">
            <span className="inline-block w-1.5 h-4 bg-gray-400 animate-pulse" />
          </div>
        )}
      </div>
    </div>
  );
}
