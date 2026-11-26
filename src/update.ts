// Verifica se há uma versão nova nas releases do GitHub (uma vez por dia).

const REPO = "lucasgmagalhaes/WindowsDoctor";
const KEY = "updateCheck";

export interface UpdateInfo {
  version: string;
  url: string;
}

/** "v1.2.10" > "1.2.9"; partes ausentes valem 0. */
export function isNewer(latest: string, current: string): boolean {
  const parse = (v: string) => v.replace(/^v/i, "").split(/[.-]/).slice(0, 3).map((x) => parseInt(x, 10) || 0);
  const [a, b] = [parse(latest), parse(current)];
  for (let i = 0; i < 3; i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return false;
}

export async function checkForUpdate(currentVersion: string): Promise<UpdateInfo | null> {
  try {
    const cached = JSON.parse(localStorage.getItem(KEY) ?? "null") as { at: number; info: UpdateInfo | null } | null;
    if (cached && Date.now() - cached.at < 24 * 3600e3) {
      return cached.info && isNewer(cached.info.version, currentVersion) ? cached.info : null;
    }
  } catch {
    // Sem cache: consulta.
  }
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    // 404: ainda não há release (ou o repositório é privado).
    const info: UpdateInfo | null = r.ok
      ? await r.json().then((j) => ({ version: String(j.tag_name ?? ""), url: String(j.html_url ?? "") }))
      : null;
    try {
      localStorage.setItem(KEY, JSON.stringify({ at: Date.now(), info }));
    } catch {
      // Ignora.
    }
    return info && info.url.startsWith("https://github.com/") && isNewer(info.version, currentVersion) ? info : null;
  } catch {
    return null;
  }
}
