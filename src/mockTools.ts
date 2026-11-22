// Dados simulados dos módulos Limpeza, Diagnóstico e Otimizações para
// desenvolver a interface no navegador.

import type { Api, CleanupTarget, Report, StartupEntry, Tweak } from "./api";

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const report: Report = {
  summary: {
    os: "Microsoft Windows 11 Pro 10.0.22631",
    cpu: "AMD Ryzen 7 5800X3D 8-Core Processor",
    ramTotal: 32 * GB,
    ramFree: 17.4 * GB,
    uptimeSecs: 9 * 86400 + 3 * 3600,
    gpus: [
      {
        name: "NVIDIA GeForce RTX 3070",
        driverVersion: "31.0.15.3623",
        driverDate: "2023-06-14",
        driverAgeDays: 420,
        refreshRate: 165,
        width: 2560,
        height: 1440,
      },
    ],
    monitors: [
      { manufacturer: "AUS", name: "VG27AQ1A", connection: "DisplayPort" },
      { manufacturer: "SAM", name: "S24R35x", connection: "HDMI" },
    ],
    disks: [
      { name: "Samsung SSD 980 1TB", mediaType: "SSD", health: "Healthy", size: 1000 * GB },
      { name: "ST2000DM008", mediaType: "HDD", health: "Healthy", size: 2000 * GB },
    ],
  },
  checks: [
    {
      id: "display_driver",
      title: "Tela preta / driver de vídeo",
      status: "crit",
      summary:
        "O driver de vídeo travou e se recuperou 6 vezes nos últimos 30 dias (4 nos últimos 7). Cada recuperação deixa a tela preta por cerca de 2 segundos.",
      details: [
        "2026-11-21 22:14  (Display 4101)",
        "2026-11-20 19:02  (Display 4101)",
        "2026-11-18 21:47  (nvlddmkm 153)",
        "2026-11-16 20:31  (Display 4101)",
      ],
      actions: [
        "Compare os horários abaixo com os momentos em que a tela apagou.",
        "Faça uma instalação limpa do driver de vídeo (DDU no Modo de Segurança + driver do site do fabricante).",
        "Remova overclock/undervolt da placa de vídeo e verifique a temperatura dela em jogo.",
        "Troque o cabo (DisplayPort certificado VESA ou HDMI Ultra High Speed) e teste outra porta da placa.",
      ],
      tweak: "disable_mpo",
      module: null,
    },
    {
      id: "driver_age",
      title: "Driver da placa de vídeo",
      status: "warn",
      summary: "O driver de NVIDIA GeForce RTX 3070 tem 14 meses.",
      details: ["NVIDIA GeForce RTX 3070: driver 31.0.15.3623 de 2023-06-14"],
      actions: ["Baixe o driver mais recente no site da NVIDIA, AMD ou Intel (o do Windows Update costuma ser antigo)."],
      tweak: null,
      module: null,
    },
    {
      id: "mpo",
      title: "Multiplane Overlay (piscadas)",
      status: "info",
      summary:
        "O MPO (Multiplane Overlay) está ativado. Em placas NVIDIA ele é uma causa conhecida de piscadas e telas pretas rápidas, principalmente com vários monitores ou taxas diferentes.",
      details: [],
      actions: ["Se você tem piscadas ou telas pretas, desative o MPO e reinicie. É reversível."],
      tweak: "disable_mpo",
      module: null,
    },
    {
      id: "uptime",
      title: "Tempo ligado",
      status: "info",
      summary:
        'Último reinício há 9 dias. Com a Inicialização Rápida ligada, "Desligar" não zera o sistema; só "Reiniciar" zera.',
      details: [],
      actions: ["Reinicie o PC (Reiniciar, não Desligar) para aplicar atualizações e liberar recursos."],
      tweak: "disable_fast_startup",
      module: null,
    },
    {
      id: "free_space",
      title: "Espaço na unidade do sistema",
      status: "warn",
      summary: "C: tem 38,0 GB livres (8%). Abaixo de 10% o Windows fica lento e as atualizações falham.",
      details: [],
      actions: ["Use a Limpeza para apagar temporários e caches.", "Use o Mapa do disco para achar o que ocupa mais espaço."],
      tweak: null,
      module: "cleanup",
    },
    { id: "whea", title: "Erros de hardware (WHEA)", status: "ok", summary: "Nenhum erro de hardware registrado nos últimos 30 dias.", details: [], actions: [], tweak: null, module: null },
    { id: "kernel_power", title: "Desligamentos inesperados", status: "ok", summary: "Nenhum desligamento inesperado nos últimos 30 dias.", details: [], actions: [], tweak: null, module: null },
    {
      id: "disks",
      title: "Saúde dos discos",
      status: "ok",
      summary: "Discos saudáveis e sem erros nos últimos 30 dias.",
      details: ["Samsung SSD 980 1TB (SSD): saudável", "ST2000DM008 (HDD): saudável"],
      actions: [],
      tweak: null,
      module: null,
    },
    { id: "memory", title: "Memória", status: "ok", summary: "14,6 GB de 32,0 GB em uso (46%).", details: [], actions: [], tweak: null, module: null },
    { id: "app_crashes", title: "Programas que travaram", status: "ok", summary: "Nenhum programa travou nos últimos 7 dias.", details: [], actions: [], tweak: null, module: null },
  ],
};

