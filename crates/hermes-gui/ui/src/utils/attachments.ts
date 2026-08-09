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
