// Camada de acesso ao backend. Dentro do Tauri usa `invoke`; no navegador
// (npm run dev sem Tauri) usa um backend simulado para desenvolver a UI.

export type NodeKind = "dir" | "file" | "others" | "rest";

export interface NodeView {
  id: number | null;
  name: string;
  size: number;
  fileCount: number;
  dirCount: number;
  kind: NodeKind;
  hasChildren: boolean;
  error: boolean;
  hiddenItems: number;
  children: NodeView[];
}

export interface Drive {
  mount: string;
  label: string;
  fileSystem: string;
  total: number;
  available: number;
  removable: boolean;
  kind: string;
}

export interface Progress {
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
  current: string;
}

export interface Stats {
  root: string;
  totalSize: number;
  fileCount: number;
  dirCount: number;
  errors: number;
  cancelled: boolean;
  elapsedMs: number;
}

export interface ScanFinished {
  ok: boolean;
  error: string | null;
  stats: Stats | null;
  rescannedPath: string | null;
  /** On a cancelled rescan the folder keeps its previous data. */
  cancelled: boolean;
}

export interface Crumb {
  id: number;
  name: string;
}

export interface FileInfo {
  path: string;
  size: number;
}

export interface DeleteResult {
  freed: number;
  stats: Stats;
}

export type CleanupLevel = "safe" | "review" | "system";
export type CleanupAction = "clean" | "recycleBin" | "manual";

/** A location from the cleanup catalogue (resolved by the backend). */
export interface CleanupItem {
  id: string;
  title: string;
  description: string;
  level: CleanupLevel;
  action: CleanupAction;
  paths: string[];
  size: number | null;
  fileCount: number | null;
  minAgeHours: number;
  needsAdmin: boolean;
  note: string | null;
  /** Windows tool that handles the item (opened with openSystemTool). */
  tool: string | null;
  /** Command run as administrator (runElevated) and its text. */
  commandId: string | null;
  command: string | null;
}

export interface CleanReport {
  freed: number;
  deleted: number;
  failed: number;
  skippedRecent: number;
}

export interface CleanupResult {
  id: string;
  title: string;
  report: CleanReport;
  error: string | null;
}

export interface CleanupRun {
  results: CleanupResult[];
  total: CleanReport;
  /** Updated stats of the current scan (if there is one). */
  stats: Stats | null;
}

export interface StartScanArgs {
  path?: string;
  nodeId?: number;
  minFileSize?: number;
}

export interface Api {
  isMock: boolean;
  listDrives(): Promise<Drive[]>;
  startScan(args: StartScanArgs): Promise<void>;
  cancelScan(): Promise<void>;
  getStats(): Promise<Stats>;
  getNode(id: number | null, depth: number, maxChildren: number, minFraction?: number): Promise<NodeView>;
  getAncestors(id: number): Promise<Crumb[]>;
  getPath(id: number): Promise<string>;
  getTopFiles(limit: number): Promise<FileInfo[]>;
  findPath(path: string): Promise<number | null>;
  reveal(path: string, select: boolean): Promise<void>;
  /** Deletes by node `id` or, for grouped files from the top list, by `path`. */
  deleteNode(target: { id: number } | { path: string }, permanent: boolean): Promise<DeleteResult>;
  pickFolder(): Promise<string | null>;
  analyzeCleanup(): Promise<CleanupItem[]>;
  runCleanup(ids: string[]): Promise<CleanupRun>;
  openSystemTool(tool: string): Promise<void>;
  runElevated(commandId: string): Promise<void>;
  onProgress(cb: (p: Progress) => void): Promise<() => void>;
  onFinished(cb: (f: ScanFinished) => void): Promise<() => void>;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const { open } = await import("@tauri-apps/plugin-dialog");
  return {
    isMock: false,
    listDrives: () => invoke("list_drives"),
    startScan: (a) => invoke("start_scan", { path: a.path, nodeId: a.nodeId, minFileSize: a.minFileSize }),
    cancelScan: () => invoke("cancel_scan"),
    getStats: () => invoke("get_stats"),
    getNode: (id, depth, maxChildren, minFraction) =>
      invoke("get_node", { id, depth, maxChildren, minFraction }),
    getAncestors: (id) => invoke("get_ancestors", { id }),
    getPath: (id) => invoke("get_path", { id }),
    getTopFiles: (limit) => invoke("get_top_files", { limit }),
    findPath: (path) => invoke("find_path", { path }),
    reveal: (path, select) => invoke("reveal_path", { path, select }),
    deleteNode: (target, permanent) =>
      invoke("delete_node", {
        id: "id" in target ? target.id : null,
        path: "path" in target ? target.path : null,
        permanent,
      }),
    pickFolder: async () => {
      const r = await open({ directory: true, multiple: false });
      return typeof r === "string" ? r : null;
    },
    analyzeCleanup: () => invoke("analyze_cleanup"),
    runCleanup: (ids) => invoke("run_cleanup", { ids }),
    openSystemTool: (tool) => invoke("open_system_tool", { tool }),
    runElevated: (commandId) => invoke("run_elevated", { commandId }),
    onProgress: (cb) => listen<Progress>("scan-progress", (e) => cb(e.payload)),
    onFinished: (cb) => listen<ScanFinished>("scan-finished", (e) => cb(e.payload)),
  };
}

let apiPromise: Promise<Api> | null = null;

export function getApi(): Promise<Api> {
  if (!apiPromise) {
    apiPromise = isTauri ? tauriApi() : import("./mockApi").then((m) => m.createMockApi());
  }
  return apiPromise;
}
