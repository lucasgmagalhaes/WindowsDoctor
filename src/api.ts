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

export type CleanupLevel = "safe" | "review";

export interface CleanupCategory {
  id: string;
  group: string;
  title: string;
  detail: string;
  level: CleanupLevel;
  needsAdmin: boolean;
  /** null when the size is only known after running (DISM). */
  size: number | null;
  files: number;
  paths: string[];
}

export interface CleanupReport {
  isAdmin: boolean;
  categories: CleanupCategory[];
}

export interface CleanupResult {
  id: string;
  title: string;
  freed: number;
  files: number;
  /** Items that could not be deleted (in use, no permission…). */
  failed: number;
  error: string | null;
}

export interface CleanupRunResult {
  results: CleanupResult[];
  /** Totals of the current scan after updating it; null without a scan. */
  stats: Stats | null;
}

export interface CleanupProgress {
  index: number;
  total: number;
  title: string;
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
    disks: {
      name: string;
      mediaType: string;
      health: string;
      size: number;
      temperature: number | null;
      wear: number | null;
      powerOnHours: number | null;
    }[];
    temps: { label: string; celsius: number }[];
    battery: { name: string; designCapacity: number; fullChargeCapacity: number; cycleCount: number | null } | null;
  };
  checks: Check[];
  monitorsDdc: { description: string; model: string | null; firmware: string | null; mccsVersion: string | null; ddc: boolean }[];
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

export type RestoreOutcome =
  | { kind: "created" }
  | { kind: "recent" }
  | { kind: "notAdmin" }
  | { kind: "failed"; message: string };

export type Undo = { type: "tweak"; id: string; apply: boolean } | { type: "startup"; id: string; enabled: boolean };

export interface HistoryEntry {
  id: number;
  timeMs: number;
  kind: string;
  title: string;
  detail: string;
  undo: Undo | null;
  undone: boolean;
}

export interface BlackScreen {
  recordId: number;
  timeUtc: string;
  provider: string;
  eventId: number;
  foregroundApp: string | null;
}

export type RepairKind = "sfc" | "dism" | "chkdsk" | "memory" | "compactos" | "uncompactos";

export interface ShadowInfo {
  drive: string;
  used: number;
  max: number;
  driveSize: number;
  copies: number;
  restorePoints: { sequence: number; description: string; time: string }[];
}

export interface RepairOutcome {
  kind: string;
  title: string;
  exitCode: number | null;
  ok: boolean;
  tail: string[];
}

export interface DuplicateGroup {
  size: number;
  wasted: number;
  paths: string[];
}

export type Frequency = "daily" | "weekly" | "monthly";
export interface ScheduleInfo {
  enabled: boolean;
  nextRun: string | null;
}

export interface Program {
  id: string;
  name: string;
  publisher: string;
  version: string;
  installDate: string | null;
  size: number | null;
  installLocation: string | null;
  uninstall: string;
}

export interface Comparison {
  previousTimeMs: number;
  previousTotal: number;
  currentTotal: number;
  changes: { path: string; before: number; after: number; delta: number }[];
}

export interface ExtStat {
  ext: string;
  size: number;
  count: number;
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
  /** Espaço por extensão na última varredura completa. */
  getExtensions(): Promise<ExtStat[]>;
  /** null na primeira varredura de um local. */
  getChanges(): Promise<Comparison | null>;
  /** Pergunta onde salvar; false se cancelar. */
  saveScan(): Promise<boolean>;
  /** Pergunta qual abrir; null se cancelar. */
  openScan(): Promise<Stats | null>;
  findPath(path: string): Promise<number | null>;
  reveal(path: string, select: boolean): Promise<void>;
  /** Deletes by node `id` or, for grouped files from the top list, by `path`. */
  deleteNode(target: { id: number } | { path: string }, permanent: boolean): Promise<DeleteResult>;
  pickFolder(): Promise<string | null>;
  cleanupAnalyze(): Promise<CleanupReport>;
  cleanupRun(ids: string[]): Promise<CleanupRunResult>;
  onCleanupProgress(cb: (p: CleanupProgress) => void): Promise<() => void>;
  onProgress(cb: (p: Progress) => void): Promise<() => void>;
  onFinished(cb: (f: ScanFinished) => void): Promise<() => void>;

