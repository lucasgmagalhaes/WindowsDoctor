import type { NodeView } from "./api";

const UNITS = ["B", "KB", "MB", "GB", "TB"];
const nf1 = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1, minimumFractionDigits: 1 });
const nf0 = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 0 });

export function formatBytes(n: number): string {
  if (n < 1024) return `${nf0.format(n)} B`;
  let i = 0;
  let v = n;
  // Compare the *rounded* value: 1023.96 KB would otherwise show as "1.024 KB".
  const rounded = (x: number) => (x >= 99.95 ? Math.round(x) : Math.round(x * 10) / 10);
  while (rounded(v) >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${rounded(v) >= 100 ? nf0.format(v) : nf1.format(v)} ${UNITS[i]}`;
}

export function formatCount(n: number): string {
  return nf0.format(n);
}

export function formatPercent(part: number, total: number): string {
  if (total <= 0) return "0%";
  const p = (part / total) * 100;
  if (p > 0 && p < 0.1) return "<0,1%";
  return `${nf1.format(p)}%`;
}

export function nodeLabel(n: NodeView): string {
  switch (n.kind) {
    case "others":
      return `${formatCount(n.fileCount)} arquivos pequenos`;
    case "rest":
      return `${formatCount(n.hiddenItems)} itens menores`;
    default:
      return n.name;
  }
}

export function joinPath(dir: string, name: string): string {
  return dir.endsWith("\\") || dir.endsWith("/") ? dir + name : `${dir}${dir.includes("/") ? "/" : "\\"}${name}`;
}