const tweaks: Tweak[] = [
  ["disable_mpo", "Desativar Multiplane Overlay (MPO)", "Corrige piscadas e telas pretas rápidas que acontecem em muitas placas NVIDIA, principalmente com dois monitores ou taxas de atualização diferentes. Custo: um pouco mais de uso da GPU em vídeos.", "Vídeo", true, true, false],
  ["disable_fast_startup", "Desativar Inicialização Rápida", 'Com ela ligada, "Desligar" hiberna o núcleo do Windows em vez de desligar de verdade, o que mantém drivers com problema carregados e pode causar falhas ao ligar. O boot fica alguns segundos mais lento.', "Sistema", true, false, false],
  ["high_performance_plan", "Plano de energia Alto Desempenho", "Mantém o processador em frequências altas e reduz atrasos ao sair do repouso. Aumenta o consumo e o calor; em notebooks, prefira só na tomada. Desfazer volta para o plano Equilibrado.", "Energia", false, false, false],
  ["pcie_aspm_off", "Desligar economia de energia do PCI Express", "Impede que o link com a placa de vídeo e o SSD NVMe entre em economia. Ajuda em telas pretas, travadinhas e quedas da placa de vídeo. Vale para o plano de energia atual, na tomada.", "Energia", false, false, false],
  ["disable_usb_selective_suspend", "Desligar suspensão seletiva de USB", "Evita que o Windows desligue portas USB para economizar energia. Ajuda quando mouse, headset ou o hub USB do monitor desconectam sozinhos (e é necessário para algumas ferramentas de firmware). Vale para o plano atual, na tomada.", "Energia", false, false, true],
  ["game_mode", "Modo de Jogo", "O Windows prioriza o jogo aberto e adia atualizações e notificações enquanto você joga.", "Jogos", false, false, true],
  ["disable_game_dvr", "Desativar gravação em segundo plano (Game DVR)", 'A Xbox Game Bar pode gravar o jogo o tempo todo para os "clipes", o que custa desempenho. Desative se não usa esse recurso.', "Jogos", false, false, false],
  ["disable_hibernation", "Desativar hibernação", "Apaga o hiberfil.sys e libera espaço (cerca de 40% da memória RAM). Também desliga a Inicialização Rápida. Em notebooks você perde a opção Hibernar.", "Armazenamento", true, false, false],
].map(([id, title, description, category, needsAdmin, needsRestart, applied]) => ({
  id, title, description, category, needsAdmin, needsRestart, applied, error: null,
} as Tweak));

const startup: StartupEntry[] = [
  { id: "0:Discord", name: "Discord", command: '"C:\\Users\\voce\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe', location: "Registro (este usuário)", enabled: true, needsAdmin: false },
  { id: "0:Spotify", name: "Spotify", command: '"C:\\Users\\voce\\AppData\\Roaming\\Spotify\\Spotify.exe" /minimized', location: "Registro (este usuário)", enabled: true, needsAdmin: false },
  { id: "0:Steam", name: "Steam", command: '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', location: "Registro (este usuário)", enabled: false, needsAdmin: false },
  { id: "1:SecurityHealth", name: "SecurityHealth", command: "%windir%\\system32\\SecurityHealthSystray.exe", location: "Registro (todos os usuários)", enabled: true, needsAdmin: true },
  { id: "2:DisplayWidget Center", name: "DisplayWidget Center", command: '"C:\\Program Files (x86)\\ASUS\\DisplayWidget Center\\DisplayWidgetCenter.exe" -startup', location: "Registro 32 bits (todos os usuários)", enabled: true, needsAdmin: true },
];

