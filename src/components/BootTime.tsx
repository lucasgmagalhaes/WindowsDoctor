import { useEffect, useState } from "react";
import type { Api, BootReport } from "../api";
import { Loading } from "./ToolLayout";

const KIND: Record<string, string> = { app: "Programa", driver: "Driver", service: "Serviço" };
const sec = (ms: number) => `${(ms / 1000).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} s`;

export function BootTime({ api, elevated }: { api: Api; elevated: boolean }) {
  const [report, setReport] = useState<BootReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!elevated && !api.isMock) {
      setError("O log de inicialização só pode ser lido com o app aberto como administrador.");
      return;
    }
    api.bootReport().then(setReport).catch((e) => setError(String(e)));
  }, [api, elevated]);

  if (error) return <p className="muted small">{error}</p>;
  if (!report) return <Loading text="Lendo o log de inicialização…" />;
  const last = report.boots[0];
  return (
    <div className="card">
      <dl>
        <dt>Último boot</dt>
        <dd>
          {last ? (
            <>
              <strong>{sec(last.bootMs)}</strong>{" "}
              <span className="muted small">
                ({last.time}: {sec(last.mainPathMs)} até a área de trabalho + {sec(last.postBootMs)} até ficar utilizável)
              </span>
            </>
          ) : (
            "sem registros"
          )}
        </dd>
        {report.avgMs != null && (
          <>
            <dt>Média dos últimos {report.boots.length}</dt>
            <dd>{sec(report.avgMs)}</dd>
          </>
        )}
      </dl>
      {report.slow.length > 0 && (
        <>
          <h3 style={{ marginTop: 14 }}>O que o Windows marcou como lento</h3>
          <ul className="details small">
            {report.slow.map((s) => (
              <li key={`${s.kind}:${s.name}`}>
                <span className="badge review">{KIND[s.kind] ?? s.kind}</span> <strong>{s.name}</strong>{" "}
                <span className="muted">
                  atrasou {sec(s.avgMs)} em média{s.times > 1 ? `, ${s.times} vezes` : ""}
                </span>
              </li>
            ))}
          </ul>
          <p className="muted small">Programas da lista podem ser desligados em "Programas na inicialização", abaixo.</p>
        </>
      )}
    </div>
  );
}
