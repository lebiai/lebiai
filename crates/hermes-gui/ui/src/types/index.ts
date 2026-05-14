export type ChatStreamEvent =
  | { event: "textDelta"; data: { text: string } }
  | { event: "thinkingDelta"; data: { text: string } }
  | { event: "toolUseStart"; data: { id: string; name: string } }
  | { event: "toolUseResult"; data: { id: string; content: string; isError: boolean } }
  | { event: "usageUpdate"; data: { inputTokens: number; outputTokens: number } }
  | { event: "microReflection"; data: { summary: string; memoryCount: number; skillCount: number } }
  | { event: "error"; data: { message: string } }
  | { event: "done" };

export interface SessionSummary {
  id: string;
  title: string;
  createdAt: string;
  path: string;
}

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string }
  | { type: "toolUse"; id: string; name: string; input: unknown }
  | { type: "toolResult"; toolUseId: string; content: string; isError: boolean };

export interface MessageData {
  role: "user" | "assistant";
  content: ContentBlock[];
}

export interface LoadedSessionData {
  id: string;
  messages: MessageData[];
  inputTokens: number;
  outputTokens: number;
}
