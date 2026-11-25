import { useEffect, useState } from "react";
import type { Progress } from "../api";
import { formatBytes, formatCount } from "../format";
import { LEVEL_INFO, hintFor } from "../hints";

export function ScanOverlay({ progress, target, onCancel }: { progress: Progress | null; target: string; onCancel: () => void }) {
  return (
    <div className="overlay">
      <div className="dialog scan-dialog" role="status" aria-live="polite">
        <h3>Analisando {target}</h3>
        <div className="spinner" aria-hidden />
        <div className="scan-numbers">
          <div>
            <strong>{formatBytes(progress?.bytes ?? 0)}</strong>
            <span>encontrados</span>
          </div>
          <div>
            <strong>{formatCount(progress?.files ?? 0)}</strong>
            <span>arquivos</span>
          </div>
          <div>
            <strong>{formatCount(progress?.dirs ?? 0)}</strong>
            <span>pastas</span>
          </div>
          <div>
            <strong>{formatCount(progress?.errors ?? 0)}</strong>
            <span>sem acesso</span>
          </div>
        </div>
        <div className="scan-current muted" title={progress?.current}>
          {progress?.current || "iniciando…"}
        </div>
        <button className="btn" onClick={onCancel}>
          Cancelar
        </button>
      </div>
    </div>
  );
}

export interface DeleteTarget {
  /** null for files from the top list that may have no node of their own. */
  id: number | null;
  path: string;
  size: number;
  isDir: boolean;
}

export function DeleteDialog({
  target,
  onConfirm,
  onClose,
}: {
  target: DeleteTarget;
  onConfirm: (permanent: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const [permanent, setPermanent] = useState(false);
  const [busy, setBusy] = useState(false);
  const hint = hintFor(target.path);

  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [busy, onClose]);

  return (
    <div className="overlay" onClick={() => !busy && onClose()}>
      <div className="dialog" role="dialog" aria-modal onClick={(e) => e.stopPropagation()}>
        <h3>Excluir {target.isDir ? "pasta" : "arquivo"}?</h3>
        <code className="path-box">{target.path}</code>
        <p>
          Isso vai liberar <strong>{formatBytes(target.size)}</strong>
          {!permanent && " depois que a Lixeira for esvaziada"}.
        </p>
        {hint && (
          <p className={`hint-box ${hint.level}`}>
            <strong>
              {LEVEL_INFO[hint.level].icon} {LEVEL_INFO[hint.level].label} — {hint.title}:
            </strong>{" "}
            {hint.detail}
          </p>
        )}
        <label className="check">
          <input type="checkbox" checked={permanent} onChange={(e) => setPermanent(e.target.checked)} />
          Excluir permanentemente (não enviar para a Lixeira)
        </label>
        {permanent && <p className="danger-text">Esta ação não pode ser desfeita.</p>}
        <div className="dialog-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancelar
          </button>
          <button
            className="btn danger"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await onConfirm(permanent);
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Excluindo…" : permanent ? "Excluir permanentemente" : "Mover para a Lixeira"}
          </button>
        </div>
      </div>
    </div>
  );
}
