// Relatório do diagnóstico como uma página HTML autocontida.

import type { Check, CheckStatus, Report } from "./api";
import { formatBytes } from "./format";

const LABEL: Record<CheckStatus, string> = { crit: "Problema", warn: "Atenção", info: "Dica", ok: "OK" };
const COLOR: Record<CheckStatus, string> = { crit: "#d93636", warn: "#b88a00", info: "#2f6fe0", ok: "#1f9d68" };

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function checkHtml(c: Check): string {
  const list = (title: string, items: string[], tag: "ol" | "ul") =>
    items.length ? `<h4>${title}</h4><${tag}>${items.map((i) => `<li>${esc(i)}</li>`).join("")}</${tag}>` : "";
  return `<section class="check" style="border-left-color:${COLOR[c.status]}">
  <h3>${esc(c.title)} <span class="tag" style="background:${COLOR[c.status]}">${LABEL[c.status]}</span></h3>
  <p>${esc(c.summary)}</p>${list("O que fazer", c.actions, "ol")}${list("Detalhes", c.details, "ul")}
</section>`;
}

export function reportHtml(r: Report, network: Check[] | null, generatedAt: Date): string {
  const s = r.summary;
  const rows: [string, string][] = [
    ["Windows", s.os],
    ["Processador", s.cpu],
    ["Memória", `${formatBytes(s.ramTotal)} (${formatBytes(s.ramFree)} livres)`],
    ...s.gpus.map((g): [string, string] => [g.name, `driver ${g.driverVersion} de ${g.driverDate}, ${g.width}×${g.height} @ ${g.refreshRate} Hz`]),
    ...s.monitors.map((m, i): [string, string] => {
      const d = r.monitorsDdc[i];
      return [`Monitor ${i + 1}`, `${m.manufacturer} ${m.name} via ${m.connection}${d?.firmware ? `, firmware ${d.firmware}` : ""}`];
    }),
    ...s.disks.map((d): [string, string] => [d.name, `${d.mediaType}, ${formatBytes(d.size)}, ${d.health}${d.wear != null ? `, desgaste ${d.wear}%` : ""}`]),
    ...s.temps.map((t): [string, string] => [t.label, `${t.celsius} °C`]),
  ];
  return `<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Relatório WindowsDoctor</title>
<style>
body{font-family:"Segoe UI",system-ui,sans-serif;max-width:920px;margin:24px auto;padding:0 16px;color:#1b1e26;background:#fff;line-height:1.5}
h1{margin:0}h2{margin-top:28px}table{border-collapse:collapse;width:100%}td{padding:6px 8px;border-bottom:1px solid #e3e5ea;vertical-align:top}
td:first-child{color:#6a7080;width:34%}.check{border:1px solid #e3e5ea;border-left:5px solid;border-radius:8px;padding:10px 14px;margin:10px 0}
.check h3{margin:0;font-size:16px}.tag{color:#fff;font-size:11px;padding:1px 7px;border-radius:99px;vertical-align:middle}
h4{margin:8px 0 2px;font-size:12px;text-transform:uppercase;color:#6a7080}ul,ol{margin:0;padding-left:20px}.muted{color:#6a7080}
</style></head><body>
<h1>Relatório do WindowsDoctor</h1>
<p class="muted">Gerado em ${esc(generatedAt.toLocaleString("pt-BR"))}</p>
<h2>Sistema</h2>
<table>${rows.map(([k, v]) => `<tr><td>${esc(k)}</td><td>${esc(v)}</td></tr>`).join("")}</table>
<h2>Verificações</h2>
${r.checks.map(checkHtml).join("\n")}
${network ? `<h2>Rede</h2>\n${network.map(checkHtml).join("\n")}` : ""}
</body></html>
`;
}
