import { useCallback, useEffect, useState } from "react";
import type { Check, CheckStatus, Report, Tweak } from "../api";
import type { ToolProps } from "../App";
import { beforeChange, blackScreenMonitorEnabled, setBlackScreenMonitorEnabled } from "../safety";
import { AdminNotice, Loading, Toggle, ToolPage } from "../components/ToolLayout";
import { formatBytes } from "../format";

const STATUS_LABEL: Record<CheckStatus, string> = { crit: "Problema", warn: "Atenção", info: "Dica", ok: "OK" };

const HEALTH: Record<string, string> = { Healthy: "Saudável", Warning: "Atenção", Unhealthy: "Com defeito" };
const health = (h: string) => HEALTH[h] ?? (h || "Saúde desconhecida");

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  return d ? `${d} d ${h} h` : `${h} h ${Math.floor((secs % 3600) / 60)} min`;
}

function reportText(r: Report): string {
  const s = r.summary;
  const lines = [
    `Sistema: ${s.os}`,
    `Processador: ${s.cpu}`,
    `Memória: ${formatBytes(s.ramTotal)} (${formatBytes(s.ramFree)} livres)`,
    `Ligado há: ${formatUptime(s.uptimeSecs)}`,
    ...s.gpus.map((g) => `Vídeo: ${g.name}, driver ${g.driverVersion} (${g.driverDate}), ${g.width}x${g.height} @ ${g.refreshRate} Hz`),
    ...s.monitors.map((m) => `Monitor: ${m.manufacturer} ${m.name} via ${m.connection}`),
    ...s.disks.map((d) => `Disco: ${d.name} (${d.mediaType}, ${formatBytes(d.size)}): ${health(d.health)}`),
    "",
  ];
  for (const c of r.checks) {
    lines.push(`[${STATUS_LABEL[c.status]}] ${c.title}: ${c.summary}`);
    c.details.forEach((d) => lines.push(`    ${d}`));
  }
  return lines.join("\n");
}

