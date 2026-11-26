import type { CleanupRunResult } from "../api";
import type { ToolProps } from "../App";
import { CleanupDialog } from "../components/Cleanup";
import { ToolPage } from "../components/ToolLayout";

export function Cleanup({ api, onOpenModule, onCleaned }: ToolProps & { onCleaned: (run: CleanupRunResult) => void }) {
  return (
    <ToolPage
      title="Limpeza"
      subtitle="Caches, temporários e restos de instalação que o Windows e os programas recriam sozinhos. Itens marcados com “revisar” têm um custo; leia antes de marcar."
      actions={
        <button className="btn" onClick={() => onOpenModule("diskmap")}>
          Abrir mapa do disco
        </button>
      }
    >
      <CleanupDialog api={api} inline onFinished={onCleaned} />
    </ToolPage>
  );
}
