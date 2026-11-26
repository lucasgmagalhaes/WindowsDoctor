import type { Comparison } from "../api";
import { formatBytes } from "../format";

const signed = (n: number) => `${n >= 0 ? "+" : "−"}${formatBytes(Math.abs(n))}`;

export function Changes({ data, onOpen }: { data: Comparison | null; onOpen: (path: string) => void }) {
  if (!data) {
    return (
      <div className="empty">
        Esta é a primeira varredura deste local. Na próxima, esta aba mostra o que cresceu e o que diminuiu desde hoje.
      </div>
    );
  }
  const diff = data.currentTotal - data.previousTotal;
  const when = new Date(data.previousTimeMs).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });
  const max = Math.max(1, ...data.changes.map((c) => Math.abs(c.delta)));
  return (
    <div className="changes">
      <p>
        Desde a varredura de <strong>{when}</strong>, o total foi de {formatBytes(data.previousTotal)} para{" "}
        {formatBytes(data.currentTotal)} (<strong className={diff > 0 ? "danger-text" : "ok-text"}>{signed(diff)}</strong>).
      </p>
      {data.changes.length === 0 ? (
        <div className="empty">Nenhuma pasta mudou mais de 200 MB.</div>
      ) : (
        <table className="top-files">
          <thead>
            <tr>
              <th>Pasta</th>
              <th className="num">Antes</th>
              <th className="num">Agora</th>
              <th className="num">Diferença</th>
            </tr>
          </thead>
          <tbody>
            {data.changes.map((c) => (
              <tr key={c.path}>
                <td className="tf-name">
                  <button className="link-btn changes-path" onClick={() => onOpen(c.path)} title="Abrir no mapa">
                    {c.path}
                  </button>
                  {c.before === 0 && <span className="badge review">nova</span>}
                  {c.after === 0 && <span className="badge safe">sumiu</span>}
                  <div className="bar thin">
                    <div className="bar-fill" style={{ width: `${(Math.abs(c.delta) / max) * 100}%`, background: c.delta > 0 ? "var(--crit)" : "var(--ok)" }} />
                  </div>
                </td>
                <td className="num muted">{formatBytes(c.before)}</td>
                <td className="num">{formatBytes(c.after)}</td>
                <td className={`num ${c.delta > 0 ? "danger-text" : "ok-text"}`}>
                  <strong>{signed(c.delta)}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="muted small">
        Mostra a pasta mais específica que explica cada mudança de pelo menos 200 MB. A comparação é com a varredura anterior
        do mesmo local.
      </p>
    </div>
  );
}
