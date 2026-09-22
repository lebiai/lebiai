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
    if (msg.role === "user" && isInternalInstructionOnly(msg)) {
      continue;
    }
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

  return mergeAssistantWorkSpans(out);
}

/** Engine nudges that were wrongly saved as user text — never show them. */
export function isInternalInstructionText(text: string): boolean {
  const t = text.trim();
  return (
    t.startsWith("[lebi-AI Care]") ||
    t.startsWith("[lebi-AI Materials]") ||
    t.startsWith("[Hermes Care]") ||
    t.startsWith("[Context:") ||
    t.startsWith("You've reached the tool-call budget")
  );
}

function isInternalInstructionOnly(msg: MessageData): boolean {
  if (msg.role !== "user") return false;
  let saw = false;
  for (const b of msg.content) {
    if (b.type === "text") {
      if (!b.text.trim()) continue;
      if (isInternalInstructionText(b.text)) {
        saw = true;
        continue;
      }
      return false;
    }
    if (b.type === "toolResult" || b.type === "toolUse") return false;
  }
  return saw;
}

/**
 * One user ask → one process fold + one answer.
 * Mid-loop assistant chatter (retry narration) is dropped; tools stay.
 */
function mergeAssistantWorkSpans(rows: DisplayMessage[]): DisplayMessage[] {
  const merged: DisplayMessage[] = [];
  for (const row of rows) {
    const prev = merged[merged.length - 1];
    // 同一个说话人、相邻两条 assistant = 同一轮里的分段（工具调用 + 收尾正文），
    // 并成一块。**换了人就不并** —— 并了就等于把后一个人的话吞进前一个的气泡。
    if (
      row.role === "assistant" &&
      prev &&
      prev.role === "assistant" &&
      prev.speaker === row.speaker
    ) {
      merged[merged.length - 1] = {
        ...prev,
        content: mergeAssistantContent(prev.content, row.content),
        rawEnd: row.rawEnd,
        durationMs: addOptional(prev.durationMs, row.durationMs),
        inputTokens: addOptional(prev.inputTokens, row.inputTokens),
        outputTokens: addOptional(prev.outputTokens, row.outputTokens),
      };
      continue;
    }
    merged.push(row);
  }
  return merged;
}

function mergeAssistantContent(
  earlier: ContentBlock[],
  later: ContentBlock[]
): ContentBlock[] {
  const tools: ContentBlock[] = [];
  const thinking: string[] = [];
  let lastText = "";
  for (const b of [...earlier, ...later]) {
    if (b.type === "toolUse" || b.type === "toolResult") {
      tools.push(b);
    } else if (b.type === "thinking" && b.thinking.trim()) {
      thinking.push(b.thinking);
    } else if (b.type === "text" && b.text.trim()) {
      lastText = b.text;
    }
  }
  const out: ContentBlock[] = [...tools];
  if (thinking.length > 0) {
    out.push({ type: "thinking", thinking: thinking.join("\n") });
  }
  if (lastText) {
    out.push({ type: "text", text: lastText });
  }
  return out;
}

function addOptional(a?: number, b?: number): number | undefined {
  if (a == null && b == null) return undefined;
  return (a ?? 0) + (b ?? 0);
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

/**
 * 上下文压缩摘要的固定开头 —— 必须与 `hermes-core::compaction::SUMMARY_PREFIX` 一致。
 * 由**引擎**写入（不是模型），所以可以当作稳定契约来认。
 */
export const CONTEXT_SUMMARY_PREFIX = "[Context Summary]";

/**
 * 这条 user 消息其实不是用户说的话，而是「更早对话被压缩成的摘要」。
 * 历史里它必须渲染成说明卡 —— 否则界面上会出现一条用户从没打过、
 * 还长得像自己说的话的气泡。
 */
export function isContextSummary(msg: MessageData): boolean {
  if (msg.role !== "user") return false;
  return userPlainText(msg).trimStart().startsWith(CONTEXT_SUMMARY_PREFIX);
}

/** 摘要正文：去掉引擎加的开头，只留模型写的部分。 */
export function contextSummaryBody(msg: MessageData): string {
  return userPlainText(msg)
    .trimStart()
    .slice(CONTEXT_SUMMARY_PREFIX.length)
    .trim();
}

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  const m = Math.floor(s / 60);
  const rem = Math.round(s % 60);
  return `${m}m ${rem}s`;
}
