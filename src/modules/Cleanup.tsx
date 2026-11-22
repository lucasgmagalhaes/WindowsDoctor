import { useCallback, useEffect, useState } from "react";
import type { CleanResult, CleanupTarget } from "../api";
import type { ToolProps } from "../App";
import { AdminNotice, Loading, ToolPage } from "../components/ToolLayout";
import { formatBytes, formatCount } from "../format";

export function Cleanup({ api, notify, elevated, onRelaunchAdmin, onOpenModule }: ToolProps) {
  const [targets, setTargets] = useState<CleanupTarget[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [results, setResults] = useState<Map<string, CleanResult>>(new Map());

  const analyze = useCallback(async () => {
    setLoading(true);
    try {
      const list = await api.listCleanup();
      setTargets(list);
      // Pré-seleciona o que tem conteúdo e pode ser limpo sem perder nada
      // (a Lixeira fica de fora: o usuário escolhe).
      setSelected(new Set(list.filter((t) => t.size > 0 && t.id !== "recycle_bin" && (elevated || !t.needsAdmin)).map((t) => t.id)));
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setLoading(false);
    }
  }, [api, notify, elevated]);

  useEffect(() => {
    analyze();
  }, [analyze]);

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const chosen = (targets ?? []).filter((t) => selected.has(t.id));
  const total = chosen.reduce((a, t) => a + t.size, 0);

  const clean = async () => {
    setConfirming(false);
    setCleaning(true);
    try {
      const res = await api.runCleanup(chosen.map((t) => t.id));
      setResults(new Map(res.map((r) => [r.id, r])));
      const freed = res.reduce((a, r) => a + r.freed, 0);
      const skipped = res.reduce((a, r) => a + r.skipped, 0);
      const errors = res.filter((r) => r.error);
      notify(
        `${formatBytes(freed)} liberados.` +
          (skipped ? ` ${formatCount(skipped)} arquivos em uso ou sem permissão foram mantidos.` : "") +
          (errors.length ? ` Falhas: ${errors.map((r) => r.error).join("; ")}` : ""),
        errors.length ? "err" : "ok",
      );
      const list = await api.listCleanup();
      setTargets(list);
      setSelected(new Set());
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setCleaning(false);
    }
  };

  return (
    <ToolPage
      title="Limpeza"
      subtitle="Arquivos temporários e caches que o Windows e os programas recriam sozinhos. Nada pessoal é apagado."
      actions={
        <>
          <button className="btn" onClick={analyze} disabled={loading || cleaning}>
            Analisar de novo
          </button>
          <button className="btn" onClick={() => onOpenModule("diskmap")}>
            Abrir mapa do disco
          </button>
        </>
      }
    >
      <AdminNotice elevated={elevated} onRelaunch={onRelaunchAdmin} what="As pastas do Windows (Temp, Windows Update) só podem ser limpas como administrador." />

      {!targets || loading ? (
        <Loading text="Medindo pastas temporárias e caches…" />
      ) : (
        <>
          <ul className="clean-list">
            {targets.map((t) => {
              const blocked = t.needsAdmin && !elevated;
              const r = results.get(t.id);
              return (
                <li key={t.id} className={`clean-item ${selected.has(t.id) ? "on" : ""}`}>
                  <label className="clean-check">
                    <input
                      type="checkbox"
                      checked={selected.has(t.id)}
                      disabled={cleaning || t.size === 0}
                      onChange={() => toggle(t.id)}
                    />
                  </label>
                  <div className="clean-main">
                    <div className="clean-title">
                      <strong>{t.title}</strong>
                      {t.needsAdmin && <span className="badge review" title="Precisa de administrador">admin</span>}
                    </div>
                    <p className="muted small">{t.description}</p>
                    {blocked && <p className="warn-text small">Sem administrador, só parte dos arquivos pode ser apagada.</p>}
                    {t.note && <p className="warn-text small">{t.note}</p>}
                    {r && (
                      <p className="small ok-text">
                        Última limpeza: {formatBytes(r.freed)} liberados
                        {r.skipped > 0 && `, ${formatCount(r.skipped)} mantidos (em uso)`}
                        {r.error && <span className="danger-text"> · {r.error}</span>}
                      </p>
                    )}
                  </div>
                  <div className="clean-size">
                    <strong>{formatBytes(t.size)}</strong>
                    <span className="muted small">
                      {formatCount(t.files)} {t.id === "recycle_bin" ? "itens" : "arquivos"}
                    </span>
                  </div>
                </li>
              );
            })}
          </ul>

          <div className="clean-footer">
            <span>
              Selecionado: <strong>{formatBytes(total)}</strong> <span className="muted">em {chosen.length} itens</span>
            </span>
            <button className="btn primary" disabled={!chosen.length || cleaning} onClick={() => setConfirming(true)}>
              {cleaning ? "Limpando…" : `Limpar ${formatBytes(total)}`}
            </button>
          </div>
        </>
      )}

      {confirming && (
        <div className="overlay" onClick={() => setConfirming(false)}>
          <div className="dialog" role="dialog" aria-modal onClick={(e) => e.stopPropagation()}>
            <h3>Limpar {formatBytes(total)}?</h3>
            <ul className="confirm-list">
              {chosen.map((t) => (
                <li key={t.id}>
                  {t.title} <span className="muted">· {formatBytes(t.size)}</span>
                </li>
              ))}
            </ul>
            <p className="muted small">
              Os arquivos são apagados permanentemente (não vão para a Lixeira). Arquivos em uso são pulados.
            </p>
            <div className="dialog-actions">
              <button className="btn" onClick={() => setConfirming(false)}>
                Cancelar
              </button>
              <button className="btn danger" onClick={clean}>
                Limpar agora
              </button>
            </div>
          </div>
        </div>
      )}
    </ToolPage>
  );
}