const cleanup: CleanupTarget[] = [
  ["user_temp", "Arquivos temporários do usuário", "Pasta %TEMP%. Arquivos usados nas últimas 24 horas são mantidos.", false, 6.8 * GB, 18233],
  ["windows_temp", "Arquivos temporários do Windows", "Pasta Windows\\Temp. Arquivos usados nas últimas 24 horas são mantidos.", true, 1.4 * GB, 912],
  ["windows_update", "Cache do Windows Update", "Instaladores de atualizações já baixadas. O Windows baixa de novo se precisar.", true, 3.4 * GB, 211],
  ["delivery_optimization", "Otimização de Entrega", "Cópias de atualizações compartilhadas com outros PCs.", true, 890 * MB, 37],
  ["shader_cache", "Cache de shaders da placa de vídeo", "DirectX, NVIDIA, AMD e Intel. É recriado; jogos podem engasgar um pouco na primeira vez. Limpar ajuda quando há artefatos ou travamentos após atualizar o driver.", false, 1.9 * GB, 4120],
  ["browser_cache", "Cache dos navegadores", "Chrome, Edge, Brave e Firefox. Senhas, histórico e cookies não são afetados. Feche os navegadores antes para liberar mais.", false, 2.7 * GB, 30981],
  ["error_reports", "Relatórios de erro e despejos de falha", "Relatórios do Windows (WER) e despejos de programas que travaram. Não inclui os minidumps de tela azul.", false, 640 * MB, 58],
  ["recycle_bin", "Lixeira", "Esvazia a Lixeira de todas as unidades. Não dá para desfazer.", false, 4.1 * GB, 133],
].map(([id, title, description, needsAdmin, size, files]) => ({
  id, title, description, needsAdmin, size: Math.round(size as number), files, paths: [], note: null,
} as CleanupTarget));

type Tools = Pick<
  Api,
  | "isElevated" | "relaunchAsAdmin" | "systemReport" | "listTweaks" | "setTweak"
  | "listStartup" | "setStartup" | "listCleanup" | "runCleanup"
>;

export function createMockTools(): Tools {
  return {
    async isElevated() {
      return false;
    },
    async relaunchAsAdmin() {
      throw new Error("no modo demonstração não dá para reabrir como administrador");
    },
    async systemReport() {
      await delay(900);
      return structuredClone(report);
    },
    async listTweaks() {
      await delay(250);
      return tweaks.map((t) => ({ ...t }));
    },
    async setTweak(id, apply) {
      await delay(300);
      const t = tweaks.find((x) => x.id === id);
      if (!t) throw new Error(`otimização desconhecida: ${id}`);
      if (t.needsAdmin) throw new Error("acesso negado: execute o app como administrador");
      t.applied = apply;
      return { ...t };
    },
    async listStartup() {
      await delay(200);
      return startup.map((s) => ({ ...s }));
    },
    async setStartup(id, enabled) {
      await delay(150);
      const s = startup.find((x) => x.id === id);
      if (!s) throw new Error("o programa não está mais na inicialização");
      if (s.needsAdmin) throw new Error("acesso negado: execute o app como administrador");
      s.enabled = enabled;
    },
    async listCleanup() {
      await delay(700);
      return cleanup.map((c) => ({ ...c }));
    },
    async runCleanup(ids) {
      await delay(1200);
      return ids.map((id) => {
        const c = cleanup.find((x) => x.id === id);
        if (!c) return { id, freed: 0, deleted: 0, skipped: 0, error: "item de limpeza desconhecido" };
        if (c.needsAdmin) return { id, freed: 0, deleted: 0, skipped: c.files, error: null };
        // Alguns arquivos ficam "em uso", como no Windows real.
        const skipped = Math.floor(c.files * 0.04);
        const freed = Math.round(c.size * 0.93);
        const deleted = c.files - skipped;
        c.size -= freed;
        c.files = skipped;
        return { id, freed, deleted, skipped, error: null };
      });
    },
  };
}
