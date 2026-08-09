export type ChatStreamEvent =
  | { event: "textDelta"; data: { text: string } }
  | { event: "thinkingDelta"; data: { text: string } }
  | { event: "toolUseStart"; data: { id: string; name: string } }
  | {
      event: "toolExecStart";
      data: { id: string; name: string; summary: string };
    }
  | { event: "toolUseResult"; data: { id: string; content: string; isError: boolean } }
  | {
      event: "confirmRequired";
      data: { id: string; toolName: string; summary: string; reason?: string };
    }
  | { event: "usageUpdate"; data: { inputTokens: number; outputTokens: number } }
  | { event: "skillCandidateProposed"; data: { name: string; description: string; body: string; triggers: string[] } }
  | { event: "error"; data: { message: string } }
  | { event: "cancelled" }
  | { event: "done" };

export interface PendingConfirm {
  id: string;
  toolName: string;
  summary: string;
  /** Why this call is especially dangerous (product policy). */
  reason?: string;
}

export type ConfirmAction = "allow" | "alwaysAllow" | "deny";

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
  /** Wall-clock ms for this assistant turn (client-measured; absent on history). */
  durationMs?: number;
  /** Per-turn token usage when known (this stream only). */
  inputTokens?: number;
  outputTokens?: number;
}

/** Display row with raw transcript span for truncate/edit. */
export interface DisplayMessage extends MessageData {
  /** Inclusive start index into raw `messages`. */
  rawStart: number;
  /** Exclusive end index into raw `messages`. */
  rawEnd: number;
}

export interface LoadedSessionData {
  id: string;
  messages: MessageData[];
  inputTokens: number;
  outputTokens: number;
}

export interface SkillCandidateView {
  name: string;
  description: string;
  triggers: string[];
  body: string;
  rationale: string;
  confidence: string;
}

export interface MemoryCandidateView {
  fact: string;
  tags: string[];
  zone?: string;
  scope: string;
  confidence: string;
  rationale: string;
  supersedes: string[];
}

export interface ConflictView {
  with: string;
  kind: string;
  explain: string;
  options: string[];
}

export interface ReflectionResult {
  summary: string;
  skillCandidates: SkillCandidateView[];
  memoryCandidates: MemoryCandidateView[];
  conflicts: ConflictView[];
}

export type SessionEndReflectionOutcome =
  | {
      status: "skipped";
      reason: string;
      userTurns: number;
      minTurns: number;
    }
  | {
      /** Quiet default: candidates landed in pending-review inbox. */
      status: "enqueued";
      added: number;
      total: number;
    }
  | {
      /** Legacy: only when reflect.pop_inbox_on_leave = true */
      status: "completed";
      reflection: ReflectionResult;
    };

export interface InboxItemView {
  id: string;
  createdAt: string;
  source: string;
  kind: "memory" | "skill" | string;
  title: string;
  body: string;
  zone?: string | null;
  tags: string[];
  confidence?: string | null;
  rationale?: string | null;
  skillName?: string | null;
  skillDescription?: string | null;
  skillTriggers?: string[] | null;
}

export function reflectionHasCandidates(r: ReflectionResult | null | undefined): boolean {
  if (!r) return false;
  const skills = r.skillCandidates?.length ?? 0;
  const memories = r.memoryCandidates?.length ?? 0;
  const conflicts = r.conflicts?.length ?? 0;
  return skills > 0 || memories > 0 || conflicts > 0;
}

/**
 * Global micro-reflection event from Rust (`hermes://micro-reflection`).
 * Not part of the turn stream Channel — see architecture in commands/micro.rs.
 */
export interface MicroReflectionEvent {
  sessionId: string;
  summary: string;
  memoryCount: number;
  skillCount: number;
  autoAccepted: number;
  reflection?: ReflectionResult;
}

export const MICRO_REFLECTION_EVENT = "hermes://micro-reflection";
