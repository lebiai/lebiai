import type { ContentBlock, DisplayMessage, MessageData } from "../types";

/**
 * Coalesce protocol transcript into chat-UI turns:
 * - User messages that are only tool_result (no text/image) are not shown alone
 * - Their tool_results are merged into the previous assistant message so tool
 *   cards can show done/failed + expandable output
 * - Tracks rawStart/rawEnd for truncate / edit / regenerate
 */
export function coalesceMessagesForDisplay(messages: MessageData[]): DisplayMessage[] {
  const out: DisplayMessage[] = [];

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.role === "user" && isToolResultOnly(msg)) {
      const results = msg.content.filter((b) => b.type === "toolResult");
      const prev = out[out.length - 1];
      if (prev && prev.role === "assistant" && results.length > 0) {
        out[out.length - 1] = {
          ...prev,
          content: [...prev.content, ...results],
          rawEnd: i + 1,
        };
      }
      continue;
    }

    if (msg.role === "user" && !hasVisibleUserContent(msg)) {
      continue;
    }

    out.push({
      ...msg,
      rawStart: i,
      rawEnd: i + 1,
    });
  }

  return out;
}

function isToolResultOnly(msg: MessageData): boolean {
  if (msg.role !== "user") return false;
  let hasResult = false;
  for (const b of msg.content) {
    if (b.type === "text" && b.text.trim()) return false;
    if (b.type === "toolUse") return false;
    if (b.type === "toolResult") hasResult = true;
  }
  return hasResult;
}

function hasVisibleUserContent(msg: MessageData): boolean {
  return msg.content.some((b) => {
    if (b.type === "text") return b.text.trim().length > 0;
    if (b.type === "toolUse") return true;
    return false;
  });
}

/** Whether an assistant message has anything to show (text / tools / thinking). */
export function hasVisibleAssistantContent(msg: MessageData): boolean {
  return msg.content.some((b: ContentBlock) => {
    if (b.type === "text") return b.text.trim().length > 0;
    if (b.type === "thinking") return b.thinking.trim().length > 0;
    if (b.type === "toolUse") return true;
    if (b.type === "toolResult") return true;
    return false;
  });
}

export function assistantPlainText(msg: MessageData): string {
  return msg.content
    .filter((b) => b.type === "text")
    .map((b) => (b.type === "text" ? b.text : ""))
    .join("\n")
    .trim();
}

export function userPlainText(msg: MessageData): string {
  return msg.content
    .filter((b) => b.type === "text")
    .map((b) => (b.type === "text" ? b.text : ""))
    .join("\n");
}

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  const m = Math.floor(s / 60);
  const rem = Math.round(s % 60);
  return `${m}m ${rem}s`;
}
