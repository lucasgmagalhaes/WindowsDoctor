import { useCallback, useEffect, useRef, useState } from "react";
import type { HierarchyRectangularNode } from "d3-hierarchy";
import type { Api, CleanupRunResult, Comparison, Crumb, ExtStat, Drive, FileInfo, NodeView, Progress, Stats } from "../api";
import { Sunburst } from "../components/Sunburst";
import { Treemap } from "../components/Treemap";
import { ItemList } from "../components/ItemList";
import { TopFiles } from "../components/TopFiles";
import { FileTypes } from "../components/FileTypes";
import { Changes } from "../components/Changes";
import { MoveDialog, type MoveTarget } from "../components/MoveDialog";
import { Sidebar } from "../components/Sidebar";
import { DeleteDialog, ScanOverlay, type DeleteTarget } from "../components/Dialogs";
import { formatBytes, formatCount, formatPercent, nodeLabel } from "../format";
import type { Notify } from "../App";

type RNode = HierarchyRectangularNode<NodeView>;
type ChartMode = "sunburst" | "treemap";
type Tab = "explore" | "top" | "types" | "changes";

interface Loaded {
  chart: NodeView;
  list: NodeView;
  crumbs: Crumb[];
  path: string;
}

interface Props {
  api: Api;
  notify: Notify;
  /** O módulo está visível (atalhos de teclado só valem nele). */
  active: boolean;
  /** Último resultado da Limpeza; o backend já atualizou a varredura. */
  cleaned: CleanupRunResult | null;
  onOpenCleanup: () => void;
}

