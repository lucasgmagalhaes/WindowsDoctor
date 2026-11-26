import { useCallback, useEffect, useState } from "react";
import type { HistoryEntry } from "../api";
import type { ToolProps } from "../App";
import { Loading, Toggle, ToolPage } from "../components/ToolLayout";
import { restorePointEnabled, setRestorePointEnabled } from "../safety";

const KIND_LABEL: Record<string, string> = {
  tweak: "Otimização",
  startup: "Inicialização",
  cleanup: "Limpeza",
  delete: "Exclusão",
  restore: "Restauração",
  repair: "Reparo",
};

export function History({ api, notify, elevated, active }: ToolProps & { active: boolean }) {
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const [restoreOn, setRestoreOn] = useState(restorePointEnabled);

  const load = useCallback(async () => {
    try {
      setEntries(await api.listHistory());
    } catch (e) {
      notify(String(e), "err");
    }
  }, [api, notify]);

  // Recarrega sempre que o módulo aparece: outras telas gravam no histórico.
  useEffect(() => {
    if (active) load();
  }, [active, load]);

  const undo = async (e: HistoryEntry) => {
    setBusy(e.id);
    try {
      await api.undoHistory(e.id);
      notify("Alteração desfeita.");
      await load();
    } catch (err) {
      notify(String(err), "err");
    } finally {
      setBusy(null);
    }
  };

  const clear = async () => {
    await api.clearHistory();
    await load();
  };

  return (
    <ToolPage
      title="Histórico"
      subtitle="Tudo o que o app alterou no seu PC, do mais recente ao mais antigo. Otimizações e programas de inicialização podem ser desfeitos daqui."
      actions={
        <button className="btn" onClick={clear} disabled={!entries?.length}>
          Limpar histórico
        </button>
      }
    >
      <div className="opt-item">
        <div className="opt-main">
          <div className="opt-title">
            <strong>Criar ponto de restauração antes de alterar o sistema</strong>
          </div>
          <p className="muted small">
            Antes da primeira otimização, mudança na inicialização ou limpeza de pastas do Windows em cada uso do app, cria um
            ponto da Restauração do Sistema. Se algo der errado, dá para voltar pelo Windows (rstrui.exe).
            {!elevated && " Só funciona com o app aberto como administrador."}
          </p>
        </div>
        <Toggle
          label="Criar ponto de restauração"
          checked={restoreOn}
          onChange={(v) => {
            setRestorePointEnabled(v);
            setRestoreOn(v);
          }}
        />
      </div>

      {!entries ? (
        <Loading text="Lendo o histórico…" />
      ) : entries.length === 0 ? (
        <p className="empty">Nenhuma alteração registrada ainda.</p>
      ) : (
        <ul className="history">
          {entries.map((e) => (
            <li key={e.id} className={`history-item ${e.undone ? "undone" : ""}`}>
              <div className="history-time muted small">
                {new Date(e.timeMs).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" })}
              </div>
              <div className="opt-main">
                <div className="opt-title">
                  <span className="badge safe">{KIND_LABEL[e.kind] ?? e.kind}</span>
                  <strong>{e.title}</strong>
                </div>
                {e.detail && <p className="muted small">{e.detail}</p>}
              </div>
              {e.undo &&
                (e.undone ? (
                  <span className="muted small">desfeito</span>
                ) : (
                  <button className="btn" disabled={busy != null} onClick={() => undo(e)}>
                    {busy === e.id ? "Desfazendo…" : "Desfazer"}
                  </button>
                ))}
            </li>
          ))}
        </ul>
      )}
    </ToolPage>
  );
}
