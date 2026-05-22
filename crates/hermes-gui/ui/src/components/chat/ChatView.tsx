import { useRef, useEffect } from "react";
import { useChatStore } from "../../store/chatStore";
import { MessageBubble } from "./MessageBubble";
import { InputArea } from "./InputArea";
import { StreamingBubble } from "./StreamingBubble";
import { ConfirmModal } from "./ConfirmModal";
import { Sparkles, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";

export function ChatView() {
  const { activeSessionId, messages, isStreaming, streamingText, streamingThinking, activeToolCalls, inputTokens, outputTokens, lastReflection, clearReflection, newSession } =
    useChatStore();
  const t = useUiStore((s) => s.t);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingText]);

  if (!activeSessionId) {
    return (
      <div className="flex flex-col h-full items-center justify-center text-gray-400">
        <p className="text-lg mb-4">{t("chat.empty")}</p>
        <button
          onClick={newSession}
          className="px-4 py-2 rounded-lg bg-blue-600 text-white hover:bg-blue-700 text-sm"
        >
          {t("chat.new")}
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <header className="flex items-center justify-between px-4 py-2 border-b border-gray-200 dark:border-gray-700 text-xs text-gray-500">
        <span>{t("chat.header")}</span>
        <span>
          {inputTokens > 0 &&
            t("chat.usage", {
              input: inputTokens.toLocaleString(),
              output: outputTokens.toLocaleString(),
            })}
        </span>
      </header>

      <div className="flex-1 overflow-y-auto px-4 py-6">
        <div className="max-w-3xl mx-auto space-y-4">
          {messages.map((msg, i) => (
            <MessageBubble key={i} message={msg} />
          ))}
          {isStreaming && (
            <StreamingBubble
              text={streamingText}
              thinking={streamingThinking}
              toolCalls={activeToolCalls}
            />
          )}
          <div ref={bottomRef} />
        </div>
      </div>

      {lastReflection && (
        <div className="mx-4 mb-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-purple-50 dark:bg-purple-900/20 border border-purple-200 dark:border-purple-700 text-sm">
          <Sparkles size={14} className="text-purple-500 shrink-0" />
          <span className="flex-1 text-purple-700 dark:text-purple-300">
            {lastReflection.summary}
            {(lastReflection.memoryCount > 0 || lastReflection.skillCount > 0) && (
              <span className="text-xs text-purple-500 ml-2">
                {t("chat.reflectionCounts", {
                  memory: lastReflection.memoryCount,
                  skill: lastReflection.skillCount,
                })}
              </span>
            )}
          </span>
          <button onClick={clearReflection} className="p-0.5 hover:bg-purple-100 dark:hover:bg-purple-800 rounded">
            <X size={12} className="text-purple-400" />
          </button>
        </div>
      )}

      <InputArea />
      <ConfirmModal />
    </div>
  );
}
