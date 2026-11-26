import { useEffect, useState } from "react";
import type { Api, Drive, Stats } from "../api";
import { formatBytes } from "../format";

/** Mesmas regras do backend: pastas do sistema e raízes não podem ser movidas. */
export function canMove(path: string): boolean {
  const p = path.replace(/[\\/]+$/, "");
  if (/^[a-z]:$/i.test(p)) return false;
  if (/^[a-z]:\\(windows|program files|program files \(x86\)|programdata|users|boot|recovery)$/i.test(p)) return false;
  if (/^[a-z]:\\windows\\/i.test(p)) return false;
  if (/^[a-z]:\\users\\[^\\]+$/i.test(p)) return false;
  if (/\\appdata(\\(local|roaming|locallow))?$/i.test(p)) return false;
  return true;
}

export interface MoveTarget {
  id: number;
  path: string;
  size: number;
}

export function MoveDialog({
  api,
  target,
  drives,
  onDone,
  onClose,
}: {
  api: Api;
  target: MoveTarget;
  drives: Drive[];
  onDone: (r: { bytes: number; to: string; leftover: string | null; stats: Stats }) => void;
  onClose: () => void;
}) {
  const [dest, setDest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<[number, number] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const srcDrive = target.path.slice(0, 2).toUpperCase();
  // Discos removíveis ficam de fora: o link quebra quando são desconectados.
  const others = drives.filter((d) => !d.removable && !d.mount.toUpperCase().startsWith(srcDrive) && d.available > target.size);

  useEffect(() => {
    let unsub: (() => void) | null = null;
    let disposed = false;
    api.onMoveProgress((c, t) => setProgress([c, t])).then((u) => (disposed ? u() : (unsub = u)));
    return () => {
      disposed = true;
      unsub?.();
    };
  }, [api]);

  const run = async () => {
    if (!dest) return;
    setBusy(true);
    setError(null);
    try {
      onDone(await api.moveFolder(target.id, dest));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const pct = progress && progress[1] ? (progress[0] / progress[1]) * 100 : 0;

  return (
    <div className="overlay" onClick={() => !busy && onClose()}>
      <div className="dialog" role="dialog" aria-modal onClick={(e) => e.stopPropagation()}>
        <h3>Mover para outro disco</h3>
        <code className="path-box">{target.path}</code>
        <p>
          A pasta ({formatBytes(target.size)}) é copiada para o destino e, no lugar dela, fica um <strong>link</strong> que
          aponta para a nova posição. Programas e jogos continuam achando os arquivos, e o espaço é liberado em {srcDrive}.
        </p>
        <p className="muted small">
          Feche os programas que usam a pasta antes. Se algo falhar no meio, nada é alterado. O disco de destino precisa
          ficar sempre conectado.
        </p>
        <div className="dup-controls">
          {others.map((d) => (
            <button key={d.mount} className={`btn ${dest === d.mount ? "primary" : ""}`} disabled={busy} onClick={() => setDest(d.mount)}>
              {d.mount.replace(/\\$/, "")} {d.label} · {formatBytes(d.available)} livres
            </button>
          ))}
          <button
            className="btn"
            disabled={busy}
            onClick={async () => {
              const p = await api.pickFolder();
              if (p) setDest(p);
            }}
          >
            Outra pasta…
          </button>
        </div>
        {dest && (
          <p className="small">
            Destino: <code>{dest}</code>
          </p>
        )}
        {busy && (
          <div className="repair-progress">
            <div className="bar">
              <div className="bar-fill" style={{ width: `${pct}%` }} />
            </div>
            <span className="muted small">
              {progress ? `${formatBytes(progress[0])} de ${formatBytes(progress[1])} copiados` : "Preparando…"}
            </span>
          </div>
        )}
        {error && <p className="danger-text small">{error}</p>}
        <div className="dialog-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancelar
          </button>
          <button className="btn primary" onClick={run} disabled={!dest || busy}>
            {busy ? "Movendo…" : "Mover"}
          </button>
        </div>
      </div>
    </div>
  );
}