  isElevated(): Promise<boolean>;
  appVersion(): Promise<string>;
  /** Pergunta onde salvar e grava o HTML; false se o usuário cancelar. */
  exportHtml(defaultName: string, html: string): Promise<boolean>;
  /** Abre uma página do GitHub no navegador. */
  openUrl(url: string): Promise<void>;
  /** Cria um ponto de restauração na primeira alteração da sessão; null se já foi feito. */
  ensureRestorePoint(reason: string): Promise<RestoreOutcome | null>;
  listHistory(): Promise<HistoryEntry[]>;
  setBlackScreenMonitor(enabled: boolean): Promise<void>;
  onBlackScreen(cb: (e: BlackScreen) => void): Promise<() => void>;
  undoHistory(id: number): Promise<void>;
  clearHistory(): Promise<void>;
  /** Reabre o app como administrador (fecha esta janela se der certo). */
  relaunchAsAdmin(): Promise<void>;
  systemReport(): Promise<Report>;
  /** Leva ~15 s. */
  networkTest(): Promise<{ checks: Check[] }>;
  cleanupScheduleStatus(): Promise<ScheduleInfo>;
  shadowInfo(): Promise<ShadowInfo>;
  deleteOldShadows(): Promise<ShadowInfo>;
  setShadowLimit(percent: number): Promise<ShadowInfo>;
  listPrograms(): Promise<Program[]>;
  /** Abre o desinstalador do programa. */
  uninstallProgram(id: string): Promise<void>;
  /** Pastas que sobraram, com o tamanho. */
  findLeftovers(id: string): Promise<[string, number][]>;
  deleteLeftovers(paths: string[]): Promise<number>;
  /** null desliga. */
  setCleanupSchedule(frequency: Frequency | null): Promise<ScheduleInfo>;
  /** Sem pastas, procura na pasta do usuário. */
  findDuplicates(roots: string[] | null, minSize: number): Promise<DuplicateGroup[]>;
  cancelDuplicates(): Promise<void>;
  /** Envia para a Lixeira; devolve os bytes liberados. */
  deleteDuplicates(paths: string[]): Promise<number>;
  onDuplicatesProgress(cb: (p: { files: number; hashed: number }) => void): Promise<() => void>;
  runRepair(kind: RepairKind): Promise<RepairOutcome>;
  onRepairProgress(cb: (p: { line: string; percent: number | null }) => void): Promise<() => void>;
  listTweaks(): Promise<Tweak[]>;
  setTweak(id: string, apply: boolean): Promise<Tweak>;
  listStartup(): Promise<StartupEntry[]>;
  setStartup(id: string, enabled: boolean): Promise<void>;
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
    getExtensions: () => invoke("get_extensions"),
    getChanges: () => invoke("get_changes"),
    saveScan: async () => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: "varredura.wdscan", filters: [{ name: "Varredura do WindowsDoctor", extensions: ["wdscan"] }] });
      if (!path) return false;
      await invoke("save_scan", { path });
      return true;
    },
    openScan: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Varredura do WindowsDoctor", extensions: ["wdscan"] }] });
      if (typeof path !== "string") return null;
      return invoke("open_scan", { path });
    },
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
    cleanupAnalyze: () => invoke("cleanup_analyze"),
    cleanupRun: (ids) => invoke("cleanup_run", { ids }),
    onCleanupProgress: (cb) => listen<CleanupProgress>("cleanup-progress", (e) => cb(e.payload)),
    onProgress: (cb) => listen<Progress>("scan-progress", (e) => cb(e.payload)),
    onFinished: (cb) => listen<ScanFinished>("scan-finished", (e) => cb(e.payload)),
    isElevated: () => invoke("is_elevated"),
    appVersion: () => import("@tauri-apps/api/app").then((m) => m.getVersion()),
    openUrl: (url) => invoke("open_url", { url }),
    exportHtml: async (defaultName, html) => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: defaultName, filters: [{ name: "Página HTML", extensions: ["html"] }] });
      if (!path) return false;
      await invoke("save_report", { path, content: html });
      return true;
    },
    ensureRestorePoint: (reason) => invoke("ensure_restore_point", { reason }),
    listHistory: () => invoke("list_history"),
    setBlackScreenMonitor: (enabled) => invoke("set_black_screen_monitor", { enabled }),
    onBlackScreen: (cb) => listen<BlackScreen>("black-screen", (e) => cb(e.payload)),
    undoHistory: (id) => invoke("undo_history", { id }),
    clearHistory: () => invoke("clear_history"),
    relaunchAsAdmin: () => invoke("relaunch_as_admin"),
    systemReport: () => invoke("system_report"),
    networkTest: () => invoke("network_test"),
    cleanupScheduleStatus: () => invoke("cleanup_schedule_status"),
    shadowInfo: () => invoke("shadow_info"),
    deleteOldShadows: () => invoke("delete_old_shadows"),
    setShadowLimit: (percent) => invoke("set_shadow_limit", { percent }),
    listPrograms: () => invoke("list_programs"),
    uninstallProgram: (id) => invoke("uninstall_program", { id }),
    findLeftovers: (id) => invoke("find_leftovers", { id }),
    deleteLeftovers: (paths) => invoke("delete_leftovers", { paths }),
    setCleanupSchedule: (frequency) => invoke("set_cleanup_schedule", { frequency }),
    findDuplicates: (roots, minSize) => invoke("find_duplicates", { roots, minSize }),
    cancelDuplicates: () => invoke("cancel_duplicates"),
    deleteDuplicates: (paths) => invoke("delete_duplicates", { paths }),
    onDuplicatesProgress: (cb) => listen<{ files: number; hashed: number }>("duplicates-progress", (e) => cb(e.payload)),
    runRepair: (kind) => invoke("run_repair", { kind }),
    onRepairProgress: (cb) => listen<{ line: string; percent: number | null }>("repair-progress", (e) => cb(e.payload)),
    listTweaks: () => invoke("list_tweaks"),
    setTweak: (id, apply) => invoke("set_tweak", { id, apply }),
    listStartup: () => invoke("list_startup"),
    setStartup: (id, enabled) => invoke("set_startup", { id, enabled }),
  };
}

let apiPromise: Promise<Api> | null = null;

export function getApi(): Promise<Api> {
  if (!apiPromise) {
    apiPromise = isTauri ? tauriApi() : import("./mockApi").then((m) => m.createMockApi());
  }
  return apiPromise;
}
