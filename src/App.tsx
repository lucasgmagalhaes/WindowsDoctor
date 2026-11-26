import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { Api, CleanupRunResult } from "./api";
import { getApi } from "./api";
import { checkForUpdate, type UpdateInfo } from "./update";
import { DiskMap } from "./modules/DiskMap";
import { Cleanup } from "./modules/Cleanup";
import { Diagnostics } from "./modules/Diagnostics";
import { Optimize } from "./modules/Optimize";
import { History } from "./modules/History";

export type Notify = (text: string, kind?: "ok" | "err") => void;
export type ModuleId = "diskmap" | "cleanup" | "diagnostics" | "optimize" | "history";

/** O que todo módulo de ferramentas recebe do shell. */
export interface ToolProps {
  api: Api;
  notify: Notify;
  elevated: boolean;
  onRelaunchAdmin: () => void;
  onOpenModule: (m: ModuleId) => void;
}

const icon = (d: string) => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d={d} />
  </svg>
);

const MODULES: { id: ModuleId; label: string; icon: ReactNode }[] = [
  { id: "diskmap", label: "Mapa do disco", icon: icon("M12 3a9 9 0 1 0 9 9h-9zM15 3.5A9 9 0 0 1 20.5 9H15z") },
  { id: "cleanup", label: "Limpeza", icon: icon("M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5M14 11v5") },
  { id: "diagnostics", label: "Diagnóstico", icon: icon("M3 12h4l2-6 4 12 2-6h6") },
  { id: "optimize", label: "Otimizações", icon: icon("M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M16 4v4M10 10v4M18 16v4") },
  { id: "history", label: "Histórico", icon: icon("M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3") },
];

export default function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [elevated, setElevated] = useState(false);
  const [active, setActive] = useState<ModuleId>("diskmap");
  // Módulos já abertos continuam montados para não perder o estado.
  const [visited, setVisited] = useState<Set<ModuleId>>(new Set(["diskmap"]));
  // Muda quando a Limpeza altera o disco, para o mapa se atualizar.
  const [cleaned, setCleaned] = useState<CleanupRunResult | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [toast, setToast] = useState<{ text: string; kind: "ok" | "err" } | null>(null);

  const notify = useCallback<Notify>((text, kind = "ok") => {
    setToast({ text, kind });
    window.setTimeout(() => setToast((t) => (t?.text === text ? null : t)), 6000);
  }, []);

  useEffect(() => {
    getApi().then((a) => {
      setApi(a);
      a.isElevated().then(setElevated).catch(() => setElevated(false));
      if (!a.isMock) a.appVersion().then(checkForUpdate).then(setUpdate).catch(() => undefined);
    });
  }, []);

  const open = useCallback((m: ModuleId) => {
    setActive(m);
    setVisited((v) => (v.has(m) ? v : new Set(v).add(m)));
  }, []);

  const relaunchAdmin = useCallback(() => {
    api?.relaunchAsAdmin().catch((e) => notify(String(e), "err"));
  }, [api, notify]);

  if (!api) return null;

  const tool: ToolProps = { api, notify, elevated, onRelaunchAdmin: relaunchAdmin, onOpenModule: open };

  return (
    <div className="app">
      <nav className="rail" aria-label="Módulos">
        <div className="rail-brand" title="Windows Folder Map">
          <img src="/icon.svg" alt="" width="28" height="28" />
        </div>
        {MODULES.map((m) => (
          <button
            key={m.id}
            className={`rail-btn ${active === m.id ? "on" : ""}`}
            aria-current={active === m.id ? "page" : undefined}
            onClick={() => open(m.id)}
          >
            {m.icon}
            <span>{m.label}</span>
          </button>
        ))}
        <div className="rail-spacer" />
        {update && (
          <button
            className="rail-update"
            title={`Versão ${update.version} disponível. Clique para baixar.`}
            onClick={() => api.openUrl(update.url).catch((e) => notify(String(e), "err"))}
          >
            Nova versão {update.version}
          </button>
        )}
        <div className={`rail-admin ${elevated ? "on" : ""}`} title={elevated ? "Rodando como administrador" : "Rodando sem privilégios de administrador"}>
          {elevated ? "Admin" : "Usuário"}
        </div>
      </nav>

      <div className="content">
        {api.isMock && (
          <div className="mock-banner">
            Modo demonstração (navegador): dados simulados. Rode com <code>npm run tauri dev</code> para usar no Windows de verdade.
          </div>
        )}
        <DiskMap api={api} notify={notify} active={active === "diskmap"} cleaned={cleaned} onOpenCleanup={() => open("cleanup")} />
        {visited.has("cleanup") && (
          <div className="module" hidden={active !== "cleanup"}>
            <Cleanup {...tool} onCleaned={setCleaned} />
          </div>
        )}
        {visited.has("diagnostics") && (
          <div className="module" hidden={active !== "diagnostics"}>
            <Diagnostics {...tool} />
          </div>
        )}
        {visited.has("optimize") && (
          <div className="module" hidden={active !== "optimize"}>
            <Optimize {...tool} />
          </div>
        )}
        {visited.has("history") && (
          <div className="module" hidden={active !== "history"}>
            <History {...tool} active={active === "history"} />
          </div>
        )}
      </div>

      {toast && (
        <div className={`toast ${toast.kind}`} role="status" onClick={() => setToast(null)}>
          {toast.text}
        </div>
      )}
    </div>
  );
}
