// Dados simulados dos módulos Limpeza, Diagnóstico e Otimizações para
// desenvolver a interface no navegador.

import type { Api, HistoryEntry, Report, StartupEntry, Tweak } from "./api";

const GB = 1024 ** 3;
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


type Tools = Pick<
  Api,
  | "isElevated" | "ensureRestorePoint" | "listHistory" | "undoHistory" | "clearHistory" | "relaunchAsAdmin" | "systemReport" | "listTweaks" | "setTweak"
  | "listStartup" | "setStartup"
>;

const history: HistoryEntry[] = [
  { id: 3, timeMs: Date.now() - 3600e3, kind: "startup", title: "Desativado na inicialização: Steam", detail: "", undo: { type: "startup", id: "0:Steam", enabled: true }, undone: false },
  { id: 2, timeMs: Date.now() - 86400e3, kind: "cleanup", title: "Limpeza automática: 9,8 GB liberados", detail: "Arquivos temporários do usuário, Cache dos navegadores", undo: null, undone: false },
  { id: 1, timeMs: Date.now() - 2 * 86400e3, kind: "restore", title: "Ponto de restauração criado", detail: "Desativar Multiplane Overlay (MPO)", undo: null, undone: false },
];

function log(kind: string, title: string, undo: HistoryEntry["undo"]) {
  history.unshift({ id: Date.now(), timeMs: Date.now(), kind, title, detail: "", undo, undone: false });
}

export function createMockTools(): Tools {
  return {
    async ensureRestorePoint() {
      return { kind: "notAdmin" };
    },
    async listHistory() {
      return history.map((h) => ({ ...h }));
    },
    async undoHistory(id) {
      const h = history.find((x) => x.id === id);
      if (!h?.undo) throw new Error("esta ação não pode ser desfeita pelo app");
      h.undone = true;
      const u = h.undo;
      if (u.type === "tweak") {
        const t = tweaks.find((x) => x.id === u.id)!;
        t.applied = u.apply;
        log("tweak", `${u.apply ? "Aplicado" : "Desfeito"}: ${t.title}`, { ...u, apply: !u.apply });
      } else {
        const s = startup.find((x) => x.id === u.id)!;
        s.enabled = u.enabled;
        log("startup", `${u.enabled ? "Ativado" : "Desativado"} na inicialização: ${s.name}`, { ...u, enabled: !u.enabled });
      }
    },
    async clearHistory() {
      history.length = 0;
    },
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
      log("tweak", `${apply ? "Aplicado" : "Desfeito"}: ${t.title}`, { type: "tweak", id, apply: !apply });
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
      log("startup", `${enabled ? "Ativado" : "Desativado"} na inicialização: ${s.name}`, { type: "startup", id, enabled: !enabled });
    },
  };
}