export function Diagnostics({ api, notify, elevated, onRelaunchAdmin, onOpenModule }: ToolProps) {
  const [report, setReport] = useState<Report | null>(null);
  const [tweaks, setTweaks] = useState<Tweak[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ranAt, setRanAt] = useState<Date | null>(null);
  const [watching, setWatching] = useState(blackScreenMonitorEnabled);
  const [net, setNet] = useState<Check[] | null>(null);
  const [netBusy, setNetBusy] = useState(false);

  const testNetwork = async () => {
    setNetBusy(true);
    try {
      setNet((await api.networkTest()).checks);
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setNetBusy(false);
    }
  };

  const toggleWatch = async (on: boolean) => {
    try {
      await api.setBlackScreenMonitor(on);
      setBlackScreenMonitorEnabled(on);
      setWatching(on);
      if (on) notify("Vigiando telas pretas enquanto o app estiver aberto.");
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const run = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [r, t] = await Promise.all([api.systemReport(), api.listTweaks()]);
      setReport(r);
      setTweaks(t);
      setRanAt(new Date());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    run();
  }, [run]);

  const applyTweak = async (id: string) => {
    try {
      await beforeChange(api, tweaks.find((x) => x.id === id)?.title ?? id, notify);
      const t = await api.setTweak(id, true);
      setTweaks((list) => list.map((x) => (x.id === id ? t : x)));
      notify(t.needsRestart ? `"${t.title}" aplicado. Reinicie o PC para valer.` : `"${t.title}" aplicado.`);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const copy = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(reportText(report));
      notify("Relatório copiado. Cole onde quiser pedir ajuda (fórum, suporte).");
    } catch (e) {
      notify(`Não foi possível copiar: ${e}`, "err");
    }
  };

  const counts = report?.checks.reduce(
    (acc, c) => ({ ...acc, [c.status]: (acc[c.status] ?? 0) + 1 }),
    {} as Partial<Record<CheckStatus, number>>,
  );

  return (
    <ToolPage
      title="Diagnóstico"
      subtitle="Lê os registros de eventos e o hardware para achar a causa de telas pretas, travamentos, lentidão e erros de disco. Não altera nada."
      actions={
        <>
          {ranAt && <span className="muted small">Executado às {ranAt.toLocaleTimeString("pt-BR")}</span>}
          <button className="btn" onClick={copy} disabled={!report || loading}>
            Copiar relatório
          </button>
          <button className="btn primary" onClick={run} disabled={loading}>
            {loading ? "Analisando…" : "Executar de novo"}
          </button>
        </>
      }
    >
      <AdminNotice elevated={elevated} onRelaunch={onRelaunchAdmin} what="O diagnóstico funciona, mas algumas correções sugeridas precisam de administrador." />

      <div className="opt-item">
        <div className="opt-main">
          <div className="opt-title">
            <strong>Vigiar telas pretas em tempo real</strong>
          </div>
          <p className="muted small">
            Enquanto o app estiver aberto (pode ficar minimizado), avisa quando o driver de vídeo travar e se recuperar e
            anota no Histórico o horário e o programa que estava na tela. Ajuda a ligar os apagões a um jogo ou programa.
          </p>
        </div>
        <Toggle label="Vigiar telas pretas" checked={watching} onChange={toggleWatch} />
      </div>

      {error && <div className="notice err">{error}</div>}
      {loading && !report && <Loading text="Coletando informações do sistema (leva alguns segundos)…" />}

      {report && (
        <>
          <div className="diag-overview">
            {(["crit", "warn", "info", "ok"] as CheckStatus[]).map((s) => (
              <div key={s} className={`count-pill ${s}`}>
                <strong>{counts?.[s] ?? 0}</strong> {STATUS_LABEL[s]}
              </div>
            ))}
          </div>

          <section className="sys-grid">
            <div className="card">
              <h3>Sistema</h3>
              <dl>
                <dt>Windows</dt>
                <dd>{report.summary.os}</dd>
                <dt>Processador</dt>
                <dd>{report.summary.cpu}</dd>
                <dt>Memória</dt>
                <dd>
                  {formatBytes(report.summary.ramTotal)} <span className="muted">({formatBytes(report.summary.ramFree)} livres)</span>
                </dd>
                <dt>Ligado há</dt>
                <dd>{formatUptime(report.summary.uptimeSecs)}</dd>
              </dl>
            </div>
            <div className="card">
              <h3>Vídeo</h3>
              <dl>
                {report.summary.gpus.map((g) => (
                  <FragmentKV key={g.name} k={g.name} v={`Driver ${g.driverVersion} · ${g.driverDate || "data desconhecida"}`} sub={g.width ? `${g.width}×${g.height} @ ${g.refreshRate} Hz` : undefined} />
                ))}
                {report.summary.monitors.map((m, i) => {
                  const ddc = report.monitorsDdc[i];
                  const fw = ddc?.firmware ? ` · firmware ${ddc.firmware}` : ddc && !ddc.ddc ? " · DDC/CI desligado ou sem resposta" : "";
                  return (
                    <FragmentKV
                      key={i}
                      k={`Monitor ${i + 1}`}
                      v={`${m.manufacturer} ${m.name}`.trim() || ddc?.model || "Desconhecido"}
                      sub={`Conexão: ${m.connection}${fw}`}
                    />
                  );
                })}
                {report.summary.monitors.some((m) => m.manufacturer === "AUS") && (
                  <>
                    <dt>Firmware ASUS</dt>
                    <dd className="small">
                      A atualização é feita pelo DisplayWidget Center com o <strong>cabo USB upstream</strong> do monitor ligado
                      ao PC e o DDC/CI ativado no menu do monitor. Sem o cabo USB o app da ASUS não encontra o monitor.
                    </dd>
                  </>
                )}
              </dl>
            </div>
            <div className="card">
              <h3>Discos</h3>
              <dl>
                {report.summary.disks.map((d) => (
                  <FragmentKV
                    key={d.name}
                    k={d.name}
                    v={`${d.mediaType || "?"} · ${formatBytes(d.size)}`}
                    sub={[
                      health(d.health),
                      d.temperature != null && `${d.temperature} °C`,
                      d.wear != null && `desgaste ${d.wear}%`,
                      d.powerOnHours != null && `${d.powerOnHours.toLocaleString("pt-BR")} h ligado`,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  />
                ))}
              </dl>
            </div>
            {(report.summary.temps.length > 0 || report.summary.battery) && (
              <div className="card">
                <h3>Sensores</h3>
                <dl>
                  {report.summary.temps.map((t) => (
                    <FragmentKV key={t.label} k={t.label} v={`${t.celsius.toLocaleString("pt-BR", { maximumFractionDigits: 1 })} °C`} />
                  ))}
                  {report.summary.battery && (
                    <FragmentKV
                      k="Bateria"
                      v={`${Math.round((report.summary.battery.fullChargeCapacity / Math.max(1, report.summary.battery.designCapacity)) * 100)}% da capacidade original`}
                      sub={report.summary.battery.cycleCount != null ? `${report.summary.battery.cycleCount} ciclos` : undefined}
                    />
                  )}
                </dl>
              </div>
            )}
          </section>

          <section className="checks">
            <div className="section-head">
              <h2>Rede</h2>
              <button className="btn" onClick={testNetwork} disabled={netBusy}>
                {netBusy ? "Testando (cerca de 15 s)…" : net ? "Testar de novo" : "Testar rede"}
              </button>
            </div>
            {!net && !netBusy && (
              <p className="muted small">Mede perda de pacotes e latência até o roteador e a internet, o DNS e o sinal do Wi-Fi.</p>
            )}
            {net?.map((c) => (
              <CheckCard key={c.id} check={c} tweak={undefined} onApplyTweak={applyTweak} onOpenModule={onOpenModule} />
            ))}
          </section>

          <section className="checks">
            <div className="section-head">
              <h2>Verificações</h2>
            </div>
            {report.checks.map((c) => (
              <CheckCard
                key={c.id}
                check={c}
                tweak={tweaks.find((t) => t.id === c.tweak)}
                onApplyTweak={applyTweak}
                onOpenModule={onOpenModule}
              />
            ))}
          </section>
        </>
      )}
    </ToolPage>
  );
}

function FragmentKV({ k, v, sub }: { k: string; v: string; sub?: string }) {
  return (
    <>
      <dt>{k}</dt>
      <dd>
        {v}
        {sub && <div className="muted small">{sub}</div>}
      </dd>
    </>
  );
}

function CheckCard({
  check: c,
  tweak,
  onApplyTweak,
  onOpenModule,
}: {
  check: Check;
  tweak: Tweak | undefined;
  onApplyTweak: (id: string) => Promise<void>;
  onOpenModule: ToolProps["onOpenModule"];
}) {
  const [open, setOpen] = useState(c.status === "crit" || c.status === "warn");
  const [busy, setBusy] = useState(false);
  const hasMore = c.details.length > 0 || c.actions.length > 0;
  const showTweak = tweak && tweak.applied === false && c.status !== "ok";

  return (
    <article className={`check-card ${c.status}`}>
      <header className={hasMore ? "clickable" : ""} onClick={() => hasMore && setOpen(!open)}>
        <span className={`status-dot ${c.status}`} aria-hidden />
        <div className="check-head">
          <div className="check-title">
            <strong>{c.title}</strong>
            <span className={`status-tag ${c.status}`}>{STATUS_LABEL[c.status]}</span>
          </div>
          <p>{c.summary}</p>
        </div>
        {hasMore && <span className="chev" aria-hidden>{open ? "▾" : "▸"}</span>}
      </header>
      {open && hasMore && (
        <div className="check-body">
          {c.actions.length > 0 && (
            <>
              <h4>O que fazer</h4>
              <ol>
                {c.actions.map((a, i) => (
                  <li key={i}>{a}</li>
                ))}
              </ol>
            </>
          )}
          {c.details.length > 0 && (
            <>
              <h4>Detalhes</h4>
              <ul className="details">
                {c.details.map((d, i) => (
                  <li key={i}>
                    <code>{d}</code>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
      {(showTweak || c.module) && (
        <footer>
          {showTweak && (
            <button
              className="btn primary"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                await onApplyTweak(tweak.id);
                setBusy(false);
              }}
            >
              {busy ? "Aplicando…" : tweak.title}
              {tweak.needsAdmin && " (admin)"}
            </button>
          )}
          {c.module && (
            <button className="btn" onClick={() => onOpenModule(c.module!)}>
              Abrir {c.module === "cleanup" ? "Limpeza" : "Mapa do disco"}
            </button>
          )}
        </footer>
      )}
    </article>
  );
}
