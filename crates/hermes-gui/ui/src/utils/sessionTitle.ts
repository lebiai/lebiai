/** Backend / legacy default session titles treated as "empty draft". */
const DEFAULT_TITLES = new Set([
  "New Chat",
  "New dialogue",
  "new chat",
  "new dialogue",
  "新聊天",
  "新对话",
  "Untitled",
  "未命名",
]);

/** Short greetings — not useful as a permanent session title (align with Rust). */
const TRIVIAL = new Set([
  "hi",
  "hi!",
  "hey",
  "hey!",
  "hello",
  "hello!",
  "你好",
  "你好啊",
  "你好!",
  "你好！",
  "您好",
  "您好！",
  "哈喽",
  "嗨",
  "在吗",
  "在吗?",
  "在吗？",
  "早上好",
  "晚上好",
  "下午好",
  "早",
  "晚安",
  "谢谢",
  "thanks",
  "thank you",
  "ok",
  "okay",
  "好的",
  "嗯",
  "嗯嗯",
  "哦",
  "噢",
]);

export function isDefaultTitle(title: string | null | undefined): boolean {
  if (!title) return true;
  return DEFAULT_TITLES.has(title.trim());
}

export function isTrivialUserText(text: string): boolean {
  const t = text.trim();
  if (!t) return true;
  const chars = Array.from(t);
  if (chars.length <= 1) return true;
  const lower = t.toLowerCase();
  if (TRIVIAL.has(lower) || TRIVIAL.has(t)) return true;
  if (t.startsWith("你好") && chars.length <= 4) return true;
  return false;
}

/** Short title derived from a user message (skip pure greetings at caller). */
export function deriveSessionTitle(content: string, maxChars = 60): string {
  const trimmed = content.trim();
  if (!trimmed || isTrivialUserText(trimmed)) return "New Chat";
  const chars = Array.from(trimmed);
  if (chars.length <= maxChars) return chars.join("");
  return chars.slice(0, maxChars - 3).join("") + "...";
}
