/** Result of Tauri `import_document` (hermes_tools::ImportResult). */
export type FileImportResult = {
  ok: boolean;
  fileId: string;
  mdRelPath: string;
  displayName: string;
  originalName: string;
  sourceExt: string;
  kind: string;
  chars: number;
  bytesMd: number;
  originalDeleted: boolean;
  warning?: string;
};

export type ConverterStatus = {
  available: boolean;
  binaryPath?: string;
  version?: string;
  error?: string;
};
