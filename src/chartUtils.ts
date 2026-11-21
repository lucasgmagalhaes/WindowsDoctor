import { useEffect, useState, type RefObject } from "react";
import type { HierarchyNode } from "d3-hierarchy";
import type { NodeView } from "./api";

// Paleta categórica; cada filho de primeiro nível ganha uma cor e seus
// descendentes herdam a mesma cor, mais transparente conforme a profundidade.
export const PALETTE = [
  "#4f8cff", "#ff7a59", "#3ccf91", "#f5c542", "#b67cff",
  "#ff5fa2", "#2ec5d3", "#a3d65c", "#ff9f40", "#8c9eff",
];
export const NEUTRAL = "#8a8f9c";

export function colorOf(n: HierarchyNode<NodeView>): string {
  if (n.data.kind === "others" || n.data.kind === "rest") return NEUTRAL;
  let top = n;
  while (top.depth > 1 && top.parent) top = top.parent;
  const idx = top.parent ? top.parent.children!.indexOf(top) : 0;
  return PALETTE[idx % PALETTE.length];
}

export function opacityOf(n: HierarchyNode<NodeView>): number {
  return Math.max(0.35, 1 - (n.depth - 1) * 0.2);
}

/** Copia a visão limitando a profundidade. */
export function truncate(v: NodeView, depth: number): NodeView {
  return { ...v, children: depth > 0 ? v.children.map((c) => truncate(c, depth - 1)) : [] };
}

/** Atribui `value` = tamanho de cada nó (os tamanhos já vêm agregados do backend). */
export function setValues(h: HierarchyNode<NodeView>) {
  h.each((n) => {
    (n as { value?: number }).value = n.data.size;
  });
  return h;
}

/** Nó de destino ao clicar: a própria pasta, ou a pasta mais próxima acima dela. */
export function navTarget(n: HierarchyNode<NodeView>): number | null {
  let cur: HierarchyNode<NodeView> | null = n;
  while (cur && cur.depth > 0) {
    if (cur.data.kind === "dir" && cur.data.id != null && cur.data.hasChildren) return cur.data.id;
    cur = cur.parent;
  }
  return null;
}

export function useSize(ref: RefObject<HTMLElement | null>) {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const { width, height } = e.contentRect;
      setSize({ width: Math.floor(width), height: Math.floor(height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return size;
}
