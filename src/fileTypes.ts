// Agrupa extensões em categorias para a visão "Por tipo".

export const CATEGORIES: { id: string; label: string; color: string; exts: string[] }[] = [
  { id: "video", label: "Vídeos", color: "#e5484d", exts: ["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v", "flv", "ts", "mpg", "mpeg"] },
  { id: "image", label: "Imagens", color: "#f5a524", exts: ["jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "tif", "tiff", "raw", "cr2", "nef", "psd", "svg", "ico"] },
  { id: "audio", label: "Áudio", color: "#a855f7", exts: ["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus"] },
  { id: "docs", label: "Documentos", color: "#3b82f6", exts: ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "odt", "ods", "rtf", "csv", "epub", "md"] },
  { id: "archive", label: "Compactados e imagens de disco", color: "#14b8a6", exts: ["zip", "rar", "7z", "tar", "gz", "xz", "bz2", "iso", "img", "cab", "zst"] },
  { id: "installer", label: "Instaladores", color: "#ec4899", exts: ["msi", "msix", "msixbundle", "appx", "msp", "msu"] },
  { id: "vm", label: "Discos virtuais", color: "#6366f1", exts: ["vhd", "vhdx", "vmdk", "vdi", "qcow2"] },
  { id: "game", label: "Dados de jogos", color: "#22c55e", exts: ["pak", "bundle", "vpk", "bsa", "ba2", "forge", "upk", "uasset", "assets", "resource", "big", "wad"] },
  { id: "program", label: "Programas e bibliotecas", color: "#64748b", exts: ["exe", "dll", "sys", "so", "lib", "pdb", "node", "jar", "pyd", "ocx", "mui"] },
  { id: "dev", label: "Código e desenvolvimento", color: "#0ea5e9", exts: ["js", "ts", "tsx", "py", "rs", "cs", "java", "c", "cpp", "h", "json", "xml", "map", "rlib", "rmeta", "o", "obj", "class"] },
  { id: "system", label: "Arquivos do sistema e caches", color: "#94a3b8", exts: ["db", "dat", "log", "etl", "tmp", "cache", "bin", "blob", "edb", "evtx", "sdb", "cat"] },
];

const BY_EXT = new Map(CATEGORIES.flatMap((c) => c.exts.map((e) => [e, c.id] as const)));

export function categoryOf(ext: string): string {
  return BY_EXT.get(ext) ?? "other";
}

export const OTHER = { id: "other", label: "Outros e sem extensão", color: "#9ca3af" };
