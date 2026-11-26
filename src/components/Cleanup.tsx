import { useEffect, useMemo, useState } from "react";
import type { Api, CleanupCategory, CleanupProgress, CleanupReport, CleanupRunResult } from "../api";
import { formatBytes, formatCount } from "../format";
import { DO_NOT_DELETE } from "../doNotDelete";
import { beforeChange } from "../safety";

type Phase =
  | { kind: "analyzing" }
  | { kind: "ready"; report: CleanupReport }
  | { kind: "running"; report: CleanupReport; progress: CleanupProgress | null }
  | { kind: "done"; report: CleanupReport; run: CleanupRunResult }
  | { kind: "error"; message: string };

const usable = (c: CleanupCategory, isAdmin: boolean) => isAdmin || !c.needsAdmin;
const hasContent = (c: CleanupCategory) => c.size == null || c.size > 0;

/** Safe categories with something to clean start selected; "review" ones never do. */
function defaultSelection(r: CleanupReport): Set<string> {
  return new Set(r.categories.filter((c) => c.level === "safe" && hasContent(c) && usable(c, r.isAdmin)).map((c) => c.id));
}

export function CleanupDialog({
  api,
  onClose,
  onFinished,
}: {
  api: Api;
  onClose: () => void;
  onFinished: (run: CleanupRunResult) => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "analyzing" });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<string | null>(null);

  const analyze = async () => {
    setPhase({ kind: "analyzing" });
    try {
      const report = await api.cleanupAnalyze();
      setSelected(defaultSelection(report));
      setPhase({ kind: "ready", report });
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  useEffect(() => {
    analyze();
    // Runs once when the dialog opens.
  }, []);

  useEffect(() => {
    let unsub: (() => void) | null = null;
    let disposed = false;
    api.onCleanupProgress((p) => setPhase((ph) => (ph.kind === "running" ? { ...ph, progress: p } : ph))).then((u) =>
      disposed ? u() : (unsub = u),
    );
    return () => {
      disposed = true;
      unsub?.();
    };
  }, [api]);

  const busy = phase.kind === "analyzing" || phase.kind === "running";
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [busy, onClose]);

  const report = "report" in phase ? phase.report : null;
  const groups = useMemo(() => {
    const m = new Map<string, CleanupCategory[]>();
    for (const c of report?.categories ?? []) m.set(c.group, [...(m.get(c.group) ?? []), c]);
    return [...m.entries()];
  }, [report]);

  const chosen = report?.categories.filter((c) => selected.has(c.id)) ?? [];
  const chosenSize = chosen.reduce((s, c) => s + (c.size ?? 0), 0);
  const total = report?.categories.filter((c) => usable(c, report.isAdmin)).reduce((s, c) => s + (c.size ?? 0), 0) ?? 0;

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const start = async () => {
    if (!report || !chosen.length) return;
    setPhase({ kind: "running", report, progress: null });
    try {
      // Categorias do sistema mexem em pastas do Windows: ponto de restauração antes.
      if (chosen.some((c) => c.needsAdmin)) await beforeChange(api, "Limpeza automática");
      const run = await api.cleanupRun(chosen.map((c) => c.id));
      setPhase({ kind: "done", report, run });
      onFinished(run);
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  return (
    <div className="overlay" onClick={() => !busy && onClose()}>
      <div className="dialog cleanup-dialog" role="dialog" aria-modal aria-label="Limpeza automática" onClick={(e) => e.stopPropagation()}>
        <h3>Limpeza automática</h3>

        {phase.kind === "analyzing" && (
          <div className="cleanup-wait">
            <div className="spinner" aria-hidden />
            <span className="muted">Procurando caches, temporários e restos de instalação…</span>
          </div>
        )}

        {phase.kind === "error" && (
          <>
            <p className="hint-box system">{phase.message}</p>
            <div className="dialog-actions">
              <button className="btn" onClick={onClose}>
                Fechar
              </button>
              <button className="btn primary" onClick={analyze}>
                Analisar de novo
              </button>
            </div>
          </>
        )}

        {phase.kind === "ready" && (
          <>
            <p className="muted small">
              Encontrado <strong>{formatBytes(total)}</strong> que pode ser liberado. Os itens são apagados{" "}
              <strong>permanentemente</strong> (não vão para a Lixeira). Arquivos em uso são ignorados.
            </p>
            {!phase.report.isAdmin && phase.report.categories.some((c) => c.needsAdmin) && (
              <div className="hint-box review small admin-box">
                <span>
                  Algumas categorias exigem <strong>administrador</strong> (temporários do Windows, Windows Update,
                  instalações antigas…).
                </span>
                <button
                  className="btn"
                  onClick={() => api.relaunchAsAdmin().catch((e) => setPhase({ kind: "error", message: String(e) }))}
                >
                  Reiniciar como administrador
                </button>
              </div>
            )}
            <div className="cleanup-list">
              {groups.map(([group, cats]) => (
                <section key={group}>
                  <h4>{group}</h4>
                  <ul>
                    {cats.map((c) => {
                      const disabled = !usable(c, phase.report.isAdmin) || !hasContent(c);
                      return (
                        <li key={c.id} className={`cleanup-item ${disabled ? "disabled" : ""}`}>
                          <label className="check">
                            <input
                              type="checkbox"
                              checked={selected.has(c.id)}
                              disabled={disabled}
                              onChange={() => toggle(c.id)}
                            />
                            <span className="cleanup-title">{c.title}</span>
                            {c.level === "review" && (
                              <span className="badge review" title="Tem um custo; revise antes de marcar">
                                revisar
                              </span>
                            )}
                            {c.needsAdmin && !phase.report.isAdmin && <span className="badge warn">requer admin</span>}
                            <span className="cleanup-size">
                              {c.size == null ? "?" : c.size > 0 ? formatBytes(c.size) : "vazio"}
                            </span>
                          </label>
                          <div className="cleanup-detail muted small">
                            {c.detail}{" "}
                            <button
                              className="link-btn"
                              onClick={() => setExpanded((x) => (x === c.id ? null : c.id))}
                              aria-expanded={expanded === c.id}
                            >
                              {expanded === c.id ? "ocultar locais" : `${c.paths.length} ${c.paths.length === 1 ? "local" : "locais"}`}
                            </button>
                          </div>
                          {expanded === c.id && (
                            <ul className="cleanup-paths small">
                              {c.paths.map((p) => (
                                <li key={p}>
                                  <code>{p}</code>
                                </li>
                              ))}
                            </ul>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                </section>
              ))}
              <section className="cleanup-donot" aria-label="Não apague manualmente">
                <h4>⛔ Não apague manualmente</h4>
                <p className="muted small">
                  Ocupam muito espaço, mas apagar à mão quebra o Windows ou programas. Veja como reduzir com segurança:
                </p>
                <ul>
                  {DO_NOT_DELETE.map((d) => (
                    <li key={d.title} className="cleanup-item">
                      <div className="donot-head">
                        <span className="cleanup-title">{d.title}</span>
                        <code className="small">{d.where}</code>
                      </div>
                      <div className="cleanup-detail muted small">{d.why}</div>
                      <div className="cleanup-detail small">
                        <strong>Em vez disso:</strong> {d.instead}
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            </div>
            <div className="dialog-actions">
              <button className="btn" onClick={analyze}>
                Analisar de novo
              </button>
              <span className="spacer" />
              <button className="btn" onClick={onClose}>
                Cancelar
              </button>
              <button className="btn danger" disabled={!chosen.length} onClick={start}>
                Limpar {chosen.length} {chosen.length === 1 ? "categoria" : "categorias"}
                {chosenSize > 0 && ` (${formatBytes(chosenSize)})`}
              </button>
            </div>
          </>
        )}

        {phase.kind === "running" && (
          <div className="cleanup-wait">
            <div className="spinner" aria-hidden />
            <strong aria-live="polite">{phase.progress?.title ?? "Iniciando…"}</strong>
            {phase.progress && (
              <span className="muted small">
                {phase.progress.index + 1} de {phase.progress.total}
              </span>
            )}
            <span className="muted small">Não feche o aplicativo.</span>
          </div>
        )}

        {phase.kind === "done" && (
          <>
            <p>
              <strong>{formatBytes(phase.run.results.reduce((s, r) => s + r.freed, 0))}</strong> liberados.
            </p>
            <table className="cleanup-results">
              <tbody>
                {phase.run.results.map((r) => (
                  <tr key={r.id}>
                    <td>
                      {r.title}
                      {r.error && <div className="danger-text small">{r.error}</div>}
                      {r.failed > 0 && (
                        <div className="muted small">
                          {formatCount(r.failed)} {r.failed === 1 ? "item ignorado" : "itens ignorados"} (em uso ou sem permissão)
                        </div>
                      )}
                    </td>
                    <td className="num">{formatBytes(r.freed)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="dialog-actions">
              <button className="btn" onClick={analyze}>
                Analisar de novo
              </button>
              <span className="spacer" />
              <button className="btn primary" onClick={onClose}>
                Fechar
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
