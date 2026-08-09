import type { SessionSummary } from "../types";

export type SessionGroupId = "today" | "yesterday" | "earlier";

export interface SessionGroup {
  id: SessionGroupId;
  sessions: SessionSummary[];
}

function startOfLocalDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function dayBucket(iso: string, now = new Date()): SessionGroupId {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "earlier";
  const day = startOfLocalDay(new Date(t));
  const today = startOfLocalDay(now);
  const yesterday = today - 86_400_000;
  if (day === today) return "today";
  if (day === yesterday) return "yesterday";
  return "earlier";
}

/** Group sessions preserving input order within each bucket. */
export function groupSessionsByDay(sessions: SessionSummary[]): SessionGroup[] {
  const buckets: Record<SessionGroupId, SessionSummary[]> = {
    today: [],
    yesterday: [],
    earlier: [],
  };
  for (const s of sessions) {
    buckets[dayBucket(s.createdAt)].push(s);
  }
  const order: SessionGroupId[] = ["today", "yesterday", "earlier"];
  return order
    .filter((id) => buckets[id].length > 0)
    .map((id) => ({ id, sessions: buckets[id] }));
}

/** Short relative / local time for list secondary line. */
export function formatSessionTime(iso: string, locale: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "";
  const d = new Date(t);
  const now = new Date();
  const sameDay = startOfLocalDay(d) === startOfLocalDay(now);
  try {
    if (sameDay) {
      return d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleDateString(locale, { month: "short", day: "numeric" });
  } catch {
    return d.toISOString().slice(0, 10);
  }
}
