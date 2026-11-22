// Backend simulado usado quando a UI roda no navegador fora do Tauri.
// Reproduz a mesma semântica do crate `diskscan` (agregação Rest/Others,
// remoção com propagação de tamanhos etc.) sobre uma partição fictícia.

import { isProtected } from "./hints";
import { createMockTools } from "./mockTools";
import type { Api, Crumb, FileInfo, NodeKind, NodeView, Progress, ScanFinished, Stats } from "./api";

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
    this.top = this.top.filter((f) => f.path !== path && !f.path.startsWith(path + "\\"));
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

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function createMockApi(): Api {
  let tree: MockTree | null = null;
  const progressCbs = new Set<(p: Progress) => void>();
  const finishedCbs = new Set<(f: ScanFinished) => void>();
  let cancelled = false;

  const need = () => {
    if (!tree) throw new Error("nenhuma varredura carregada");
    return tree;
  };

  return {
    isMock: true,
    ...createMockTools(),
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
