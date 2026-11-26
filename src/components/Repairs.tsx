import { useEffect, useState } from "react";
import type { Api, RepairKind, RepairOutcome } from "../api";
import type { Notify } from "../App";

const REPAIRS: { kind: RepairKind; title: string; detail: string; admin: boolean }[] = [
  {
    kind: "sfc",
    title: "Verificar arquivos do sistema (SFC)",
    detail: "Procura e restaura arquivos do Windows corrompidos. Leva de 5 a 15 minutos.",
    admin: true,
  },
  {
    kind: "dism",
    title: "Reparar a imagem do Windows (DISM)",
    detail: "Conserta a cópia de reparo que o SFC usa, baixando do Windows Update. Rode antes do SFC se ele falhar. Leva até 30 minutos.",
    admin: true,
  },
  {
    kind: "chkdsk",
    title: "Verificar o disco do sistema (CHKDSK)",
    detail: "Procura erros no sistema de arquivos sem precisar reiniciar (/scan).",
    admin: true,
  },
  {
    kind: "memory",
    title: "Teste de memória",
    detail: "Abre o Diagnóstico de Memória do Windows, que testa a RAM no próximo reinício (cerca de 20 minutos).",
    admin: false,
  },
];

export function Repairs({ api, notify, elevated }: { api: Api; notify: Notify; elevated: boolean }) {
  const [running, setRunning] = useState<RepairKind | null>(null);
  const [progress, setProgress] = useState<{ line: string; percent: number | null } | null>(null);
  const [results, setResults] = useState<Partial<Record<RepairKind, RepairOutcome>>>({});

  useEffect(() => {
    let unsub: (() => void) | null = null;
    let disposed = false;
    api.onRepairProgress((p) => setProgress((old) => ({ line: p.line, percent: p.percent ?? old?.percent ?? null }))).then((u) =>
      disposed ? u() : (unsub = u),
    );
    return () => {
      disposed = true;
      unsub?.();
    };
  }, [api]);

  const run = async (kind: RepairKind) => {
    setRunning(kind);
    setProgress(null);
    try {
      const r = await api.runRepair(kind);
      setResults((x) => ({ ...x, [kind]: r }));
      notify(`${r.title}: ${r.ok ? "concluído" : "terminou com erro"}.`, r.ok ? "ok" : "err");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setRunning(null);
    }
  };

  return (
    <ul className="opt-list">
      {REPAIRS.map((r) => {
        const res = results[r.kind];
        return (
          <li key={r.kind} className="opt-item repair-item">
            <div className="opt-main">
              <div className="opt-title">
                <strong>{r.title}</strong>
                {r.admin && <span className="badge review">admin</span>}
              </div>
              <p className="muted small">{r.detail}</p>
              {running === r.kind && (
                <div className="repair-progress">
                  <div className="bar">
                    <div className="bar-fill" style={{ width: `${progress?.percent ?? 2}%` }} />
                  </div>
                  <code className="small">{progress?.line ?? "Iniciando…"}</code>
                </div>
              )}
              {res && (
                <div className={`small ${res.ok ? "ok-text" : "danger-text"}`}>
                  {res.ok ? "Concluído" : `Terminou com erro (código ${res.exitCode ?? "?"})`}
                  {res.tail.length > 0 && (
                    <ul className="details">
                      {res.tail.map((l, i) => (
                        <li key={i} className="muted">
                          {l}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>
            <button className="btn" disabled={running != null || (r.admin && !elevated && !api.isMock)} onClick={() => run(r.kind)}>
              {running === r.kind ? "Rodando…" : "Executar"}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
