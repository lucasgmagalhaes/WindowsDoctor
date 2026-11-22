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

export interface StartScanArgs {
  path?: string;
  nodeId?: number;
  minFileSize?: number;
}

// ---------------------------------------------------------------- ferramentas

export type CheckStatus = "ok" | "info" | "warn" | "crit";

export interface Check {
  id: string;
  title: string;
  status: CheckStatus;
  summary: string;
  details: string[];
  actions: string[];
  /** Otimização que resolve ou ajuda (id de `Tweak`). */
  tweak: string | null;
  /** Módulo do app que ajuda. */
  module: "cleanup" | "diskmap" | null;
}

export interface Gpu {
  name: string;
  driverVersion: string;
  driverDate: string;
  driverAgeDays: number | null;
  refreshRate: number;
  width: number;
  height: number;
}

export interface Report {
  summary: {
    os: string;
    cpu: string;
    ramTotal: number;
    ramFree: number;
    uptimeSecs: number;
    gpus: Gpu[];
    monitors: { manufacturer: string; name: string; connection: string }[];
    disks: { name: string; mediaType: string; health: string; size: number }[];
  };
  checks: Check[];
}

export interface Tweak {
  id: string;
  title: string;
  description: string;
  category: string;
  needsAdmin: boolean;
  needsRestart: boolean;
  /** null quando o estado não pôde ser lido (veja `error`). */
  applied: boolean | null;
  error: string | null;
}

export interface StartupEntry {
  id: string;
  name: string;
  command: string;
  location: string;
  enabled: boolean;
  needsAdmin: boolean;
}

export interface CleanupTarget {
  id: string;
  title: string;
  description: string;
  needsAdmin: boolean;
  size: number;
  files: number;
  paths: string[];
  note: string | null;
}

export interface CleanResult {
  id: string;
  freed: number;
  deleted: number;
  skipped: number;
  error: string | null;
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
  onProgress(cb: (p: Progress) => void): Promise<() => void>;
  onFinished(cb: (f: ScanFinished) => void): Promise<() => void>;

  isElevated(): Promise<boolean>;
  /** Reabre o app como administrador (fecha esta janela se der certo). */
  relaunchAsAdmin(): Promise<void>;
  systemReport(): Promise<Report>;
  listTweaks(): Promise<Tweak[]>;
  setTweak(id: string, apply: boolean): Promise<Tweak>;
  listStartup(): Promise<StartupEntry[]>;
  setStartup(id: string, enabled: boolean): Promise<void>;
  listCleanup(): Promise<CleanupTarget[]>;
  runCleanup(ids: string[]): Promise<CleanResult[]>;
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
    onProgress: (cb) => listen<Progress>("scan-progress", (e) => cb(e.payload)),
    onFinished: (cb) => listen<ScanFinished>("scan-finished", (e) => cb(e.payload)),
    isElevated: () => invoke("is_elevated"),
    relaunchAsAdmin: () => invoke("relaunch_as_admin"),
    systemReport: () => invoke("system_report"),
    listTweaks: () => invoke("list_tweaks"),
    setTweak: (id, apply) => invoke("set_tweak", { id, apply }),
    listStartup: () => invoke("list_startup"),
    setStartup: (id, enabled) => invoke("set_startup", { id, enabled }),
    listCleanup: () => invoke("list_cleanup"),
    runCleanup: (ids) => invoke("run_cleanup", { ids }),
  };
}

let apiPromise: Promise<Api> | null = null;

export function getApi(): Promise<Api> {
  if (!apiPromise) {
    apiPromise = isTauri ? tauriApi() : import("./mockApi").then((m) => m.createMockApi());
  }
  return apiPromise;
}
