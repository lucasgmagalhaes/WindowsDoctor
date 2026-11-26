import { useState } from "react";
import type { CleanupRunResult } from "../api";
import type { ToolProps } from "../App";
import { CleanupDialog } from "../components/Cleanup";
import { Duplicates } from "../components/Duplicates";
import { Schedule } from "../components/Schedule";
import { ToolPage } from "../components/ToolLayout";

type Tab = "auto" | "duplicates";

const TABS: { id: Tab; label: string; subtitle: string }[] = [
  {
    id: "auto",
    label: "Limpeza automática",
    subtitle:
      "Caches, temporários e restos de instalação que o Windows e os programas recriam sozinhos. Itens marcados com “revisar” têm um custo; leia antes de marcar.",
  },
  {
    id: "duplicates",
    label: "Duplicados",
    subtitle: "Arquivos idênticos guardados em mais de um lugar.",
  },
];

export function Cleanup({ api, notify, elevated, onOpenModule, onCleaned }: ToolProps & { onCleaned: (run: CleanupRunResult) => void }) {
  const [tab, setTab] = useState<Tab>("auto");
  // Abas já abertas continuam montadas para não perder o resultado.
  const [seen, setSeen] = useState<Set<Tab>>(new Set(["auto"]));
  const open = (t: Tab) => {
    setTab(t);
    setSeen((s) => new Set(s).add(t));
  };
  return (
    <ToolPage
      title="Limpeza"
      subtitle={TABS.find((t) => t.id === tab)!.subtitle}
      actions={
        <>
          <div className="tabs" role="tablist">
            {TABS.map((t) => (
              <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? "on" : ""} onClick={() => open(t.id)}>
                {t.label}
              </button>
            ))}
          </div>
          <button className="btn" onClick={() => onOpenModule("diskmap")}>
            Abrir mapa do disco
          </button>
        </>
      }
    >
      <div hidden={tab !== "auto"} className="tab-page">
        <Schedule api={api} notify={notify} elevated={elevated} />
        <CleanupDialog api={api} inline onFinished={onCleaned} />
      </div>
      {seen.has("duplicates") && (
        <div hidden={tab !== "duplicates"}>
          <Duplicates api={api} notify={notify} />
        </div>
      )}
    </ToolPage>
  );
}
