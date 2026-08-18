/**
 * User messages may end with a machine-readable block:
 *
 *   [attachments]
 *   - uploads/sess/id_name.md (original: name.doc, 1841 chars)
 *
 * Agent still receives this text; UI parses it for cards.
 */

export type ParsedAttachment = {
  mdRelPath: string;
  originalName: string;
  chars?: number;
};

const ATTACHMENTS_HEADER = /^\[attachments\]\s*$/im;
/**
 * Paths and originals may contain spaces, e.g. `file (2).md`.
 * Do NOT use \S+ for the path — use greedy path + backtrack to ` (original:`.
 */
const ATTACH_LINE =
  /^-\s+(.+)\s+\(original:\s*(.+),\s*(\d+)\s*chars\)\s*$/i;

export function splitUserTextAndAttachments(text: string): {
  body: string;
  attachments: ParsedAttachment[];
} {
  const match = text.match(ATTACHMENTS_HEADER);
  if (!match || match.index === undefined) {
    return { body: text, attachments: [] };
  }

  const headerStart = match.index;
  const afterHeader = text.slice(headerStart + match[0].length);
  // Attachments section runs to end of message
  const body = text.slice(0, headerStart).replace(/\s+$/, "");
  const attachments: ParsedAttachment[] = [];

  for (const line of afterHeader.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const m = trimmed.match(ATTACH_LINE);
    if (m) {
      attachments.push({
        mdRelPath: m[1].trim(),
        originalName: m[2].trim(),
        chars: Number(m[3]),
      });
    }
  }

  return { body, attachments };
}

const ON_HAND_HEADER = /^\[on-hand\]\s*$/im;

/** Catalog the engine appended when the user asked what files are on hand. */
export function splitOnHand(text: string): { body: string; titles: string[] } {
  const match = text.match(ON_HAND_HEADER);
  if (!match || match.index === undefined) {
    return { body: text, titles: [] };
  }
  const body = text.slice(0, match.index).replace(/\s+$/, "");
  const titles: string[] = [];
  for (const line of text.slice(match.index + match[0].length).split("\n")) {
    const m = line.trim().match(/^-\s*《(.+)》\s*$/);
    if (m) titles.push(m[1]);
    else if (line.trim() === "(none)") {
      /* empty catalog */
    }
  }
  return { body, titles };
}

export function wantsSpokenKeep(text: string): boolean {
  const t = text.toLowerCase();
  return (
    t.includes("留下") ||
    t.includes("收着") ||
    t.includes("以后按这个") ||
    t.includes("下次还用") ||
    t.includes("下次还按") ||
    t.includes("keep this") ||
    t.includes("keep it for next") ||
    t.includes("save this file")
  );
}

export function formatAttachmentsBlock(
  items: { mdRelPath: string; originalName: string; chars: number }[]
): string {
  if (items.length === 0) return "";
  // Keep one line per file; spaces in names are OK (parser backtracks to " (original:").
  const lines = items.map(
    (a) =>
      `- ${a.mdRelPath} (original: ${a.originalName}, ${a.chars} chars)`
  );
  return `\n\n[attachments]\n${lines.join("\n")}`;
}

/** Strip noisy IPC prefixes for toast. */
export function importErrorCode(err: unknown): string | null {
  const s = String(err);
  const m = s.match(/\b(too_large|unsupported_type|conversion_failed|empty_markdown|encrypted|markitdown_missing|io_error)\b/i);
  return m ? m[1].toLowerCase() : null;
}

export function humanizeImportError(err: unknown): string {
  let s = String(err);
  s = s.replace(/^tool:\s*/i, "");
  s = s.replace(/^import_document:\s*/i, "");
  // code: message  or  code: nested: message
  const coded = s.match(/^[a-z_]+:\s*(.+)$/i);
  if (coded) {
    const rest = coded[1];
    const nested = rest.match(/^[a-z_]+:\s*(.+)$/i);
    return nested ? nested[1] : rest;
  }
  return s;
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function isAcceptedDocumentFile(file: File): boolean {
  const name = file.name.toLowerCase();
  return (
    name.endsWith(".pdf") ||
    name.endsWith(".doc") ||
    name.endsWith(".docx") ||
    name.endsWith(".xlsx") ||
    name.endsWith(".csv") ||
    name.endsWith(".txt") ||
    name.endsWith(".md")
  );
}
