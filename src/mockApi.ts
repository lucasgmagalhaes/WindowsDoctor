// Backend simulado usado quando a UI roda no navegador fora do Tauri.
// Reproduz a mesma semântica do crate `diskscan` (agregação Rest/Others,
// remoção com propagação de tamanhos etc.) sobre uma partição fictícia.

import { isProtected } from "./hints";
import type { Api, CleanupItem, CleanupResult, Crumb, FileInfo, NodeKind, NodeView, Progress, ScanFinished, Stats } from "./api";

interface MNode {
  id: number;
  name: string;
  parent: number;
  size: number;
  fileCount: number;
  dirCount: number;
  kind: Exclude<NodeKind, "rest">;
  children: number[];
  removed: boolean;
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;

// Gerador determinístico para a árvore ser sempre a mesma.
function rng(seed: number) {
  return () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 2 ** 32;
  };
}

type Spec = [string, number | Spec[]];

// Estrutura típica de um C:\ para a demonstração (tamanhos em bytes nas folhas).
const C_DRIVE: Spec[] = [
  ["Windows", [
    ["WinSxS", 9.8 * GB],
    ["System32", 6.1 * GB],
    ["SoftwareDistribution", [["Download", 3.4 * GB], ["DataStore", 0.4 * GB]]],
    ["Installer", 2.9 * GB],
    ["Temp", 1.6 * GB],
    ["SysWOW64", 1.4 * GB],
    ["assembly", 0.9 * GB],
  ]],
  ["Users", [
    ["voce", [
      ["AppData", [
        ["Local", [
          ["Temp", 7.2 * GB],
          ["Google", [["Chrome", [["User Data", [["Default", [["Cache", 2.1 * GB], ["Service Worker", 1.2 * GB]]]]]]]]],
          ["Docker", [["wsl", [["disk", [["ext4.vhdx", 24 * GB]]]]]]],
          ["NVIDIA", [["DXCache", 1.3 * GB]]],
          ["npm-cache", 2.6 * GB],
          ["CrashDumps", 0.8 * GB],
        ]],
        ["Roaming", [["Spotify", 3.1 * GB], ["Code", 1.1 * GB], ["discord", 0.7 * GB]]],
      ]],
      ["Downloads", [
        ["Win11_23H2.iso", 6.2 * GB],
        ["instaladores", 4.3 * GB],
        ["videos", 3.8 * GB],
      ]],
      ["Videos", 18.5 * GB],
      ["projetos", [
        ["site", [["node_modules", 1.8 * GB], ["src", 0.05 * GB]]],
        ["app-mobile", [["node_modules", 2.4 * GB], ["android", [["build", 1.6 * GB]]]]],
        ["api", [["target", 5.2 * GB], ["src", 0.02 * GB]]],
      ]],
      [".gradle", [["caches", 4.7 * GB]]],
      [".nuget", [["packages", 2.2 * GB]]],
      ["OneDrive", 5.4 * GB],
      ["Documents", 2.7 * GB],
    ]],
    ["Public", 0.3 * GB],
  ]],
  ["Program Files", [
    ["Steam", [["steamapps", [["common", [["Jogo Grande", 68 * GB], ["Jogo Médio", 31 * GB], ["Indie", 4.2 * GB]]]]]]],
    ["Microsoft Office", 3.9 * GB],
    ["NVIDIA Corporation", [["Installer2", 2.6 * GB]]],
    ["Adobe", 5.6 * GB],
  ]],
  ["Program Files (x86)", [["Microsoft", 2.1 * GB], ["Common Files", 0.9 * GB]]],
  ["ProgramData", [["Package Cache", 3.7 * GB], ["Microsoft", 1.9 * GB]]],
  ["$Recycle.Bin", 8.4 * GB],
  ["Windows.old", 21 * GB],
  ["hiberfil.sys", 12.7 * GB],
  ["pagefile.sys", 16 * GB],
  ["swapfile.sys", 0.25 * GB],
];

class MockTree {
  nodes: MNode[] = [];
  top: FileInfo[] = [];
  rand = rng(42);

  constructor(public rootPath: string, spec: Spec[]) {
    const root = this.push(rootPath, -1, "dir");
    this.buildChildren(root, spec, 0);
    this.finish(root);
  }

  push(name: string, parent: number, kind: MNode["kind"], size = 0, fileCount = 0): number {
    const id = this.nodes.length;
    this.nodes.push({ id, name, parent, size, fileCount, dirCount: 0, kind, children: [], removed: false });
    if (parent >= 0) this.nodes[parent].children.push(id);
    return id;
  }

