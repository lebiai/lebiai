/** Human process verbs for the dialogue canvas — not tool-trace chrome. */

export type ProcessKind =
  | "lookup"
  | "read"
  | "write"
  | "workspace"
  | "notes"
  | "saveNote"
  | "skill"
  | "agent"
  | "think"
  | "steps"
  | "zaiban"
  | "open"
  | "recall"
  | "other";

export function processKindForTool(name: string): ProcessKind {
  const n = name.toLowerCase();
  if (n === "web_search" || n === "web_fetch") return "lookup";
  if (n === "read") return "read";
  if (n === "open") return "open";
  // 翻旧账：翻的是**本会话**更早的对话（spec §9 第 7 条）。慢是允许的，但要看得见。
  if (n === "conversation_recall") return "recall";
  if (n === "write" || n === "edit") return "write";
  if (n === "bash" || n === "git" || n === "glob" || n === "grep") return "workspace";
  if (n.startsWith("palace_") || n === "memory_search" || n === "memory_delete")
    return "notes";
  if (n === "memory_save") return "saveNote";
  if (n.startsWith("skill_") || n === "propose_skill") return "skill";
  // 派活给子代理是真实动作，不能掉进「做了些事」。同时开几路也只算一件事。
  if (n === "subagent") return "agent";
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

/**
 * 落盘的消息只留工具入参 `input`，不留 `toolExecStart` 的 summary —— 于是折叠行只能
 * 说出「写了文件」，说不出写了哪一个（用户原话：「看不出写了什么」）。
 * 这里按引擎 `hermes_turn::tool_call_summary` 的同一条键位规则把它补回来。
 */
const SUMMARY_KEY: Record<string, string> = {
  bash: "command",
  read: "path",
  write: "path",
  edit: "path",
  git: "operation",
  web_fetch: "url",
  web_search: "query",
  memory_search: "query",
  memory_save: "content",
  memory_delete: "id",
  commitment_save: "title",
};

/** 入参里那条「这一次在动什么」（文件路径 / 命令 / 查询…）。 */
function toolKeyValue(name: string, input: unknown): string | undefined {
  if (!input || typeof input !== "object") return undefined;
  const rec = input as Record<string, unknown>;
  const keys =
    name === "write" || name === "edit"
      ? ["file_path", "path"]
      : [SUMMARY_KEY[name]];
  for (const key of keys) {
    if (!key) continue;
    const value = rec[key];
    if (typeof value !== "string" || !value.trim()) continue;
    return value.trim();
  }
  return undefined;
}

export function summaryFromToolInput(
  name: string,
  input: unknown
): string | undefined {
  const value = toolKeyValue(name, input);
  if (!value) return undefined;
  return `${name}: ${value.length > 120 ? value.slice(0, 120) : value}`;
}

/**
 * 这一组**写出来的东西**（写了 / 改了哪个文件）。只有写类工具算产出：读、查、
 * 跑命令都不留下給用户的文件。
 *
 * 两个来源都要认：刚落盘的消息带 `input`；正在流的工具只带 `toolExecStart` 的
 * summary（引擎拼的 `write: <path>`）—— 两条路都得能让用户点开。
 */
export function artifactPathsOf(
  tools: { name: string; summary?: string; input?: unknown }[]
): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const tool of tools) {
    if (tool.name !== "write" && tool.name !== "edit") continue;
    const fromInput = toolKeyValue(tool.name, tool.input);
    const prefix = `${tool.name}: `;
    const fromSummary =
      tool.summary && tool.summary.startsWith(prefix)
        ? tool.summary.slice(prefix.length).trim()
        : undefined;
    const path = fromInput ?? (fromSummary || undefined);
    if (!path || seen.has(path)) continue;
    seen.add(path);
    out.push(path);
  }
  return out;
}

/** 标签上给用户的短名字：去掉 `outputs/` 根（那截人人都一样），太长就留尾巴。 */
export function artifactLabel(path: string): string {
  const trimmed = path.replace(/^outputs\//, "");
  return trimmed.length > 48 ? `…${trimmed.slice(-47)}` : trimmed;
}

export type ProcessT = (key: string, params?: Record<string, string | number>) => string;

export function processHeadline(
  toolNames: string[],
  thinking: string,
  streaming: boolean,
  running: boolean,
  t: ProcessT,
  runningObject?: string
): string {
  const kinds = uniqueProcessKinds(toolNames);
  if (kinds.length > 0) {
    const doing = streaming && running;
    const acts = kinds
      .map((k) => t(doing ? `process.${k}Doing` : `process.${k}`))
      .join(t("process.join"));
    if (doing && runningObject) {
      return t("process.doingObject", { acts, obj: runningObject });
    }
    return doing ? t("process.doingPrefix", { acts }) : acts;
  }
  if (thinking.trim()) {
    return streaming ? t("process.thinking") : t("process.thought");
  }
  return streaming ? t("message.responding") : "";
}
