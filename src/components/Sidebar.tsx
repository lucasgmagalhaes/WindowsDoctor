import type { Drive } from "../api";
import { formatBytes, formatPercent } from "../format";

interface Props {
  drives: Drive[];
  activeRoot: string | null;
  busy: boolean;
  minFileSize: number;
  onMinFileSize: (v: number) => void;
  onScan: (path: string) => void;
  onPickFolder: () => void;
  onRefreshDrives: () => void;
  onCleanup: () => void;
}

const MIN_SIZES = [
  { v: 0, label: "Mostrar todos" },
  { v: 100 * 1024, label: "100 KB" },
  { v: 1024 * 1024, label: "1 MB" },
  { v: 10 * 1024 * 1024, label: "10 MB" },
];

export function Sidebar({ drives, activeRoot, busy, minFileSize, onMinFileSize, onScan, onPickFolder, onRefreshDrives, onCleanup }: Props) {
  return (
    <aside className="sidebar">
      <div className="side-head">
        <h2>Partições</h2>
        <button className="icon-btn" title="Atualizar lista" onClick={onRefreshDrives}>
          ⟳
        </button>
      </div>
      <ul className="drives">
        {drives.map((d) => {
          const used = d.total - d.available;
          const pct = d.total ? used / d.total : 0;
          const level = pct > 0.9 ? "crit" : pct > 0.75 ? "warn" : "ok";
          return (
            <li key={d.mount} className={`drive ${activeRoot === d.mount ? "active" : ""}`}>
              <div className="drive-top">
                <span className="drive-mount">{d.mount.replace(/\\$/, "")}</span>
                <span className="drive-label">{d.label || "Disco local"}</span>
                <span className="muted small">{d.fileSystem}{d.removable ? " · removível" : ""}</span>
              </div>
              <div className={`bar drive-bar ${level}`}>
                <div className="bar-fill" style={{ width: `${pct * 100}%` }} />
              </div>
              <div className="drive-stats small">
                <span>
                  <strong>{formatBytes(d.available)}</strong> livres de {formatBytes(d.total)}
                </span>
                <span className="muted">{formatPercent(used, d.total)} usado</span>
              </div>
              <button className="btn primary block" disabled={busy} onClick={() => onScan(d.mount)}>
                Analisar {d.mount.replace(/\\$/, "")}
              </button>
            </li>
          );
        })}
      </ul>
      <button className="btn block" disabled={busy} onClick={onPickFolder}>
        Analisar uma pasta…
      </button>
      <button
        className="btn block"
        disabled={busy}
        onClick={onCleanup}
        title="Apaga caches, temporários e restos de instalação do Windows e dos aplicativos"
      >
        🧹 Limpeza automática…
      </button>
      <label className="field">
        <span>Agrupar arquivos menores que</span>
        <select value={minFileSize} onChange={(e) => onMinFileSize(Number(e.target.value))} disabled={busy}>
          {MIN_SIZES.map((o) => (
            <option key={o.v} value={o.v}>
              {o.label}
            </option>
          ))}
        </select>
        <small className="muted">Arquivos pequenos de cada pasta viram um único bloco. Reduz memória em partições grandes.</small>
      </label>
      <p className="muted small tip">
        Dica: execute o app como <strong>administrador</strong> para incluir pastas protegidas (outros usuários,
        System Volume Information etc.).
      </p>
    </aside>
  );
}