  buildChildren(parent: number, spec: Spec[], depth: number) {
    for (const [name, v] of spec) {
      if (Array.isArray(v)) {
        this.buildChildren(this.push(name, parent, "dir"), v, depth + 1);
      } else if (/\.(sys|iso|vhdx)$/i.test(name)) {
        this.push(name, parent, "file", v, 1);
      } else {
        this.fillLeafDir(this.push(name, parent, "dir"), v, depth + 1);
      }
    }
  }

  // Preenche uma pasta "folha" com subpastas e arquivos aleatórios até `budget` bytes.
  fillLeafDir(dir: number, budget: number, depth: number) {
    const r = this.rand;
    let left = budget;
    if (depth < 6 && budget > 200 * MB) {
      const n = 2 + Math.floor(r() * 5);
      for (let i = 0; i < n && left > 50 * MB; i++) {
        const part = left * (0.25 + r() * 0.4);
        this.fillLeafDir(this.push(`pasta-${depth}-${i + 1}`, dir, "dir"), part, depth + 1);
        left -= part;
      }
    }
    const files = 1 + Math.floor(r() * 6);
    for (let i = 0; i < files && left > MB; i++) {
      const sz = Math.round(left * (0.2 + r() * 0.5));
      const ext = [".bin", ".dat", ".mp4", ".zip", ".pak", ".log"][Math.floor(r() * 6)];
      this.push(`arquivo-${i + 1}${ext}`, dir, "file", sz, 1);
      left -= sz;
    }
    if (left > 0) this.push("", dir, "others", Math.round(left), 20 + Math.floor(r() * 3000));
  }

  finish(id: number): void {
    const n = this.nodes[id];
    if (n.kind !== "dir") return;
    let size = 0, files = 0, dirs = 0;
    for (const c of n.children) {
      this.finish(c);
      const cn = this.nodes[c];
      size += cn.size;
      files += cn.fileCount;
      dirs += cn.kind === "dir" ? cn.dirCount + 1 : 0;
    }
    Object.assign(n, { size, fileCount: files, dirCount: dirs });
    n.children.sort((a, b) => this.nodes[b].size - this.nodes[a].size);
    if (id === 0) {
      this.top = this.nodes
        .filter((x) => x.kind === "file")
        .sort((a, b) => b.size - a.size)
        .slice(0, 500)
        .map((x) => ({ path: this.pathOf(x.id), size: x.size }));
    }
  }

  pathOf(id: number): string {
    const parts: string[] = [];
    let cur = id;
    while (cur > 0) {
      const n = this.nodes[cur];
      if (n.kind !== "others") parts.push(n.name);
      cur = n.parent;
    }
    const root = this.rootPath.replace(/\\$/, "");
    return [root, ...parts.reverse()].join("\\") + (parts.length ? "" : "\\");
  }

  view(id: number, depth: number, max: number, minSize: number): NodeView {
    const n = this.nodes[id];
    const children: NodeView[] = [];
    if (depth > 0) {
      let shown = 0;
      while (shown < n.children.length && shown < Math.max(1, max) && this.nodes[n.children[shown]].size >= minSize) shown++;
      for (const c of n.children.slice(0, shown)) children.push(this.view(c, depth - 1, max, minSize));
      const rest = n.children.slice(shown).map((c) => this.nodes[c]);
      if (rest.length) {
        children.push({
          id: null, name: "", kind: "rest", hasChildren: false, error: false, children: [],
          size: rest.reduce((s, x) => s + x.size, 0),
          fileCount: rest.reduce((s, x) => s + x.fileCount, 0),
          dirCount: rest.reduce((s, x) => s + x.dirCount + (x.kind === "dir" ? 1 : 0), 0),
          hiddenItems: rest.length,
        });
      }
    }
    return {
      id, name: n.name, size: n.size, fileCount: n.fileCount, dirCount: n.dirCount, kind: n.kind,
      hasChildren: n.children.length > 0, error: false, hiddenItems: 0, children,
    };
  }

  remove(id: number): number {
    const n = this.nodes[id];
    const path = this.pathOf(id);
    n.removed = true;
    const p = this.nodes[n.parent];
    p.children = p.children.filter((c) => c !== id);
    let cur = n.parent;
    while (cur >= 0) {
      const a = this.nodes[cur];
      a.size -= n.size;
      a.fileCount -= n.fileCount;
      a.dirCount -= n.dirCount + (n.kind === "dir" ? 1 : 0);
      a.children.sort((x, y) => this.nodes[y].size - this.nodes[x].size);
      cur = a.parent;
    }
    // An "others" node has its folder's path: it must not take the folder's files with it.
    if (n.kind !== "others") this.top = this.top.filter((f) => f.path !== path && !f.path.startsWith(path + "\\"));
    return n.size;
  }

