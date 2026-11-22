// Windows locations that take up a lot of space but must NOT be deleted by
// hand. Shown in the "Limpeza automática" dialog next to the cleanable
// categories, with the safe way to reduce each one.

export interface DoNotDeleteItem {
  title: string;
  where: string;
  why: string;
  instead: string;
}

export const DO_NOT_DELETE: DoNotDeleteItem[] = [
  {
    title: "Componentes do Windows",
    where: "C:\\Windows\\WinSxS",
    why: "Bibliotecas usadas pelo sistema e pelas atualizações. Apagar à mão quebra o Windows.",
    instead:
      "Marque \"Limpeza de componentes (WinSxS)\" acima: ela usa o DISM, que remove só versões antigas. O tamanho mostrado pelo Explorer é exagerado (muitos arquivos são links).",
  },
  {
    title: "Instaladores de programas",
    where: "C:\\Windows\\Installer",
    why: "O Windows usa estes arquivos para reparar, atualizar e desinstalar programas.",
    instead: "Não há forma segura de reduzir; desinstale programas que não usa em Configurações > Aplicativos.",
  },
  {
    title: "Memória virtual",
    where: "C:\\pagefile.sys, C:\\swapfile.sys",
    why: "Extensão da memória RAM no disco; o Windows recria o arquivo.",
    instead: "Ajuste o tamanho em Sistema > Configurações avançadas > Desempenho > Avançado > Memória virtual.",
  },
  {
    title: "Arquivo de hibernação",
    where: "C:\\hiberfil.sys",
    why: "Usado pela hibernação e pela Inicialização Rápida; o arquivo está sempre em uso.",
    instead: "Se não usa esses recursos, rode \"powercfg /h off\" como administrador e ele é removido.",
  },
  {
    title: "Pontos de restauração",
    where: "C:\\System Volume Information",
    why: "Cópias de segurança para desfazer alterações no sistema; a pasta é protegida.",
    instead: "Em Proteção do Sistema > Configurar, exclua pontos antigos ou limite o espaço usado.",
  },
  {
    title: "Programas instalados",
    where: "C:\\Program Files, C:\\Program Files (x86)",
    why: "Apagar a pasta de um programa deixa restos no registro e pode quebrar outros programas.",
    instead: "Desinstale em Configurações > Aplicativos (jogos: pelo próprio launcher).",
  },
  {
    title: "Pasta do Windows e do sistema",
    where: "C:\\Windows (exceto Temp)",
    why: "Arquivos essenciais do sistema operacional.",
    instead: "Use as categorias acima (temporários, Windows Update, logs) para liberar o que é seguro.",
  },
];
