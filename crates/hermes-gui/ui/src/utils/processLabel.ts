/** Human process verbs for the dialogue canvas — not tool-trace chrome. */

export type ProcessKind =
  | "lookup"
  | "read"
  | "write"
  | "workspace"
  | "notes"
  | "saveNote"
  | "skill"
  | "think"
  | "steps"
  | "zaiban"
  | "open"
  | "other";

export function processKindForTool(name: string): ProcessKind {
  const n = name.toLowerCase();
  if (n === "web_search" || n === "web_fetch") return "lookup";
  if (n === "read") return "read";
  if (n === "open") return "open";
  if (n === "write" || n === "edit") return "write";
  if (n === "bash" || n === "git" || n === "glob" || n === "grep") return "workspace";
  if (n.startsWith("palace_") || n === "memory_search" || n === "memory_delete")
    return "notes";
  if (n === "memory_save") return "saveNote";
  if (n.startsWith("skill_") || n === "propose_skill") return "skill";
  if (n === "think") return "think";
  if (n.startsWith("todo_")) return "steps";
  if (n.startsWith("commitment_")) return "zaiban";
  return "other";
}

export function uniqueProcessKinds(names: string[]): ProcessKind[] {
  const seen = new Set<ProcessKind>();
  const order: ProcessKind[] = [];
  for (const name of names) {
    const kind = processKindForTool(name);
    if (!seen.has(kind)) {
      seen.add(kind);
      order.push(kind);
    }
  }
  return order;
}

export function objectFromToolSummary(
  summary: string | undefined,
  name: string
): string | undefined {
  if (!summary) return undefined;
  const trimmed = summary.trim();
  const colon = trimmed.indexOf(":");
  if (colon >= 0) {
    const rest = trimmed.slice(colon + 1).trim();
    if (rest) return rest.length > 80 ? `${rest.slice(0, 79)}…` : rest;
  }
  if (trimmed === name || trimmed.startsWith(`${name} `)) return undefined;
  return trimmed.length > 80 ? `${trimmed.slice(0, 79)}…` : trimmed;
}

export type ProcessT = (key: string, params?: Record<string, string | number>) => string;

export function processHeadline(
  toolNames: string[],
  thinking: string,
  streaming: boolean,
  running: boolean,
  t: ProcessT
): string {
  const kinds = uniqueProcessKinds(toolNames);
  if (kinds.length > 0) {
    const doing = streaming && running;
    const acts = kinds
      .map((k) => t(doing ? `process.${k}Doing` : `process.${k}`))
      .join(t("process.join"));
    return doing ? t("process.doingPrefix", { acts }) : acts;
  }
  if (thinking.trim()) {
    return streaming ? t("process.thinking") : t("process.thought");
  }
  return streaming ? t("message.responding") : "";
}