  stats(): Stats {
    const r = this.nodes[0];
    return {
      root: this.rootPath, totalSize: r.size, fileCount: r.fileCount, dirCount: r.dirCount,
      errors: 3, cancelled: false, elapsedMs: 4213,
    };
  }
}

// Copy (simulated mode only) of the catalogue in src-tauri/src/cleanup.rs.
type MockRule = Omit<CleanupItem, "size" | "fileCount">;
const U = "C:\\Users\\voce";
const L = `${U}\\AppData\\Local`;
const rule = (r: Partial<MockRule> & Pick<MockRule, "id" | "title" | "description" | "level" | "action" | "paths">): MockRule => ({
  minAgeHours: 0, needsAdmin: false, note: null, tool: null, commandId: null, command: null, ...r,
});
const MOCK_RULES: MockRule[] = [
  rule({ id: "userTemp", level: "safe", action: "clean", title: "Arquivos temporários do usuário",
    description: "Arquivos que instaladores e programas deixam para trás na pasta TEMP.", paths: [`${L}\\Temp`],
    minAgeHours: 24, note: "Arquivos modificados nas últimas 24 horas são mantidos (podem estar em uso)." }),
  rule({ id: "windowsTemp", level: "safe", action: "clean", title: "Arquivos temporários do Windows",
    description: "Temporários criados pelo sistema e por serviços.", paths: ["C:\\Windows\\Temp"], minAgeHours: 24, needsAdmin: true }),
  rule({ id: "updateCache", level: "safe", action: "clean", title: "Cache do Windows Update",
    description: "Atualizações já baixadas. O Windows baixa de novo se precisar.",
    paths: ["C:\\Windows\\SoftwareDistribution\\Download"], minAgeHours: 24, needsAdmin: true,
    note: "Evite durante uma atualização em andamento." }),
  rule({ id: "crashDumps", level: "safe", action: "clean", title: "Relatórios e dumps de erro",
    description: "Despejos de memória e relatórios de travamento de programas e do Windows.", paths: [`${L}\\CrashDumps`] }),
  rule({ id: "shaderCache", level: "safe", action: "clean", title: "Cache de shaders (placa de vídeo)",
    description: "Shaders compilados por jogos e pelo DirectX/OpenGL. São recriados automaticamente.",
    paths: [`${L}\\NVIDIA\\DXCache`], note: "Jogos podem demorar um pouco mais para abrir na primeira vez." }),
  rule({ id: "browserCache", level: "safe", action: "clean", title: "Cache dos navegadores",
    description: "Cópias de páginas e imagens do Chrome, Edge, Brave e Firefox. Não apaga senhas, histórico nem favoritos.",
    paths: [`${L}\\Google\\Chrome\\User Data\\Default\\Cache`], note: "Feche os navegadores antes; arquivos em uso são ignorados." }),
  rule({ id: "recycleBin", level: "safe", action: "recycleBin", title: "Lixeira",
    description: "Itens já excluídos. Esvaziar libera o espaço de verdade.", paths: [],
    note: "Não é possível recuperar os itens depois." }),
  rule({ id: "devCaches", level: "review", action: "clean", title: "Caches de ferramentas de programação",
    description: "Pacotes baixados por npm, Yarn, pip, NuGet, Gradle, Maven e Cargo.",
    paths: [`${L}\\npm-cache`, `${U}\\.nuget\\packages`, `${U}\\.gradle\\caches`],
    note: "Serão baixados de novo (exige internet) na próxima compilação dos projetos." }),
  rule({ id: "nvidiaInstaller", level: "review", action: "clean", title: "Instaladores antigos de driver NVIDIA",
    description: "Cópias dos instaladores de drivers de vídeo já instalados.",
    paths: ["C:\\Program Files\\NVIDIA Corporation\\Installer2"], needsAdmin: true }),
  rule({ id: "downloads", level: "review", action: "manual", title: "Pasta Downloads",
    description: "Costuma acumular instaladores, ISOs e arquivos esquecidos. Só você sabe o que ainda é útil.",
    paths: [`${U}\\Downloads`], note: "Revise e apague pela aba Explorar." }),
  rule({ id: "windowsOld", level: "review", action: "manual", title: "Instalação anterior do Windows (Windows.old)",
    description: "Sobra de uma atualização do Windows. Removê-la elimina a opção de voltar à versão anterior.",
    paths: ["C:\\Windows.old"], tool: "cleanmgr",
    note: "Na Limpeza de Disco, clique em \"Limpar arquivos do sistema\" e marque \"Instalações anteriores do Windows\"." }),
  rule({ id: "hibernation", level: "review", action: "manual", title: "Arquivo de hibernação (hiberfil.sys)",
    description: "Usado pela hibernação e pela Inicialização Rápida. Só desative se não usa esses recursos.",
    paths: ["C:\\hiberfil.sys"], commandId: "hibernateOff", command: "powercfg.exe /h off" }),
  rule({ id: "winsxs", level: "system", action: "manual", title: "Componentes do Windows (WinSxS)",
    description: "Bibliotecas usadas pelo sistema. Apagar à mão quebra o Windows. O DISM remove versões antigas com segurança.",
    paths: ["C:\\Windows\\WinSxS"], commandId: "dism", command: "Dism.exe /Online /Cleanup-Image /StartComponentCleanup",
    note: "O tamanho mostrado pelo Explorer é exagerado: muitos arquivos são links para outras pastas do sistema." }),
  rule({ id: "installer", level: "system", action: "manual", title: "Instaladores de programas (Windows\\Installer)",
    description: "O Windows usa estes arquivos para reparar, atualizar e desinstalar programas. Nunca apague.",
    paths: ["C:\\Windows\\Installer"] }),
  rule({ id: "pagefile", level: "system", action: "manual", title: "Memória virtual (pagefile.sys / swapfile.sys)",
    description: "Extensão da memória RAM no disco. Apagar não é possível; o tamanho pode ser ajustado.",
    paths: ["C:\\pagefile.sys", "C:\\swapfile.sys"], tool: "performance", note: "Em Desempenho > Avançado > Memória virtual." }),
  rule({ id: "programFiles", level: "system", action: "manual", title: "Programas instalados (Program Files)",
    description: "Apagar pastas de programas deixa restos no registro e pode quebrar outros programas.",
    paths: ["C:\\Program Files", "C:\\Program Files (x86)"], tool: "storageSense", note: "Desinstale em Configurações > Aplicativos." }),
];

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function createMockApi(): Api {
  let tree: MockTree | null = null;
  const progressCbs = new Set<(p: Progress) => void>();
  const finishedCbs = new Set<(f: ScanFinished) => void>();
  let cancelled = false;

  // The cleanup works on C:\ even before any scan (just like the real backend).
  let systemDrive: MockTree | null = null;
  const disk = () => (tree && tree.rootPath === "C:\\" ? tree : (systemDrive ??= new MockTree("C:\\", C_DRIVE)));
  const nodeAt = (t: MockTree, path: string) =>
    t.nodes.find((x) => !x.removed && x.kind !== "others" && t.pathOf(x.id).toLowerCase() === path.toLowerCase());
  const pathsOf = (r: MockRule) => (r.action === "recycleBin" ? ["C:\\$Recycle.Bin"] : r.paths);

  const need = () => {
    if (!tree) throw new Error("nenhuma varredura carregada");
    return tree;
  };

  return {
    isMock: true,
    async listDrives() {
      return [
        { mount: "C:\\", label: "Windows", fileSystem: "NTFS", total: 476 * GB, available: 38 * GB, removable: false, kind: "SSD" },
        { mount: "D:\\", label: "Dados", fileSystem: "NTFS", total: 931 * GB, available: 412 * GB, removable: false, kind: "HDD" },
        { mount: "E:\\", label: "Pendrive", fileSystem: "exFAT", total: 58 * GB, available: 21 * GB, removable: true, kind: "Unknown" },
      ];
    },
    async startScan({ path, nodeId }) {
      cancelled = false;
      const root = nodeId != null && tree ? tree.pathOf(nodeId) : (path ?? "C:\\");
      (async () => {
        const steps = nodeId != null ? 3 : 12;
        for (let i = 1; i <= steps && !cancelled; i++) {
          await delay(150);
          progressCbs.forEach((cb) =>
            cb({
              files: i * 48211, dirs: i * 6113, bytes: i * 31 * GB, errors: Math.floor(i / 4),
              current: `${root.replace(/\\$/, "")}\\Users\\voce\\AppData\\Local\\pasta-${i}`,
            }),
          );
        }
        if (nodeId == null || !tree) tree = new MockTree(root.endsWith("\\") ? root : root + "\\", C_DRIVE);
        finishedCbs.forEach((cb) =>
          cb({ ok: true, error: null, stats: tree!.stats(), rescannedPath: nodeId != null ? root : null, cancelled }),
        );
      })();
    },
    async cancelScan() {
      cancelled = true;
    },
    async getStats() {
      return need().stats();
    },
    async getNode(id, depth, maxChildren, minFraction = 0) {
      const t = need();
      const nid = id ?? 0;
      return t.view(nid, depth, maxChildren, t.nodes[nid].size * minFraction);
    },
    async getAncestors(id): Promise<Crumb[]> {
      const t = need();
      const out: Crumb[] = [];
      for (let cur = id; cur >= 0; cur = t.nodes[cur].parent) out.unshift({ id: cur, name: t.nodes[cur].name });
      return out;
    },
    async getPath(id) {
      return need().pathOf(id);
    },
    async getTopFiles(limit) {
      return need().top.slice(0, limit);
    },
    async findPath(path) {
      const t = need();
      const n = t.nodes.find((x) => !x.removed && x.kind !== "others" && t.pathOf(x.id) === path);
      return n ? n.id : null;
    },
    async reveal(path) {
      console.info("[mock] abrir no Explorer:", path);
    },
    async deleteNode(target) {
      const t = need();
      const id = "id" in target ? target.id : t.nodes.find((x) => !x.removed && x.kind === "file" && t.pathOf(x.id) === target.path)?.id;
      if (id == null) throw new Error("item não encontrado na varredura atual");
      if (id === 0) throw new Error("não é possível apagar a pasta raiz da varredura");
      if (isProtected(t.pathOf(id))) {
        throw new Error("itens dentro da pasta do Windows não podem ser apagados por aqui (use a Limpeza de Disco / DISM)");
      }
      await delay(300);
      const freed = t.remove(id);
      return { freed, stats: t.stats() };
    },
    async analyzeCleanup() {
      await delay(400);
      const t = disk();
      return MOCK_RULES.flatMap((r) => {
        const nodes = pathsOf(r).map((p) => nodeAt(t, p)).filter((n) => n != null);
        if (!nodes.length && r.action !== "recycleBin") return [];
        const measured = r.id !== "winsxs" && r.id !== "programFiles";
        return [{
          ...r,
          paths: r.action === "recycleBin" ? [] : nodes.map((n) => t.pathOf(n.id)),
          size: measured ? nodes.reduce((s, n) => s + n.size, 0) : null,
          fileCount: measured ? nodes.reduce((s, n) => s + n.fileCount, 0) : null,
        }];
      });
    },
    async runCleanup(ids) {
      await delay(700);
      const t = disk();
      const results: CleanupResult[] = ids.map((id) => {
        const r = MOCK_RULES.find((x) => x.id === id);
        const report = { freed: 0, deleted: 0, failed: 0, skippedRecent: 0 };
        if (!r) return { id, title: id, report, error: "item desconhecido" };
        if (r.level === "system" || r.action === "manual") {
          return { id, title: r.title, report, error: "este item não é limpo automaticamente" };
        }
        for (const p of pathsOf(r)) {
          const n = nodeAt(t, p);
          if (!n) continue;
          for (const c of [...n.children]) {
            report.deleted += t.nodes[c].fileCount;
            report.freed += t.remove(c);
          }
        }
        // Simulates files in use / too recent being kept.
        if (r.minAgeHours) report.skippedRecent = 12;
        if (r.id === "browserCache") report.failed = 3;
        return { id, title: r.title, report, error: null };
      });
      const total = results.reduce(
        (a, r) => ({ freed: a.freed + r.report.freed, deleted: a.deleted + r.report.deleted,
          failed: a.failed + r.report.failed, skippedRecent: a.skippedRecent + r.report.skippedRecent }),
        { freed: 0, deleted: 0, failed: 0, skippedRecent: 0 },
      );
      return { results, total, stats: tree ? tree.stats() : null };
    },
    async openSystemTool(tool) {
      console.info("[mock] abrir ferramenta:", tool);
    },
    async runElevated(commandId) {
      console.info("[mock] executar como administrador:", commandId);
    },
    async pickFolder() {
      return window.prompt("Caminho da pasta:", "D:\\Projetos");
    },
    async onProgress(cb) {
      progressCbs.add(cb);
      return () => progressCbs.delete(cb);
    },
    async onFinished(cb) {
      finishedCbs.add(cb);
      return () => finishedCbs.delete(cb);
    },
  };
}
