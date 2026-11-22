import { useCallback, useEffect, useMemo, useState } from "react";
import type { Api, CleanupItem, CleanupLevel, CleanupRun } from "../api";
import { formatBytes, formatCount } from "../format";

interface Props {
  api: Api;
  /** Called after a cleanup, so the rest of the app refreshes the map and the drives. */
  onCleaned: (run: CleanupRun) => void;
  onReveal: (path: string, select: boolean) => void;
  notify: (text: string, kind?: "ok" | "err") => void;
}

const SECTIONS: { level: CleanupLevel; icon: string; title: string; lead: string }[] = [
  {
    level: "safe",
    icon: "✅",
    title: "Pode apagar",
    lead: "Arquivos temporários e caches. São recriados quando necessário; apagar não afeta seus documentos nem programas.",
  },
  {
    level: "review",
    icon: "⚠️",
    title: "Revise antes de apagar",
    lead: "Pode apagar, mas há consequências (baixar de novo, perder a opção de voltar o Windows). Só é limpo se você marcar.",
  },
  {
    level: "system",
    icon: "⛔",
    title: "Não apague manualmente",
    lead: "Arquivos do sistema. Apagar à mão pode quebrar o Windows; use a ferramenta indicada para reduzir o espaço com segurança.",
  },
];

const TOOL_LABEL: Record<string, string> = {
  cleanmgr: "Abrir Limpeza de Disco",
  systemProtection: "Abrir Proteção do Sistema",
  performance: "Abrir opções de desempenho",
  storageSense: "Abrir Configurações de armazenamento",
};

const isCleanable = (i: CleanupItem) => i.level !== "system" && i.action !== "manual";

