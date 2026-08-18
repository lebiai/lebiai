/** Result of Tauri `import_document` (hermes_tools::ImportResult). */
export type KeptMaterial = {
  item: {
    id: string;
    title: string;
    readable?: boolean;
  };
  kind: "created" | "duplicate" | "new_version" | string;
};

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
  kept?: KeptMaterial;
};

export type ConverterStatus = {
  available: boolean;
  binaryPath?: string;
  version?: string;
  error?: string;
};
