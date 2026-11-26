import { useCallback, useEffect, useState } from "react";
import type { StartupEntry, Tweak } from "../api";
import type { ToolProps } from "../App";
import { beforeChange } from "../safety";
import { BootTime } from "../components/BootTime";
import { AdminNotice, Loading, Toggle, ToolPage } from "../components/ToolLayout";

/** Estado desejado por otimização em cada perfil. */
const PROFILES: { id: string; label: string; detail: string; tweaks: Record<string, boolean> }[] = [
  {
    id: "games",
    label: "Jogos",
    detail: "Desempenho máximo e menos atraso: plano Alto Desempenho, Modo de Jogo, sem gravação em segundo plano e sem economia de energia no PCIe/USB.",
    tweaks: {
      high_performance_plan: true,
      game_mode: true,
      disable_game_dvr: true,
      pcie_aspm_off: true,
      disable_usb_selective_suspend: true,
      windowed_game_optimizations: true,
    },
  },
  {
    id: "battery",
    label: "Economia de energia",
    detail: "Para notebooks na bateria: plano Equilibrado e economia de energia do PCIe e do USB ligadas.",
    tweaks: { high_performance_plan: false, pcie_aspm_off: false, disable_usb_selective_suspend: false },
  },
  {
    id: "privacy",
    label: "Privacidade",
    detail: "Liga todos os ajustes de privacidade.",
    tweaks: {
      disable_advertising_id: true,
      disable_tailored_experiences: true,
      disable_suggested_apps: true,
      disable_web_search: true,
      disable_activity_history: true,
      minimal_telemetry: true,
    },
  },
  {
    id: "defaults",
    label: "Padrão do Windows",
    detail: "Volta tudo o que este app pode mudar para o padrão do Windows (o agendamento de GPU fica como está).",
    tweaks: {
      disable_mpo: false,
      disable_fast_startup: false,
      high_performance_plan: false,
      pcie_aspm_off: false,
      disable_usb_selective_suspend: false,
      game_mode: true,
      disable_game_dvr: false,
      disable_hibernation: false,
      windowed_game_optimizations: false,
      windowed_vrr: false,
      visual_effects_performance: false,
      disable_advertising_id: false,
      disable_tailored_experiences: false,
      disable_suggested_apps: false,
      disable_web_search: false,
      disable_activity_history: false,
      minimal_telemetry: false,
    },
  },
];