export function DiskMap({ api, notify, active, cleaned, onOpenCleanup }: Props) {
  const [drives, setDrives] = useState<Drive[]>([]);
  const [scanTarget, setScanTarget] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);
  const [currentId, setCurrentId] = useState<number | null>(null);
  const [version, setVersion] = useState(0);
  const [data, setData] = useState<Loaded | null>(null);
  const [topFiles, setTopFiles] = useState<FileInfo[]>([]);
  const [mode, setMode] = useState<ChartMode>("sunburst");
  const [tab, setTab] = useState<Tab>("explore");
  const [exts, setExts] = useState<ExtStat[]>([]);
  const [extFilter, setExtFilter] = useState<string | null>(null);
  const [changes, setChanges] = useState<Comparison | null>(null);
  const [toMove, setToMove] = useState<MoveTarget | null>(null);
  const [hovered, setHovered] = useState<RNode | null>(null);
  const [toDelete, setToDelete] = useState<DeleteTarget | null>(null);
  const [minFileSize, setMinFileSize] = useState(1024 * 1024);
  const rescanReturnPath = useRef<string | null>(null);

  const refreshDrives = useCallback(async (a: Api) => {
    try {
      setDrives(await a.listDrives());
    } catch (e) {
      notify(String(e), "err");
    }
  }, [notify]);

  // Partições e assinatura dos eventos de varredura.
  useEffect(() => {
    // The subscription is asynchronous: if the effect is torn down before it
    // finishes (React StrictMode mounts twice), the listener has to be removed
    // as soon as it is registered, otherwise each event is handled twice.
    let disposed = false;
    const unsub: Array<() => void> = [];
    const keep = (u: () => void) => (disposed ? u() : unsub.push(u));
    const a = api;
    (async () => {
      refreshDrives(a);
      keep(await a.onProgress(setProgress));
      keep(
        await a.onFinished(async (f) => {
          setScanTarget(null);
          setProgress(null);
          if (!f.ok) {
            notify(f.error ?? "Falha na varredura", "err");
            return;
          }
          setStats(f.stats);
          if (f.cancelled && !f.rescannedPath) notify("Varredura cancelada — exibindo resultado parcial.");
          if (f.rescannedPath && rescanReturnPath.current) {
            const id = await a.findPath(rescanReturnPath.current);
            setCurrentId(id);
            notify(
              f.cancelled
                ? "Re-análise cancelada — os dados anteriores da pasta foram mantidos."
                : `Pasta re-analisada: ${f.rescannedPath}`,
            );
          } else {
            setCurrentId(null);
          }
          rescanReturnPath.current = null;
          setHovered(null);
          setVersion((v) => v + 1);
          refreshDrives(a);
        }),
      );
    })();
    return () => {
      disposed = true;
      unsub.forEach((u) => u());
    };
  }, [api, notify, refreshDrives]);

  // Carrega o nó atual (gráfico + lista + breadcrumb).
  useEffect(() => {
    if (!stats) return;
    let stale = false;
    (async () => {
      try {
        const [chart, list] = await Promise.all([
          api.getNode(currentId, 4, 80, 0.002),
          api.getNode(currentId, 1, 2000, 0),
        ]);
        const id = list.id!;
        const [crumbs, path, top] = await Promise.all([api.getAncestors(id), api.getPath(id), api.getTopFiles(500)]);
        api.getExtensions().then((e) => !stale && setExts(e)).catch(() => undefined);
        api.getChanges().then((c) => !stale && setChanges(c)).catch(() => undefined);
        if (!stale) {
          setData({ chart, list, crumbs, path });
          setTopFiles(top);
        }
      } catch (e) {
        if (!stale) {
          // O nó pode ter sumido (ex.: re-varredura); volta para a raiz.
          if (currentId != null) setCurrentId(null);
          else notify(String(e), "err");
        }
      }
    })();
    return () => {
      stale = true;
    };
  }, [api, stats, currentId, version, notify]);

  const navigate = useCallback((id: number) => {
    setHovered(null);
    setCurrentId(id);
  }, []);

  const goUp = useCallback(() => {
    if (!data || data.crumbs.length < 2) return;
    navigate(data.crumbs[data.crumbs.length - 2].id);
  }, [data, navigate]);

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = !!t && (t.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(t.tagName));
      if (e.key === "Backspace" && active && !typing && !toDelete && !scanTarget) goUp();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [goUp, toDelete, scanTarget, active]);

  const startScan = async (path: string) => {
    setScanTarget(path);
    setProgress(null);
    try {
      await api.startScan({ path, minFileSize });
    } catch (e) {
      setScanTarget(null);
      notify(String(e), "err");
    }
  };

  const rescan = async (id: number) => {
    if (!data) return;
    rescanReturnPath.current = data.path;
    setScanTarget(await api.getPath(id));
    try {
      await api.startScan({ nodeId: id, minFileSize });
    } catch (e) {
      setScanTarget(null);
      notify(String(e), "err");
    }
  };

  const pickFolder = async () => {
    const p = await api.pickFolder();
    if (p) startScan(p);
  };

  const reveal = (path: string, select: boolean) => api.reveal(path, select).catch((e) => notify(String(e), "err"));

  const askDeleteFile = (f: FileInfo) => setToDelete({ id: null, path: f.path, size: f.size, isDir: false });

  const confirmDelete = async (permanent: boolean) => {
    if (!toDelete) return;
    try {
      const r = await api.deleteNode(toDelete.id != null ? { id: toDelete.id } : { path: toDelete.path }, permanent);
      setStats(r.stats);
      notify(
        permanent
          ? `${formatBytes(r.freed)} liberados.`
          : `${formatBytes(r.freed)} enviados para a Lixeira (esvazie-a para liberar o espaço).`,
      );
      setToDelete(null);
      setHovered(null);
      setVersion((v) => v + 1);
      refreshDrives(api);
    } catch (e) {
      notify(String(e), "err");
      setToDelete(null);
      setVersion((v) => v + 1);
    }
  };

  // The backend already updated the folders it cleaned in the current scan.
  useEffect(() => {
    if (!cleaned) return;
    if (cleaned.stats) {
      setStats(cleaned.stats);
      setHovered(null);
      setVersion((v) => v + 1);
    }
    refreshDrives(api);
  }, [cleaned, api, refreshDrives]);

  const scanDrive = stats ? drives.find((d) => d.mount === stats.root) : undefined;
  const unaccounted = scanDrive ? scanDrive.total - scanDrive.available - stats!.totalSize : 0;
  const highlightId = hovered ? hovered.ancestors().find((a) => a.depth === 1)?.data.id ?? null : null;
  const hoverPath = hovered
    ? hovered
        .ancestors()
        .reverse()
        .slice(1)
        .map((a) => nodeLabel(a.data))
        .join(" › ")
    : null;

  return (
    <div className="module" hidden={!active}>
      <Sidebar
        drives={drives}
        activeRoot={stats?.root ?? null}
        busy={!!scanTarget}
        minFileSize={minFileSize}
        onMinFileSize={setMinFileSize}
        onScan={startScan}
        onPickFolder={pickFolder}
        onOpenScan={async () => {
          try {
            const s = await api.openScan();
            if (!s) return;
            setStats(s);
            setCurrentId(null);
            setHovered(null);
            setVersion((v) => v + 1);
            notify(`Varredura de ${s.root} aberta.`);
          } catch (e) {
            notify(String(e), "err");
          }
        }}
        onRefreshDrives={() => refreshDrives(api)}
        onCleanup={onOpenCleanup}
      />

      <main className="main">
        {!stats || !data ? (
          <div className="welcome">
            <h1>Mapa do disco</h1>
            <p>Escolha uma partição à esquerda para ver onde o espaço está sendo usado, da raiz até cada pasta.</p>
            <ul className="muted">
              <li>O <strong>sunburst</strong> mostra cada nível como um anel; a fatia é proporcional ao tamanho.</li>
              <li>O <strong>treemap</strong> mostra os mesmos dados como retângulos aninhados.</li>
              <li>Clique em uma pasta para entrar nela; clique no centro ou use <kbd>Backspace</kbd> para subir.</li>
            </ul>
          </div>
        ) : (
          <>
            <header className="topbar">
              <div className="stats">
                <div>
                  <span className="muted small">Analisado</span>
                  <strong>{formatBytes(stats.totalSize)}</strong>
                </div>
                <div>
                  <span className="muted small">Arquivos</span>
                  <strong>{formatCount(stats.fileCount)}</strong>
                </div>
                <div>
                  <span className="muted small">Pastas</span>
                  <strong>{formatCount(stats.dirCount)}</strong>
                </div>
                <div>
                  <span className="muted small">Tempo</span>
                  <strong>{(stats.elapsedMs / 1000).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} s</strong>
                </div>
                {stats.errors > 0 && (
                  <div title="Pastas que não puderam ser lidas. Execute como administrador para incluí-las.">
                    <span className="muted small">Sem acesso</span>
                    <strong className="warn-text">{formatCount(stats.errors)}</strong>
                  </div>
                )}
                {scanDrive && unaccounted > 1024 ** 3 && (
                  <div title="Espaço usado na partição que não apareceu na varredura: arquivos de sistema (MFT, sombras de volume), pastas sem permissão ou arquivos em uso.">
                    <span className="muted small">Não contabilizado</span>
                    <strong className="muted">{formatBytes(unaccounted)}</strong>
                  </div>
                )}
              </div>
              <button
                className="btn"
                title="Salva esta varredura num arquivo .wdscan para abrir depois ou comparar"
                onClick={() =>
                  api
                    .saveScan()
                    .then((ok) => ok && notify("Varredura salva."))
                    .catch((e) => notify(String(e), "err"))
                }
              >
                Salvar
              </button>
              <div className="tabs" role="tablist">
                <button role="tab" aria-selected={tab === "explore"} className={tab === "explore" ? "on" : ""} onClick={() => setTab("explore")}>
                  Explorar
                </button>
                <button role="tab" aria-selected={tab === "changes"} className={tab === "changes" ? "on" : ""} onClick={() => setTab("changes")}>
                  O que mudou
                </button>
                <button role="tab" aria-selected={tab === "types"} className={tab === "types" ? "on" : ""} onClick={() => setTab("types")}>
                  Por tipo
                </button>
                <button role="tab" aria-selected={tab === "top"} className={tab === "top" ? "on" : ""} onClick={() => setTab("top")}>
                  Maiores arquivos
                </button>
              </div>
            </header>

            {tab === "explore" ? (
              <>
                <nav className="crumbs" aria-label="Caminho">
                  {data.crumbs.map((c, i) => (
                    <span key={c.id}>
                      {i > 0 && <span className="sep">›</span>}
                      <button className="crumb" disabled={i === data.crumbs.length - 1} onClick={() => navigate(c.id)}>
                        {c.name.replace(/\\$/, "") || c.name}
                      </button>
                    </span>
                  ))}
                  <span className="crumb-size">
                    {formatBytes(data.list.size)}
                    {data.crumbs.length > 1 && <span className="muted"> · {formatPercent(data.list.size, stats.totalSize)} do total</span>}
                  </span>
                  {data.crumbs.length > 1 && (
                    <button className="icon-btn" title="Mostrar no Explorer" onClick={() => reveal(data.path, false)}>
                      ↗
                    </button>
                  )}
                </nav>

                <div className="explore">
                  <section className="chart-panel">
                    <div className="chart-toolbar">
                      <div className="seg-ctrl">
                        <button className={mode === "sunburst" ? "on" : ""} onClick={() => setMode("sunburst")}>
                          Sunburst
                        </button>
                        <button className={mode === "treemap" ? "on" : ""} onClick={() => setMode("treemap")}>
                          Treemap
                        </button>
                      </div>
                      <div className="hover-info" aria-live="polite">
                        {hovered ? (
                          <>
                            <span className="hover-path">{hoverPath}</span>
                            <strong>{formatBytes(hovered.data.size)}</strong>
                            <span className="muted">{formatPercent(hovered.data.size, data.chart.size)}</span>
                          </>
                        ) : (
                          <span className="muted">Passe o mouse para detalhes · clique para entrar na pasta</span>
                        )}
                      </div>
                    </div>
                    {mode === "sunburst" ? (
                      <Sunburst
                        data={data.chart}
                        depth={4}
                        hovered={hovered}
                        onHover={setHovered}
                        onNavigate={navigate}
                        onUp={goUp}
                        canGoUp={data.crumbs.length > 1}
                      />
                    ) : (
                      <Treemap data={data.chart} depth={3} hovered={hovered} onHover={setHovered} onNavigate={navigate} />
                    )}
                  </section>

                  <section className="list-panel">
                    <h3>
                      Conteúdo <span className="muted small">({formatCount(data.list.children.length)} itens, maior primeiro)</span>
                    </h3>
                    <ItemList
                      items={data.list.children}
                      parentPath={data.path}
                      parentSize={data.list.size}
                      highlightId={highlightId}
                      onOpen={navigate}
                      onReveal={reveal}
                      onDelete={setToDelete}
                      onRescan={rescan}
                      onMove={setToMove}
                    />
                  </section>
                </div>
              </>
            ) : tab === "changes" ? (
              <section className="top-panel">
                <Changes
                  data={changes}
                  onOpen={async (p) => {
                    const id = await api.findPath(p);
                    if (id == null) {
                      notify("Essa pasta não está mais na varredura.", "err");
                      return;
                    }
                    navigate(id);
                    setTab("explore");
                  }}
                />
              </section>
            ) : tab === "types" ? (
              <section className="top-panel">
                <FileTypes
                  exts={exts}
                  onPickExt={(e) => {
                    setExtFilter(e);
                    setTab("top");
                  }}
                />
              </section>
            ) : (
              <section className="top-panel">
                {extFilter && (
                  <div className="filter-chip">
                    Mostrando só arquivos <strong>.{extFilter}</strong> entre os 500 maiores
                    <button className="icon-btn" title="Remover filtro" onClick={() => setExtFilter(null)}>
                      ✕
                    </button>
                  </div>
                )}
                <TopFiles
                  files={extFilter ? topFiles.filter((f) => f.path.toLowerCase().endsWith(`.${extFilter}`)) : topFiles}
                  onReveal={reveal}
                  onDelete={askDeleteFile}
                />
              </section>
            )}
          </>
        )}
      </main>

      {scanTarget && <ScanOverlay progress={progress} target={scanTarget} onCancel={() => api.cancelScan()} />}
      {toMove && (
        <MoveDialog
          api={api}
          target={toMove}
          drives={drives}
          onClose={() => setToMove(null)}
          onDone={(r) => {
            setToMove(null);
            setStats(r.stats);
            setHovered(null);
            setVersion((v) => v + 1);
            refreshDrives(api);
            notify(
              `${formatBytes(r.bytes)} movidos para ${r.to}.` +
                (r.leftover ? ` Não foi possível apagar ${r.leftover}; apague-a depois.` : ""),
              r.leftover ? "err" : "ok",
            );
          }}
        />
      )}
      {toDelete && <DeleteDialog target={toDelete} onConfirm={confirmDelete} onClose={() => setToDelete(null)} />}
    </div>
  );
}
