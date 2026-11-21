import type { NodeView } from "../api";
import { PALETTE, NEUTRAL, isRepeatClick } from "../chartUtils";
import { formatBytes, formatCount, formatPercent, joinPath, nodeLabel } from "../format";
import { hintFor, isProtected } from "../hints";

interface Props {
  items: NodeView[];
  parentPath: string;
  parentSize: number;
  highlightId: number | null;
  onOpen: (id: number) => void;
  onReveal: (path: string, select: boolean) => void;
  onDelete: (item: { id: number; path: string; size: number; isDir: boolean }) => void;
  onRescan: (id: number) => void;
}

export function ItemList({ items, parentPath, parentSize, highlightId, onOpen, onReveal, onDelete, onRescan }: Props) {
  if (items.length === 0) return <div className="empty">Pasta vazia.</div>;
  return (
    <ul className="items">
      {items.map((it, i) => {
        const real = it.kind === "dir" || it.kind === "file";
        const path = real ? joinPath(parentPath, it.name) : parentPath;
        const hint = real ? hintFor(path) : null;
        const canOpen = it.kind === "dir" && it.id != null && it.hasChildren;
        const pct = parentSize > 0 ? (it.size / parentSize) * 100 : 0;
        const color = real ? PALETTE[i % PALETTE.length] : NEUTRAL;
        return (
          <li
            key={it.id ?? `rest-${i}`}
            className={`item ${canOpen ? "clickable" : ""} ${highlightId != null && it.id === highlightId ? "hl" : ""}`}
            onClick={(e) => canOpen && !isRepeatClick(e) && onOpen(it.id!)}
          >
            <div className="item-main">
              <span className="item-icon" aria-hidden>
                {it.kind === "dir" ? "📁" : it.kind === "file" ? "📄" : "⋯"}
              </span>
              <span className={`item-name ${real ? "" : "muted"}`} title={real ? path : undefined}>
                {nodeLabel(it)}
              </span>
              {it.error && <span className="badge warn" title="Sem permissão para ler parte desta pasta">sem acesso</span>}
              {hint && (
                <span className={`badge ${hint.level}`} title={hint.detail}>
                  {hint.title}
                </span>
              )}
              <span className="item-size">{formatBytes(it.size)}</span>
            </div>
            <div className="item-sub">
              <div className="bar">
                <div className="bar-fill" style={{ width: `${Math.max(pct, 0.5)}%`, background: color }} />
              </div>
              <span className="item-pct">{formatPercent(it.size, parentSize)}</span>
              <span className="item-meta">
                {it.kind === "dir" && `${formatCount(it.fileCount)} arq · ${formatCount(it.dirCount)} pastas`}
                {it.kind === "others" && "agrupados (abaixo do limite)"}
              </span>
              {real && it.id != null && (
                <span className="item-actions" onClick={(e) => e.stopPropagation()}>
                  <button className="icon-btn" title="Mostrar no Explorer" onClick={() => onReveal(path, true)}>
                    ↗
                  </button>
                  {it.kind === "dir" && (
                    <button className="icon-btn" title="Analisar esta pasta de novo" onClick={() => onRescan(it.id!)}>
                      ⟳
                    </button>
                  )}
                  <button
                    className="icon-btn danger"
                    title={isProtected(path) ? "Protegido: pasta do sistema" : "Excluir"}
                    disabled={isProtected(path)}
                    onClick={() => onDelete({ id: it.id!, path, size: it.size, isDir: it.kind === "dir" })}
                  >
                    🗑
                  </button>
                </span>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
