import type { FileInfo } from "../api";
import { formatBytes } from "../format";
import { LEVEL_INFO, hintFor, isProtected } from "../hints";

interface Props {
  files: FileInfo[];
  onReveal: (path: string, select: boolean) => void;
  onDelete: (f: FileInfo) => void;
}

export function TopFiles({ files, onReveal, onDelete }: Props) {
  if (files.length === 0) return <div className="empty">Nenhum arquivo.</div>;
  const max = files[0].size || 1;
  return (
    <table className="top-files">
      <thead>
        <tr>
          <th className="num">#</th>
          <th>Arquivo</th>
          <th className="num">Tamanho</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {files.map((f, i) => {
          const sep = Math.max(f.path.lastIndexOf("\\"), f.path.lastIndexOf("/"));
          const name = f.path.slice(sep + 1);
          const dir = f.path.slice(0, sep).replace(/^([A-Za-z]:)$/, "$1\\");
          const hint = hintFor(f.path);
          return (
            <tr key={f.path}>
              <td className="num muted">{i + 1}</td>
              <td className="tf-name">
                <div>
                  <strong>{name}</strong>
                  {hint && (
                    <span className={`badge ${hint.level}`} title={`${LEVEL_INFO[hint.level].label}: ${hint.detail}`}>
                      <span aria-hidden>{LEVEL_INFO[hint.level].icon} </span>
                      {hint.title}
                    </span>
                  )}
                </div>
                <div className="muted tf-dir" title={dir}>
                  {dir}
                </div>
                <div className="bar thin">
                  <div className="bar-fill" style={{ width: `${(f.size / max) * 100}%` }} />
                </div>
              </td>
              <td className="num">{formatBytes(f.size)}</td>
              <td className="tf-actions">
                <button className="icon-btn" title="Mostrar no Explorer" onClick={() => onReveal(f.path, true)}>
                  ↗
                </button>
                <button
                  className="icon-btn danger"
                  title={isProtected(f.path) || hint?.level === "system" ? "Não apague manualmente: veja a aba Limpeza" : "Excluir"}
                  disabled={isProtected(f.path) || !!hint && hint.level === "system"}
                  onClick={() => onDelete(f)}
                >
                  🗑
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
