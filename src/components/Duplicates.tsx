import { useEffect, useMemo, useState } from "react";
import type { Api, DuplicateGroup } from "../api";
import type { Notify } from "../App";
import { formatBytes, formatCount } from "../format";
import { Loading } from "./ToolLayout";

const MIN_SIZES = [
  { v: 1024 * 1024, label: "1 MB" },
  { v: 10 * 1024 * 1024, label: "10 MB" },
  { v: 100 * 1024 * 1024, label: "100 MB" },
];

const SHOW = 200;

export function Duplicates({ api, notify }: { api: Api; notify: Notify }) {
  const [root, setRoot] = useState<string | null>(null);
  const [minSize, setMinSize] = useState(MIN_SIZES[1].v);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ files: number; hashed: number } | null>(null);
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    let unsub: (() => void) | null = null;
    let disposed = false;
    api.onDuplicatesProgress(setProgress).then((u) => (disposed ? u() : (unsub = u)));
    return () => {
      disposed = true;
      unsub?.();
    };
  }, [api]);

  const search = async () => {
    setBusy(true);
    setProgress(null);
    setGroups(null);
    try {
      const g = await api.findDuplicates(root ? [root] : null, minSize);
      setGroups(g);
      // Marca todas as cópias menos a primeira de cada grupo.
      setSelected(new Set(g.flatMap((x) => x.paths.slice(1))));
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(false);
    }
  };

  const toggle = (path: string, group: DuplicateGroup) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(path)) n.delete(path);
      else if (group.paths.filter((p) => n.has(p)).length < group.paths.length - 1) n.add(path);
      else notify("Mantenha pelo menos uma cópia de cada arquivo.", "err");
      return n;
    });

  const selectedSize = useMemo(
    () => (groups ?? []).reduce((acc, g) => acc + g.size * g.paths.filter((p) => selected.has(p)).length, 0),
    [groups, selected],
  );
  const wasted = (groups ?? []).reduce((a, g) => a + g.wasted, 0);

  const remove = async () => {
    setDeleting(true);
    try {
      const freed = await api.deleteDuplicates([...selected]);
      notify(`${formatBytes(freed)} enviados para a Lixeira (esvazie-a para liberar o espaço).`);
      setGroups((gs) =>
        (gs ?? [])
          .map((g) => {
            const paths = g.paths.filter((p) => !selected.has(p));
            return { ...g, paths, wasted: g.size * (paths.length - 1) };
          })
          .filter((g) => g.paths.length > 1),
      );
      setSelected(new Set());
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="dup">
      <div className="dup-controls">
        <span>
          Procurar em <strong>{root ?? "sua pasta de usuário"}</strong>
        </span>
        <button
          className="btn"
          disabled={busy}
          onClick={async () => {
            const p = await api.pickFolder();
            if (p) setRoot(p);
          }}
        >
          Escolher pasta…
        </button>
        <label className="inline-field">
          a partir de
          <select value={minSize} onChange={(e) => setMinSize(Number(e.target.value))} disabled={busy}>
            {MIN_SIZES.map((o) => (
              <option key={o.v} value={o.v}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        {busy ? (
          <button className="btn" onClick={() => api.cancelDuplicates()}>
            Cancelar
          </button>
        ) : (
          <button className="btn primary" onClick={search}>
            Procurar duplicados
          </button>
        )}
      </div>
      <p className="muted small">
        Compara tamanho e conteúdo byte a byte. Pastas do Windows, de programas (AppData, Program Files) e arquivos que estão
        só na nuvem ficam de fora. As cópias removidas vão para a Lixeira.
      </p>

      {busy && (
        <Loading
          text={
            progress
              ? `${formatCount(progress.files)} arquivos encontrados · ${formatCount(progress.hashed)} comparados`
              : "Procurando…"
          }
        />
      )}

      {groups && !busy && (
        <>
          <p>
            {groups.length === 0 ? (
              "Nenhum arquivo duplicado encontrado."
            ) : (
              <>
                <strong>{formatCount(groups.length)}</strong> grupos de arquivos iguais; dá para liberar{" "}
                <strong>{formatBytes(wasted)}</strong> mantendo uma cópia de cada.
              </>
            )}
          </p>
          <ul className="dup-groups">
            {groups.slice(0, SHOW).map((g) => (
              <li key={g.paths[0]} className="dup-group">
                <div className="dup-head">
                  <strong>{formatBytes(g.size)}</strong>
                  <span className="muted small">
                    {g.paths.length} cópias · {formatBytes(g.wasted)} a mais
                  </span>
                </div>
                {g.paths.map((p) => (
                  <label key={p} className="check dup-path">
                    <input type="checkbox" checked={selected.has(p)} onChange={() => toggle(p, g)} />
                    <code>{p}</code>
                    {!selected.has(p) && <span className="badge safe">manter</span>}
                  </label>
                ))}
              </li>
            ))}
          </ul>
          {groups.length > SHOW && <p className="muted small">Mostrando os {SHOW} grupos que ocupam mais espaço.</p>}
          {groups.length > 0 && (
            <div className="dialog-actions sticky-actions">
              <button className="btn" onClick={() => setSelected(new Set())} disabled={!selected.size}>
                Desmarcar tudo
              </button>
              <span className="spacer" />
              <button className="btn danger" disabled={!selected.size || deleting} onClick={remove}>
                {deleting ? "Enviando…" : `Enviar ${formatCount(selected.size)} para a Lixeira (${formatBytes(selectedSize)})`}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