export function CleanupPanel({ api, onCleaned, onReveal, notify }: Props) {
  const [items, setItems] = useState<CleanupItem[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [running, setRunning] = useState(false);
  const [lastRun, setLastRun] = useState<CleanupRun | null>(null);
  const [elevate, setElevate] = useState<CleanupItem | null>(null);

  const analyze = useCallback(
    async (keepSelection: boolean) => {
      setLoading(true);
      try {
        const list = await api.analyzeCleanup();
        setItems(list);
        if (!keepSelection) {
          // By default only the safe items that have something to clean.
          setSelected(new Set(list.filter((i) => i.level === "safe" && isCleanable(i) && (i.size ?? 0) > 0).map((i) => i.id)));
        } else {
          setSelected((s) => new Set([...s].filter((id) => list.some((i) => i.id === id && (i.size ?? 0) > 0))));
        }
      } catch (e) {
        notify(String(e), "err");
      } finally {
        setLoading(false);
      }
    },
    [api, notify],
  );

  useEffect(() => {
    analyze(false);
  }, [analyze]);

  const chosen = useMemo(() => (items ?? []).filter((i) => selected.has(i.id)), [items, selected]);
  const chosenSize = chosen.reduce((s, i) => s + (i.size ?? 0), 0);
  const needsAdmin = chosen.some((i) => i.needsAdmin);

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const run = async () => {
    setRunning(true);
    try {
      const r = await api.runCleanup(chosen.map((i) => i.id));
      setLastRun(r);
      setConfirming(false);
      onCleaned(r);
      const failed = r.results.filter((x) => x.error);
      notify(
        `Limpeza concluída: ${formatBytes(r.total.freed)} liberados` +
          (r.total.failed ? ` · ${formatCount(r.total.failed)} arquivos em uso ou sem permissão foram ignorados` : "") +
          (failed.length ? ` · ${failed.length} item(ns) com erro` : ""),
        failed.length ? "err" : "ok",
      );
      await analyze(true);
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setRunning(false);
    }
  };

  return (
    <section className="cleanup">
      <div className="cleanup-head">
        <div>
          <h2>Limpeza</h2>
          <p className="muted">
            O que ocupa espaço no Windows e pode (ou não) ser apagado. O app só apaga o <strong>conteúdo</strong> dos itens
            marcados; a exclusão é <strong>permanente</strong> e arquivos em uso são ignorados.
          </p>
        </div>
        <div className="cleanup-summary">
          <div>
            <span className="muted small">Selecionado</span>
            <strong>{formatBytes(chosenSize)}</strong>
            <span className="muted small">
              {chosen.length} {chosen.length === 1 ? "item" : "itens"}
            </span>
          </div>
          <button className="btn primary" disabled={!chosen.length || running || loading} onClick={() => setConfirming(true)}>
            Executar limpeza
          </button>
          <button className="btn" disabled={loading || running} onClick={() => analyze(true)}>
            {loading ? "Analisando…" : "Analisar de novo"}
          </button>
        </div>
      </div>

      {needsAdmin && (
        <p className="hint-box review">
          Alguns itens marcados exigem que o app seja executado <strong>como administrador</strong>. Sem isso, os arquivos
          protegidos são ignorados (o resto é limpo normalmente).
        </p>
      )}

      {lastRun && <RunReport run={lastRun} onClose={() => setLastRun(null)} />}

      {!items ? (
        <div className="empty">
          <div className="spinner" aria-hidden /> Procurando o que pode ser limpo…
        </div>
      ) : (
        SECTIONS.map((sec) => {
          const list = items.filter((i) => i.level === sec.level);
          if (!list.length) return null;
          const total = list.reduce((s, i) => s + (i.size ?? 0), 0);
          return (
            <div key={sec.level} className={`cleanup-section ${sec.level}`}>
              <h3>
                <span aria-hidden>{sec.icon}</span> {sec.title}
                <span className="muted small"> · {formatBytes(total)}</span>
              </h3>
              <p className="muted small section-lead">{sec.lead}</p>
              <ul className="cleanup-list">
                {list.map((i) => (
                  <CleanupRow
                    key={i.id}
                    item={i}
                    checked={selected.has(i.id)}
                    onToggle={() => toggle(i.id)}
                    onReveal={onReveal}
                    onTool={(t) => api.openSystemTool(t).catch((e) => notify(String(e), "err"))}
                    onElevate={() => setElevate(i)}
                  />
                ))}
              </ul>
            </div>
          );
        })
      )}

      {confirming && (
        <div className="overlay" onClick={() => !running && setConfirming(false)}>
          <div className="dialog" role="dialog" aria-modal onClick={(e) => e.stopPropagation()}>
            <h3>Executar limpeza?</h3>
            <p>
              O conteúdo dos itens abaixo será apagado <strong>permanentemente</strong> (não vai para a Lixeira):
            </p>
            <ul className="confirm-list">
              {chosen.map((i) => (
                <li key={i.id}>
                  <span>
                    {i.title}
                    {i.level === "review" && <span className="badge review">revisar</span>}
                  </span>
                  <strong>{i.size != null ? formatBytes(i.size) : "—"}</strong>
                </li>
              ))}
            </ul>
            <p>
              Até <strong>{formatBytes(chosenSize)}</strong> serão liberados. Arquivos em uso e itens recentes demais são
              mantidos.
            </p>
            <div className="dialog-actions">
              <button className="btn" onClick={() => setConfirming(false)} disabled={running}>
                Cancelar
              </button>
              <button className="btn danger" onClick={run} disabled={running}>
                {running ? "Limpando…" : "Apagar agora"}
              </button>
            </div>
          </div>
        </div>
      )}

      {elevate && (
        <div className="overlay" onClick={() => setElevate(null)}>
          <div className="dialog" role="dialog" aria-modal onClick={(e) => e.stopPropagation()}>
            <h3>Executar como administrador?</h3>
            <p>{elevate.title}</p>
            <code className="path-box">{elevate.command}</code>
            <p className="muted small">
              O Windows vai pedir permissão (UAC) e abrir uma janela de comando para você acompanhar. {elevate.description}
            </p>
            <div className="dialog-actions">
              <button className="btn" onClick={() => setElevate(null)}>
                Cancelar
              </button>
              <button
                className="btn primary"
                onClick={() => {
                  api
                    .runElevated(elevate.commandId!)
                    .then(() => notify("Comando iniciado. Acompanhe na janela que foi aberta e depois clique em \"Analisar de novo\"."))
                    .catch((e) => notify(String(e), "err"));
                  setElevate(null);
                }}
              >
                Executar
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function CleanupRow({
  item,
  checked,
  onToggle,
  onReveal,
  onTool,
  onElevate,
}: {
  item: CleanupItem;
  checked: boolean;
  onToggle: () => void;
  onReveal: (path: string, select: boolean) => void;
  onTool: (tool: string) => void;
  onElevate: () => void;
}) {
  const cleanable = isCleanable(item);
  const empty = item.size === 0;
  const id = `cl-${item.id}`;
  return (
    <li className={`cleanup-row ${checked ? "on" : ""}`}>
      <div className="cleanup-check">
        {cleanable ? (
          <input id={id} type="checkbox" checked={checked} onChange={onToggle} disabled={empty} aria-describedby={`${id}-d`} />
        ) : (
          <span className="no-check" title="Não é apagado pelo app" aria-hidden>
            {item.level === "system" ? "⛔" : "✋"}
          </span>
        )}
      </div>
      <div className="cleanup-body">
        <div className="cleanup-title">
          <label htmlFor={cleanable ? id : undefined}>{item.title}</label>
          {item.needsAdmin && <span className="badge review" title="Exige executar o app como administrador">admin</span>}
          {item.minAgeHours > 0 && (
            <span className="badge safe" title={`Arquivos modificados nas últimas ${item.minAgeHours} h são mantidos`}>
              mantém últimas {item.minAgeHours} h
            </span>
          )}
          <span className="cleanup-size">
            {item.size != null ? formatBytes(item.size) : <span className="muted" title="Tamanho não medido">—</span>}
          </span>
        </div>
        <p id={`${id}-d`} className="cleanup-desc">
          {item.description}
        </p>
        {item.note && <p className="muted small">{item.note}</p>}
        <div className="cleanup-actions">
          {item.paths.length > 0 && (
            <details>
              <summary className="small">
                {item.paths.length === 1 ? "1 local" : `${item.paths.length} locais`}
                {item.fileCount != null && ` · ${formatCount(item.fileCount)} arquivos`}
              </summary>
              <ul className="path-list">
                {item.paths.map((p) => (
                  <li key={p}>
                    <code title={p}>{p}</code>
                    <button className="icon-btn" title="Mostrar no Explorer" onClick={() => onReveal(p, false)}>
                      ↗
                    </button>
                  </li>
                ))}
              </ul>
            </details>
          )}
          {item.tool && (
            <button className="btn small-btn" onClick={() => onTool(item.tool!)}>
              {TOOL_LABEL[item.tool] ?? "Abrir ferramenta"}
            </button>
          )}
          {item.commandId && (
            <button className="btn small-btn" onClick={onElevate} title={item.command ?? undefined}>
              Executar como administrador…
            </button>
          )}
          {item.action === "manual" && !item.tool && !item.commandId && item.level === "review" && item.paths[0] && (
            <button className="btn small-btn" onClick={() => onReveal(item.paths[0], false)}>
              Abrir pasta
            </button>
          )}
        </div>
      </div>
    </li>
  );
}

function RunReport({ run, onClose }: { run: CleanupRun; onClose: () => void }) {
  return (
    <div className="run-report" role="status">
      <div className="run-report-head">
        <strong>Última limpeza: {formatBytes(run.total.freed)} liberados</strong>
        <button className="icon-btn" title="Fechar" onClick={onClose}>
          ✕
        </button>
      </div>
      <table>
        <thead>
          <tr>
            <th>Item</th>
            <th className="num">Liberado</th>
            <th className="num">Apagados</th>
            <th className="num" title="Em uso ou sem permissão">Ignorados</th>
            <th className="num" title="Modificados recentemente">Recentes</th>
          </tr>
        </thead>
        <tbody>
          {run.results.map((r) => (
            <tr key={r.id}>
              <td>
                {r.title}
                {r.error && <div className="danger-text small">{r.error}</div>}
              </td>
              <td className="num">{formatBytes(r.report.freed)}</td>
              <td className="num">{formatCount(r.report.deleted)}</td>
              <td className="num">{r.report.failed ? <span className="warn-text">{formatCount(r.report.failed)}</span> : 0}</td>
              <td className="num">{formatCount(r.report.skippedRecent)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {run.total.failed > 0 && (
        <p className="muted small">
          Arquivos ignorados estavam em uso ou exigem administrador. Feche os programas (ou execute o app como administrador) e
          tente de novo.
        </p>
      )}
    </div>
  );
}
