import { useEffect, useState } from "react";
import type { Api, ShadowInfo } from "../api";
import type { Notify } from "../App";
import { formatBytes } from "../format";
import { Repairs } from "./Repairs";
import { Loading } from "./ToolLayout";

const LIMITS = [3, 5, 10, 15];

export function SystemSpace({ api, notify, elevated }: { api: Api; notify: Notify; elevated: boolean }) {
  const [info, setInfo] = useState<ShadowInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.shadowInfo().then(setInfo).catch((e) => setError(String(e)));
  }, [api]);

  const act = async (f: () => Promise<ShadowInfo>, done: string) => {
    setBusy(true);
    try {
      setInfo(await f());
      notify(done);
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(false);
    }
  };

  const pct = info && info.driveSize ? Math.round((info.max / info.driveSize) * 100) : null;

  return (
    <div className="tab-page">
      <section className="card">
        <h3>Pontos de restauração</h3>
        {error ? (
          <p className="warn-text small">{error}</p>
        ) : !info ? (
          <Loading text="Lendo os pontos de restauração…" />
        ) : (
          <>
            <p>
              Usam <strong>{formatBytes(info.used)}</strong> em {info.drive}
              {info.max > 0 && (
                <span className="muted">
                  {" "}
                  de um limite de {formatBytes(info.max)}
                  {pct != null && ` (${pct}% do disco)`}
                </span>
              )}
              . {info.copies} cópia(s) de sombra.
            </p>
            {info.restorePoints.length > 0 && (
              <ul className="details small">
                {info.restorePoints.map((r) => (
                  <li key={r.sequence}>
                    <code>{r.time}</code> {r.description}
                  </li>
                ))}
              </ul>
            )}
            {!elevated && <p className="muted small">A lista completa e as ações precisam do app aberto como administrador.</p>}
            <div className="dup-controls" style={{ marginTop: 10 }}>
              <button
                className="btn danger"
                disabled={busy || info.copies < 2 || (!elevated && !api.isMock)}
                onClick={() =>
                  window.confirm("Apagar todos os pontos de restauração menos o mais recente? Não dá para desfazer.") &&
                  act(() => api.deleteOldShadows(), "Pontos antigos apagados; o mais recente foi mantido.")
                }
              >
                Apagar todos menos o mais recente
              </button>
              <label className="inline-field">
                Limite de espaço
                <select
                  className="select"
                  disabled={busy || (!elevated && !api.isMock)}
                  value={pct != null && LIMITS.includes(pct) ? pct : ""}
                  onChange={(e) => act(() => api.setShadowLimit(Number(e.target.value)), "Limite alterado.")}
                >
                  <option value="" disabled>
                    {pct != null ? `${pct}%` : "—"}
                  </option>
                  {LIMITS.map((l) => (
                    <option key={l} value={l}>
                      {l}% do disco
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </>
        )}
      </section>
      <Repairs api={api} notify={notify} elevated={elevated} kinds={["compactos", "uncompactos"]} />
    </div>
  );
}
