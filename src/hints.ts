// Dicas sobre pastas/arquivos conhecidos que costumam ocupar muito espaço no
// Windows e o que fazer com eles.

export type HintLevel = "safe" | "review" | "system";

export interface Hint {
  level: HintLevel;
  title: string;
  detail: string;
}

interface Rule extends Hint {
  test: RegExp;
}

// Testadas contra o caminho completo em minúsculas, com "\" como separador.
const RULES: Rule[] = [
  { test: /\\windows\\winsxs$/, level: "system", title: "Componentes do Windows",
    detail: "Não apague manualmente. Use \"Limpeza de componentes (WinSxS)\" na Limpeza automática, que roda o DISM com segurança." },
  { test: /\\windows\\softwaredistribution\\download$/, level: "safe", title: "Cache do Windows Update",
    detail: "Pode ser limpo pela Limpeza de Disco (cleanmgr) > Limpar arquivos do sistema." },
  { test: /\\windows\\installer$/, level: "system", title: "Instaladores MSI",
    detail: "Não apague: necessário para desinstalar/reparar programas." },
  { test: /^[a-z]:\\windows\.old$/, level: "review", title: "Instalação anterior do Windows",
    detail: "Remova pela Limpeza automática (\"Instalações anteriores do Windows\"). Você perde a opção de voltar à versão anterior." },
  { test: /^[a-z]:\\\$recycle\.bin$/, level: "safe", title: "Lixeira",
    detail: "Esvazie a Lixeira para liberar este espaço." },
  { test: /^[a-z]:\\hiberfil\.sys$/, level: "review", title: "Arquivo de hibernação",
    detail: "Se não usa hibernação: 'powercfg /h off' (como administrador) remove o arquivo." },
  { test: /^[a-z]:\\(pagefile|swapfile)\.sys$/, level: "system", title: "Memória virtual",
    detail: "Ajuste em Sistema > Configurações avançadas > Desempenho > Memória virtual." },
  { test: /^[a-z]:\\system volume information$/, level: "system", title: "Pontos de restauração",
    detail: "Gerencie em Proteção do Sistema > Configurar > Excluir." },
  { test: /\\(windows|appdata\\local)\\temp$/, level: "safe", title: "Arquivos temporários",
    detail: "Seguro apagar o conteúdo (alguns arquivos em uso podem falhar)." },
  { test: /\\appdata\\local\\crashdumps$|\\windows\\minidump$|\\memory\.dmp$/, level: "safe", title: "Dumps de erro",
    detail: "Relatórios de travamento; seguro apagar." },
  { test: /\\node_modules$/, level: "review", title: "Dependências npm",
    detail: "Recriável com 'npm install' no projeto." },
  { test: /\\(target|build|\.next|dist|obj|bin\\debug|bin\\release)$/, level: "review", title: "Saída de build",
    detail: "Normalmente recriável compilando o projeto de novo." },
  // Same split as the "Limpeza automática" categories (dev-caches / dev-packages).
  { test: /\\(npm-cache|\.cargo\\registry|pip\\cache|yarn\\cache|go-build)$/,
    level: "safe", title: "Cache de pacotes", detail: "Baixado de novo quando necessário." },
  { test: /\\(\.gradle\\caches|\.m2\\repository|\.nuget\\packages|\.pnpm-store)$/,
    level: "review", title: "Pacotes baixados",
    detail: "Repositórios locais de dependências; os projetos baixam tudo de novo no próximo build (pode demorar)." },
  { test: /\\appdata\\local\\docker\\wsl|\\ext4\.vhdx$/, level: "review", title: "Disco virtual WSL/Docker",
    detail: "Não apague: rode 'docker system prune' e compacte o .vhdx (Optimize-VHD / diskpart compact vdisk)." },
  { test: /\\nvidia\\dxcache$/, level: "safe", title: "Cache de shaders NVIDIA",
    detail: "Seguro apagar; recriado conforme necessário." },
  { test: /\\nvidia corporation\\installer2$/, level: "safe", title: "Restos de instaladores NVIDIA",
    detail: "Pacotes extraídos pelo instalador do driver; podem ser apagados (Limpeza automática)." },
  { test: /^[a-z]:\\program files( \(x86\))?$/, level: "system", title: "Programas instalados",
    detail: "Não apague pastas de programas: desinstale em Configurações > Aplicativos." },
  { test: /\\programdata\\package cache$/, level: "review", title: "Cache de instaladores",
    detail: "Usado para reparar/desinstalar programas. Apagar pode impedir desinstalações limpas." },
  { test: /\\(cache|cache2|code cache|gpucache|service worker)$/, level: "safe", title: "Cache",
    detail: "Dados temporários do aplicativo; recriados automaticamente." },
  { test: /\\downloads$/, level: "review", title: "Downloads",
    detail: "Costuma acumular instaladores e arquivos esquecidos. Revise." },
  { test: /\\steamapps\\common$/, level: "review", title: "Jogos da Steam",
    detail: "Desinstale pela Steam para não deixar registros órfãos." },
  { test: /\.(iso|img|vhd)$/, level: "review", title: "Imagem de disco",
    detail: "Imagens de instalação geralmente podem ser baixadas de novo." },
];

/** How each level is shown to the user (same classification as the Limpeza tab). */
export const LEVEL_INFO: Record<HintLevel, { icon: string; label: string }> = {
  safe: { icon: "✓", label: "Pode apagar" },
  review: { icon: "!", label: "Revise antes de apagar" },
  system: { icon: "⛔", label: "Não apague manualmente" },
};

export function hintFor(path: string): Hint | null {
  const p = path.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  for (const r of RULES) if (r.test.test(p)) return r;
  return null;
}

// Folders inside Windows whose *contents* can be deleted (mirrors
// WINDOWS_CLEANABLE in the backend).
const WINDOWS_CLEANABLE = /^[a-z]:\\windows\\(temp|softwaredistribution\\download)\\./;

/**
 * Items the app refuses to delete (the backend refuses them too): drive
 * roots and the Windows folder, except the contents of WINDOWS_CLEANABLE.
 */
export function isProtected(path: string): boolean {
  const p = path.replace(/\//g, "\\").toLowerCase();
  if (WINDOWS_CLEANABLE.test(p)) return false;
  return /^[a-z]:\\windows(\\|$)/.test(p) || /^[a-z]:\\?$/.test(p);
}
