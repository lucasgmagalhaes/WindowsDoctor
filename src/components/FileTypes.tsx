import { useMemo, useState } from "react";
import type { ExtStat } from "../api";
import { CATEGORIES, OTHER, categoryOf } from "../fileTypes";
import { formatBytes, formatCount, formatPercent } from "../format";

export function FileTypes({ exts, onPickExt }: { exts: ExtStat[]; onPickExt: (ext: string) => void }) {
  const [open, setOpen] = useState<string | null>(null);
  const total = exts.reduce((a, e) => a + e.size, 0);
  const groups = useMemo(() => {
    const all = [...CATEGORIES, OTHER];
    return all
      .map((c) => {
        const items = exts.filter((e) => categoryOf(e.ext) === c.id);
        return { ...c, items, size: items.reduce((a, e) => a + e.size, 0), count: items.reduce((a, e) => a + e.count, 0) };
      })
      .filter((g) => g.size > 0)
      .sort((a, b) => b.size - a.size);
  }, [exts]);

  if (!exts.length) return <div className="empty">Sem dados de tipos: faça uma varredura completa.</div>;
  const max = groups[0]?.size || 1;

  return (
    <div className="types">
      <div className="types-bar" aria-hidden>
        {groups.map((g) => (
          <span key={g.id} style={{ width: `${(g.size / total) * 100}%`, background: g.color }} title={g.label} />
        ))}
      </div>
      <ul className="types-list">
        {groups.map((g) => (
          <li key={g.id}>
            <button className="types-row" onClick={() => setOpen(open === g.id ? null : g.id)} aria-expanded={open === g.id}>
              <span className="types-dot" style={{ background: g.color }} />
              <span className="types-name">{g.label}</span>
              <span className="bar thin types-mini">
                <span className="bar-fill" style={{ width: `${(g.size / max) * 100}%`, background: g.color, display: "block" }} />
              </span>
              <span className="muted small">{formatCount(g.count)} arq.</span>
              <span className="muted small types-pct">{formatPercent(g.size, total)}</span>
              <strong className="types-size">{formatBytes(g.size)}</strong>
            </button>
            {open === g.id && (
              <ul className="types-exts">
                {g.items.slice(0, 30).map((e) => (
                  <li key={e.ext}>
                    <button className="link-btn" onClick={() => e.ext && onPickExt(e.ext)} disabled={!e.ext} title="Ver os maiores arquivos deste tipo">
                      {e.ext ? `.${e.ext}` : "(sem extensão)"}
                    </button>
                    <span className="muted small">{formatCount(e.count)} arq.</span>
                    <strong>{formatBytes(e.size)}</strong>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      <p className="muted small">
        Inclui os arquivos pequenos agrupados no mapa. Os números são da última varredura completa e não mudam com exclusões
        feitas depois. Clique numa extensão para ver os maiores arquivos dela.
      </p>
    </div>
  );
}
