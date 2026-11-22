import { useCallback, useEffect, useState } from "react";
import type { StartupEntry, Tweak } from "../api";
import type { ToolProps } from "../App";
import { AdminNotice, Loading, Toggle, ToolPage } from "../components/ToolLayout";

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
      const n = await api.setTweak(t.id, apply);
      setTweaks((list) => list?.map((x) => (x.id === t.id ? n : x)) ?? null);
      const verb = apply ? "aplicado" : "desfeito";
      notify(n.needsRestart ? `"${n.title}" ${verb}. Reinicie o PC para valer.` : `"${n.title}" ${verb}.`);
    });

  const toggleStartup = (s: StartupEntry, enabled: boolean) =>
    withBusy(s.id, async () => {
      await api.setStartup(s.id, enabled);
      setStartup((list) => list?.map((x) => (x.id === s.id ? { ...x, enabled } : x)) ?? null);
    });

  const categories = [...new Set((tweaks ?? []).map((t) => t.category))];
  const enabledStartup = startup?.filter((s) => s.enabled).length ?? 0;

  return (
    <ToolPage
      title="Otimizações"
      subtitle="Ajustes do Windows que resolvem problemas comuns ou melhoram o desempenho. Todos podem ser desfeitos aqui mesmo."
      actions={
        <button className="btn" onClick={load}>
          Atualizar
        </button>
      }
    >
      <AdminNotice elevated={elevated} onRelaunch={onRelaunchAdmin} what="Ajustes marcados com “admin” e programas de todos os usuários só podem ser alterados como administrador." />

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
          programa só deixa de abrir sozinho. É o mesmo ajuste da aba Inicializar do Gerenciador de Tarefas.
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
