import { useEffect, useMemo, useState } from "react";
import type { Api, Program } from "../api";
import type { Notify } from "../App";
import { formatBytes } from "../format";
import { Loading } from "./ToolLayout";

type Sort = "size" | "name" | "date";

interface Pending {
  program: Program;
  leftovers: [string, number][] | null;
}

export function Programs({ api, notify }: { api: Api; notify: Notify }) {
  const [list, setList] = useState<Program[] | null>(null);
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<Sort>("size");
  const [pending, setPending] = useState<Record<string, Pending>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const load = () =>
    api
      .listPrograms()
      .then(setList)
      .catch((e) => {
        setList([]);
        notify(String(e), "err");
      });

  useEffect(() => {
    load();
    // Carrega só ao abrir a aba.
  }, []);

  const shown = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const items = (list ?? []).filter((p) => !f || p.name.toLowerCase().includes(f) || p.publisher.toLowerCase().includes(f));
    const by: Record<Sort, (a: Program, b: Program) => number> = {
      size: (a, b) => (b.size ?? -1) - (a.size ?? -1),
      name: (a, b) => a.name.localeCompare(b.name, "pt-BR"),
      date: (a, b) => (b.installDate ?? "").localeCompare(a.installDate ?? ""),
    };
    return [...items].sort(by[sort]);
  }, [list, filter, sort]);

  const uninstall = async (p: Program) => {
    setBusy(p.id);
    try {
      await api.uninstallProgram(p.id);
      setPending((x) => ({ ...x, [p.id]: { program: p, leftovers: null } }));
      notify(`Desinstalador de ${p.name} aberto. Termine por lá e depois procure os restos.`);
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(null);
    }
  };

  const findLeftovers = async (id: string) => {
    setBusy(id);
    try {
      const found = await api.findLeftovers(id);
      setPending((x) => ({ ...x, [id]: { ...x[id], leftovers: found } }));
      if (!found.length) notify("Nenhuma pasta sobrando.");
      await load();
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(null);
    }
  };

  const removeLeftovers = async (id: string) => {
    const paths = pending[id]?.leftovers?.map(([p]) => p) ?? [];
    setBusy(id);
    try {
      const freed = await api.deleteLeftovers(paths);
      notify(`${formatBytes(freed)} de restos enviados para a Lixeira.`);
      setPending((x) => {
        const n = { ...x };
        delete n[id];
        return n;
      });
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(null);
    }
  };

  const total = (list ?? []).reduce((a, p) => a + (p.size ?? 0), 0);

  return (
    <div className="dup">
      {Object.values(pending).map(({ program: p, leftovers }) => (
        <div key={p.id} className="notice">
          <span>
            <strong>{p.name}</strong>:{" "}
            {leftovers == null
              ? "depois de terminar o desinstalador, procure pastas que ficaram para trás."
              : leftovers.length
                ? `${leftovers.length} pasta(s) sobrando: ${leftovers.map(([d, s]) => `${d} (${formatBytes(s)})`).join(", ")}`
                : "nenhuma pasta sobrando."}
          </span>
          {leftovers == null ? (
            <button className="btn" disabled={busy === p.id} onClick={() => findLeftovers(p.id)}>
              Procurar restos
            </button>
          ) : leftovers.length ? (
            <button className="btn danger" disabled={busy === p.id} onClick={() => removeLeftovers(p.id)}>
              Enviar para a Lixeira
            </button>
          ) : null}
        </div>
      ))}

      <div className="dup-controls">
        <input className="search" placeholder="Buscar programa ou fabricante" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <label className="inline-field">
          Ordenar por
          <select value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            <option value="size">Tamanho</option>
            <option value="name">Nome</option>
            <option value="date">Instalação mais recente</option>
          </select>
        </label>
        {list && (
          <span className="muted small">
            {list.length} programas · {formatBytes(total)} informados
          </span>
        )}
      </div>

      {!list ? (
        <Loading text="Lendo os programas instalados…" />
      ) : (
        <table className="top-files programs">
          <thead>
            <tr>
              <th>Programa</th>
              <th>Fabricante</th>
              <th>Instalado em</th>
              <th className="num">Tamanho</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {shown.map((p) => (
              <tr key={p.id}>
                <td>
                  <strong>{p.name}</strong>
                  {p.version && <div className="muted small">versão {p.version}</div>}
                </td>
                <td className="small">{p.publisher}</td>
                <td className="small">{p.installDate ? new Date(`${p.installDate}T12:00`).toLocaleDateString("pt-BR") : "—"}</td>
                <td className="num">{p.size ? formatBytes(p.size) : "—"}</td>
                <td className="num">
                  <button className="btn" disabled={busy != null} onClick={() => uninstall(p)}>
                    Desinstalar
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
