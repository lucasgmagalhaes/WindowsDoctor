import { useEffect, useState } from "react";
import type { Api, Frequency, ScheduleInfo } from "../api";
import type { Notify } from "../App";

const OPTIONS: { v: Frequency | "off"; label: string }[] = [
  { v: "off", label: "Desligada" },
  { v: "daily", label: "Todo dia" },
  { v: "weekly", label: "Todo domingo" },
  { v: "monthly", label: "Todo dia 1º do mês" },
];

const KEY = "cleanupScheduleFrequency";

export function Schedule({ api, notify, elevated }: { api: Api; notify: Notify; elevated: boolean }) {
  const [info, setInfo] = useState<ScheduleInfo | null>(null);
  const [freq, setFreq] = useState<Frequency | "off">("off");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .cleanupScheduleStatus()
      .then((i) => {
        setInfo(i);
        let saved: string | null = null;
        try {
          saved = localStorage.getItem(KEY);
        } catch {
          // Sem armazenamento.
        }
        setFreq(i.enabled ? ((saved as Frequency) ?? "weekly") : "off");
      })
      .catch(() => setInfo(null));
  }, [api]);

  const change = async (v: Frequency | "off") => {
    setBusy(true);
    try {
      const i = await api.setCleanupSchedule(v === "off" ? null : v);
      setInfo(i);
      setFreq(v);
      try {
        localStorage.setItem(KEY, v);
      } catch {
        // Ignora.
      }
      notify(v === "off" ? "Limpeza agendada desligada." : "Limpeza agendada no Agendador de Tarefas do Windows.");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="opt-item">
      <div className="opt-main">
        <div className="opt-title">
          <strong>Limpeza agendada</strong>
        </div>
        <p className="muted small">
          O Windows roda a limpeza das categorias seguras sozinho, ao meio-dia (ou quando o PC ligar, se estava desligado),
          sem abrir janela. O resultado aparece no Histórico.
          {!elevated && " Agendada sem administrador, as pastas do sistema ficam de fora."}
        </p>
        {info?.enabled && info.nextRun && <p className="small ok-text">Próxima execução: {info.nextRun}</p>}
      </div>
      <select className="select" value={freq} disabled={busy || !info} onChange={(e) => change(e.target.value as Frequency | "off")}>
        {OPTIONS.map((o) => (
          <option key={o.v} value={o.v}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );
}
