export type ChatStreamEvent =
  | { event: "textDelta"; data: { text: string } }
  | { event: "textCorrected"; data: { text: string } }
  | { event: "thinkingDelta"; data: { text: string } }
  | { event: "toolUseStart"; data: { id: string; name: string } }
  | {
      event: "toolExecStart";
      data: { id: string; name: string; summary: string };
    }
  | { event: "toolUseResult"; data: { id: string; content: string; isError: boolean } }
  | { event: "usageUpdate"; data: { inputTokens: number; outputTokens: number } }
  | { event: "skillCandidateProposed"; data: { name: string; description: string; body: string; triggers: string[] } }
  | {
      event: "zaibanUpdated";
      data: {
        action: string;
        id?: string;
        title?: string;
        existingId?: string;
        existingTitle?: string;
      };
    }
  | { event: "error"; data: { message: string } }
  | { event: "cancelled" }
  | { event: "rememberQueued" }
  | {
      event: "contextCompacted";
      data: { replaced: number; beforeTokens: number; afterTokens: number };
    }
  | { event: "done" };

export interface SessionSummary {
  id: string;
  title: string;
  createdAt: string;
  /** Last activity (file mtime); used for day grouping. */
  updatedAt?: string;
  path: string;
  channel?: string | null;
  readOnly?: boolean;
  /** 工位（人物）id；null 表示自由对话。 */
  persona?: string | null;
  /** 项目组 id；null 表示不属于任何项目组。与 persona 互斥。 */
  team?: string | null;
}

/** 桌上一行。`present=false` = 缺席（灰掉、不可点、写明缺谁）。 */
export interface TeamMemberItem {
  id: string;
  name: string;
  /** 他在这张桌子上的那一摊活。 */
  duty: string;
  present: boolean;
}

/** 侧栏「项目组」那一行。members 是这张桌子的名册（含缺席）。 */
export interface TeamItem {
  id: string;
  name: string;
  /** 这是个什么活（一行小字用）。 */
  role: string;
  members: TeamMemberItem[];
  /** 缺了几个人（0 = 到齐）。 */
  missing: number;
  /** 接口人在册才跑得起来：没人接的桌子开不出一条会话。 */
  canRun: boolean;
  /** 一行小字：缺人时说缺谁，到齐了说这活跑到哪了。 */
  hint: string;
  /** 组会话里这一轮由谁开口（接棒的；没交过棒 → 第一棒采集）。 */
  speakerId: string;
}

/** 一份「决定」在界面上的样子（引擎写的头 + 人物写的正文）。 */
export interface DecisionItemView {
  title: string;
  note?: string;
}

export interface DecisionView {
  kind: "topicList" | "review";
  /** 「选题单」/「审稿意见」——引擎给的中文名，界面直接用。 */
  kindLabel: string;
  /** 谁定的（人物 id / 名字）。 */
  by: string;
  byName: string;
  at: string;
  /** 为什么这么定。 */
  why: string;
  status: "pending" | "approved" | "revised" | "settled";
  statusLabel: string;
  items: DecisionItemView[];
  verdict?: string;
  answeredAt?: string;
  answeredNote?: string;
}

/** 今天的一件产物。带决定头的会多一张卡（`decision`）。 */
export interface EpisodeItem {
  relPath: string;
  name: string;
  ext: string;
  modified: string;
  decision?: DecisionView;
}

/** 这一棒是怎么传下来的。 */
export interface HandoffView {
  fromName?: string;
  toName: string;
  toId: string;
  at: string;
}

/** 今天走到了哪。真源是文件与会话事件，不另存一份。界面不用「期」这个面。 */
export interface EpisodeView {
  day: string;
  /** 这一棒现在在谁手上（没交过 = 第一棒采集）。 */
  holderId: string;
  holderName: string;
  handoffs: HandoffView[];
  items: EpisodeItem[];
  /** 有没有**待你点头**的决定（唯一必停的一步）。 */
  pendingDecision: boolean;
}

/** A 工位 the user can talk to. `enabled` is computed by the engine. */
export interface PersonaItem {
  id: string;
  name: string;
  role: string;
  builtin: boolean;
  enabled: boolean;
}

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string }
  | { type: "toolUse"; id: string; name: string; input: unknown }
  | { type: "toolResult"; toolUseId: string; content: string; isError: boolean };

export interface MessageData {
  role: "user" | "assistant";
  content: ContentBlock[];
  /** 这一轮开口的人（人物 id）。组会话里棒会换人，界面靠它标名字。 */
  speaker?: string;
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
  channel?: string | null;
  readOnly?: boolean;
  /** 工位（人物）id；null 表示自由对话。 */
  persona?: string | null;
  /** 项目组 id；null 表示不属于任何项目组。与 persona 互斥。 */
  team?: string | null;
  /**
   * 窗口前面还有多少条消息（会话数组里的下标）。**编辑重发的截断要加回去**：
   * 窗口内的下标不是文件里的下标。
   */
  baseOffset: number;
  /** 更早的日子（最新在前）。空 = 没有折叠条（短会话）。 */
  days: SessionDay[];
}

/** 一条被折起来的「那天」。 */
export interface SessionDay {
  /** 本地日 YYYY-MM-DD；null = 那段消息没有日期（老文件 / 压缩摘要）。 */
  day: string | null;
  /** 界面上写的一行：`9 月 16 日` / `更早`。 */
  label: string;
  /** 人说了几句。 */
  turns: number;
  /** 这一段有多少条消息。 */
  messages: number;
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
  /** 点头之后会归到哪。判据在 Rust 侧，这里只显示。 */
  ownerId?: string | null;
  ownerName?: string | null;
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
