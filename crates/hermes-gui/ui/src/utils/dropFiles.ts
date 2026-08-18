/** Walk a dropped folder (Chromium / WKWebView) into a flat File list. */

type FsEntry = {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  file?: (ok: (f: File) => void, err?: (e: Error) => void) => void;
  createReader?: () => {
    readEntries: (ok: (e: FsEntry[]) => void, err?: (e: Error) => void) => void;
  };
};

async function walkEntry(entry: FsEntry, out: File[]): Promise<void> {
  if (entry.isFile && entry.file) {
    const file = await new Promise<File>((resolve, reject) => {
      entry.file!(resolve, reject);
    });
    out.push(file);
    return;
  }
  if (entry.isDirectory && entry.createReader) {
    const reader = entry.createReader();
    const children: FsEntry[] = [];
    for (;;) {
      const batch = await new Promise<FsEntry[]>((resolve, reject) => {
        reader.readEntries(resolve, reject);
      });
      if (!batch.length) break;
      children.push(...batch);
    }
    for (const child of children) {
      await walkEntry(child, out);
    }
  }
}

/** Finder often hands a 0-byte dummy when a folder didn't expand. */
export function isLikelyFolderDummy(file: File): boolean {
  if (file.size > 0) return false;
  if (file.name.includes(".")) return false;
  return file.type === "" || file.type === "application/x-directory";
}

export async function filesFromDataTransfer(dt: DataTransfer): Promise<File[]> {
  const items = dt.items;
  if (items && items.length > 0) {
    const out: File[] = [];
    const jobs: Promise<void>[] = [];
    for (let i = 0; i < items.length; i++) {
      const raw = items[i] as DataTransferItem & {
        webkitGetAsEntry?: () => FsEntry | null;
      };
      const entry = raw.webkitGetAsEntry?.();
      if (entry) jobs.push(walkEntry(entry, out));
    }
    if (jobs.length > 0) {
      await Promise.all(jobs);
      if (out.length > 0) return out;
    }
  }
  return Array.from(dt.files ?? []);
}