export function Optimize({ api, notify, elevated, onRelaunchAdmin }: ToolProps) {
  const [tweaks, setTweaks] = useState<Tweak[] | null>(null);
  const [startup, setStartup] = useState<StartupEntry[] | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Set<string>>(new Set());

  const load = useCallback(async () => {
    try {
      setTweaks(await api.listTweaks());
    } catch (e) {
      notify(String(e), "err");
    }
    try {
      setStartup(await api.listStartup());
      setStartupError(null);
    } catch (e) {
      setStartup([]);
      setStartupError(String(e));
    }
  }, [api, notify]);

  useEffect(() => {
    load();
  }, [load]);

  const withBusy = async (key: string, f: () => Promise<void>) => {
    setBusy((b) => new Set(b).add(key));
    try {
      await f();
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy((b) => {
        const n = new Set(b);
        n.delete(key);
        return n;
      });
    }
  };

  const setTweak = (t: Tweak, apply: boolean) =>
    withBusy(t.id, async () => {
      await beforeChange(api, t.title, notify);
      const n = await api.setTweak(t.id, apply);
      setTweaks((list) => list?.map((x) => (x.id === t.id ? n : x)) ?? null);
      const message = !apply && t.id === "high_performance_plan"
        ? "Plano Equilibrado ativado."
        : `"${n.title}" ${apply ? "aplicado" : "desligado"}.`;
      notify(n.needsRestart ? `${message} Reinicie o PC para valer.` : message);
    });

  const toggleStartup = (s: StartupEntry, enabled: boolean) =>
    withBusy(s.id, async () => {
      await beforeChange(api, `Inicialização: ${s.name}`, notify);
      await api.setStartup(s.id, enabled);
      setStartup((list) => list?.map((x) => (x.id === s.id ? { ...x, enabled } : x)) ?? null);
    });

  const applyProfile = (profile: (typeof PROFILES)[number]) =>
    withBusy(`profile:${profile.id}`, async () => {
      const list = tweaks ?? [];
      const todo = list.filter((t) => t.id in profile.tweaks && t.applied !== null && t.applied !== profile.tweaks[t.id]);
      const allowed = todo.filter((t) => !t.needsAdmin || elevated || api.isMock);
      const skipped = todo.length - allowed.length;
      if (!todo.length) {
        notify(`O perfil ${profile.label} já está aplicado.`);
        return;
      }
      await beforeChange(api, `Perfil ${profile.label}`, notify);
      let changed = 0;
      const errors: string[] = [];
      for (const t of allowed) {
        try {
          const n = await api.setTweak(t.id, profile.tweaks[t.id]);
          setTweaks((l) => l?.map((x) => (x.id === t.id ? n : x)) ?? null);
          changed++;
        } catch (e) {
          errors.push(`${t.title}: ${e}`);
        }
      }
      const restart = allowed.some((t) => t.needsRestart);
      notify(
        `Perfil ${profile.label}: ${changed} ajuste(s) alterado(s).` +
          (skipped ? ` ${skipped} precisam de administrador e ficaram como estavam.` : "") +
          (restart ? " Reinicie o PC para valer tudo." : "") +
          (errors.length ? ` Falhas: ${errors.join("; ")}` : ""),
        errors.length ? "err" : "ok",
      );
    });

  const categories = [...new Set((tweaks ?? []).map((t) => t.category))];
  const enabledStartup = startup?.filter((s) => s.enabled).length ?? 0;

  return (
    <ToolPage
      title="Otimizações"
      subtitle="Ajustes do Windows que resolvem problemas comuns ou melhoram o desempenho. Desligar um ajuste aplica o valor descrito; o estado anterior pode não ser restaurado."
      actions={
        <button className="btn" onClick={load}>
          Atualizar
        </button>
      }
    >
      <AdminNotice elevated={elevated} onRelaunch={onRelaunchAdmin} what="Ajustes marcados com “admin” e programas de todos os usuários só podem ser alterados como administrador." />

      <section className="opt-section">
        <h2>Perfis</h2>
        <div className="profiles">
          {PROFILES.map((p) => (
            <button key={p.id} className="profile" disabled={!tweaks || busy.size > 0} onClick={() => applyProfile(p)}>
              <strong>{busy.has(`profile:${p.id}`) ? "Aplicando…" : p.label}</strong>
              <span className="muted small">{p.detail}</span>
            </button>
          ))}
        </div>
      </section>

      {!tweaks ? (
        <Loading text="Lendo as configurações atuais…" />
      ) : (
        categories.map((cat) => (
          <section key={cat} className="opt-section">
            <h2>{cat}</h2>
            <ul className="opt-list">
              {tweaks
                .filter((t) => t.category === cat)
                .map((t) => (
                  <li key={t.id} className="opt-item">
                    <div className="opt-main">
                      <div className="opt-title">
                        <strong>{t.title}</strong>
                        {t.needsAdmin && <span className="badge review" title="Precisa de administrador">admin</span>}
                        {t.needsRestart && <span className="badge safe" title="Vale depois de reiniciar">reiniciar</span>}
                      </div>
                      <p className="muted small">{t.description}</p>
                      {t.error && <p className="warn-text small">Estado desconhecido: {t.error}</p>}
                    </div>
                    <Toggle
                      label={t.title}
                      checked={!!t.applied}
                      disabled={t.applied === null || busy.has(t.id) || (t.needsAdmin && !elevated && !api.isMock)}
                      onChange={(v) => setTweak(t, v)}
                    />
                  </li>
                ))}
            </ul>
          </section>
        ))
      )}

      <section className="opt-section">
        <h2>Tempo de inicialização</h2>
        <BootTime api={api} elevated={elevated} />
      </section>

      <section className="opt-section">
        <h2>
          Programas na inicialização{" "}
          {startup && startup.length > 0 && (
            <span className="muted small">
              ({enabledStartup} de {startup.length} ativados)
            </span>
          )}
        </h2>
        <p className="muted small">
          Cada programa ativado atrasa o início do Windows e fica consumindo memória. Desativar não desinstala nada; o
          programa só deixa de abrir sozinho. Inclui as chaves do Registro e a pasta Inicializar (como na aba Inicializar do
          Gerenciador de Tarefas), tarefas agendadas que rodam ao entrar ou ligar e serviços de outros fabricantes. Serviços
          desligados passam para "manual": o programa ainda consegue iniciá-los quando precisar. Para religar um serviço,
          use o Histórico.
        </p>
        {startupError && <p className="warn-text small">{startupError}</p>}
        {!startup ? (
          <Loading text="Lendo programas…" />
        ) : (
          <ul className="opt-list">
            {startup.map((s) => (
              <li key={s.id} className="opt-item">
                <div className="opt-main">
                  <div className="opt-title">
                    <strong>{s.name}</strong>
                    <span className="muted small">{s.location}</span>
                  </div>
                  <code className="cmd small" title={s.command}>
                    {s.command}
                  </code>
                </div>
                <Toggle
                  label={`Iniciar ${s.name} com o Windows`}
                  checked={s.enabled}
                  disabled={busy.has(s.id) || (s.needsAdmin && !elevated && !api.isMock)}
                  onChange={(v) => toggleStartup(s, v)}
                />
              </li>
            ))}
            {startup.length === 0 && !startupError && <li className="empty">Nenhum programa inicia com o Windows.</li>}
          </ul>
        )}
      </section>
    </ToolPage>
  );
}
