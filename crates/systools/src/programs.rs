//! Programas instalados (chaves Uninstall), desinstalação pelo desinstalador
//! do próprio programa e busca de pastas que sobram depois.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::reg::{self, Hive};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    /// `<origem>|<subchave>`.
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// AAAA-MM-DD, quando o instalador informa.
    pub install_date: Option<String>,
    /// Bytes: o que o instalador informa ou, sem isso, o tamanho da pasta.
    pub size: Option<u64>,
    pub install_location: Option<String>,
    pub uninstall: String,
}

const ORIGINS: [(Hive, &str); 3] = [
    (Hive::LocalMachine, UNINSTALL),
    (Hive::LocalMachine, UNINSTALL32),
    (Hive::CurrentUser, UNINSTALL),
];

/// Monta um programa a partir dos valores da subchave; `None` para
/// componentes do sistema, atualizações e entradas sem nome.
pub fn from_values(id: String, values: &HashMap<String, String>, estimated_kb: Option<u32>, system: bool) -> Option<Program> {
    let get = |k: &str| values.get(k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let name = get("DisplayName")?;
    if system || values.contains_key("ParentKeyName") {
        return None;
    }
    if matches!(get("ReleaseType").as_deref(), Some("Update" | "Hotfix" | "Security Update")) {
        return None;
    }
    // O UninstallString abre o desinstalador normal (com janela), de propósito.
    let uninstall = get("UninstallString")?;
    let install_date = get("InstallDate").filter(|d| d.len() == 8 && d.chars().all(|c| c.is_ascii_digit())).map(|d| {
        format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])
    });
    Some(Program {
        id,
        name,
        publisher: get("Publisher").unwrap_or_default(),
        version: get("DisplayVersion").unwrap_or_default(),
        install_date,
        size: estimated_kb.filter(|kb| *kb > 0).map(|kb| u64::from(kb) * 1024),
        install_location: get("InstallLocation").map(|l| l.trim_matches('"').trim_end_matches('\\').to_string()),
        uninstall,
    })
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(dir) else { return 0 };
    read.flatten()
        .filter_map(|e| {
            let m = std::fs::symlink_metadata(e.path()).ok()?;
            Some(if m.file_type().is_symlink() {
                0
            } else if m.is_dir() {
                dir_size(&e.path())
            } else {
                m.len()
            })
        })
        .sum()
}

/// Pastas que nunca são tratadas como pasta de um programa.
fn is_protected_dir(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    let s = s.trim_end_matches('\\');
    let parents = [
        "c:", "c:\\windows", "c:\\program files", "c:\\program files (x86)", "c:\\programdata", "c:\\users",
    ];
    parents.contains(&s) || p.parent().is_none() || s.ends_with("\\appdata\\local") || s.ends_with("\\appdata\\roaming")
}

pub fn list() -> Result<Vec<Program>, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let mut out: Vec<Program> = Vec::new();
    for (i, (hive, path)) in ORIGINS.iter().enumerate() {
        for key in reg::subkeys(*hive, path).unwrap_or_default() {
            let full = format!(r"{path}\{key}");
            let values: HashMap<String, String> = reg::string_values(*hive, &full).unwrap_or_default().into_iter().collect();
            let est = reg::get_dword(*hive, &full, "EstimatedSize").ok().flatten();
            let system = reg::get_dword(*hive, &full, "SystemComponent").ok().flatten() == Some(1);
            if let Some(mut p) = from_values(format!("{i}|{key}"), &values, est, system) {
                if p.size.is_none() {
                    if let Some(loc) = p.install_location.as_deref().map(Path::new).filter(|l| l.is_dir() && !is_protected_dir(l)) {
                        p.size = Some(dir_size(loc)).filter(|s| *s > 0);
                    }
                }
                // O mesmo programa pode aparecer em 32 e 64 bits.
                if !out.iter().any(|o| o.name == p.name && o.version == p.version) {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by(|a, b| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Separa o executável dos argumentos de um UninstallString.
pub fn split_command(cmd: &str) -> (String, String) {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
        }
    }
    let lower = cmd.to_lowercase();
    if let Some(i) = lower.find(".exe") {
        let end = i + 4;
        return (cmd[..end].to_string(), cmd[end..].trim().to_string());
    }
    match cmd.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (cmd.to_string(), String::new()),
    }
}

/// `MsiExec.exe /I{GUID}` abre o "modificar"; para desinstalar é `/X`.
pub fn fix_msiexec(program: &str, args: &str) -> String {
    if program.to_lowercase().ends_with("msiexec.exe") || program.eq_ignore_ascii_case("msiexec") {
        let a = args.trim();
        if let Some(rest) = a.strip_prefix("/I").or_else(|| a.strip_prefix("/i")) {
            return format!("/X{rest}");
        }
    }
    args.to_string()
}

/// Abre o desinstalador do programa (o Windows pede elevação se precisar).
/// Não espera terminar: muitos desinstaladores se copiam e saem na hora.
pub fn uninstall(id: &str) -> Result<Program, String> {
    let p = list()?.into_iter().find(|p| p.id == id).ok_or("o programa não está mais instalado")?;
    let (program, args) = split_command(&p.uninstall);
    let args = fix_msiexec(&program, &args);
    let q = |s: &str| s.replace('\'', "''");
    let script = if args.is_empty() {
        format!("Start-Process -FilePath '{}'", q(&program))
    } else {
        format!("Start-Process -FilePath '{}' -ArgumentList '{}'", q(&program), q(&args))
    };
    crate::shell::powershell(&format!("$ErrorActionPreference = 'Stop'; {script}"))
        .map_err(|_| "o desinstalador não abriu (a elevação pode ter sido cancelada)".to_string())?;
    Ok(p)
}

/// Pastas que costumam sobrar: a pasta de instalação e pastas com o nome do
/// programa em AppData e ProgramData. Só nomes exatos, para não pegar pastas
/// de outros programas.
pub fn leftovers(p: &Program) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(loc) = &p.install_location {
        candidates.push(PathBuf::from(loc));
    }
    let base_name = p
        .name
        .split(|c: char| c == '(' || c.is_ascii_digit())
        .next()
        .unwrap_or(&p.name)
        .trim()
        .trim_end_matches([' ', '-', 'v', 'V'])
        .trim()
        .to_string();
    let names: Vec<String> = [p.name.clone(), base_name].into_iter().filter(|n| n.len() >= 3).collect();
    for var in ["APPDATA", "LOCALAPPDATA", "ProgramData"] {
        let Some(root) = std::env::var_os(var).map(PathBuf::from) else { continue };
        for n in &names {
            candidates.push(root.join(n));
            if !p.publisher.is_empty() {
                candidates.push(root.join(&p.publisher).join(n));
            }
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for c in candidates {
        if c.is_dir() && !is_protected_dir(&c) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn builds_program_and_skips_updates_and_components() {
        let v = values(&[
            ("DisplayName", "Spotify"),
            ("Publisher", "Spotify AB"),
            ("DisplayVersion", "1.2.3"),
            ("InstallDate", "20251102"),
            ("InstallLocation", "\"C:\\Users\\voce\\AppData\\Roaming\\Spotify\\\""),
            ("UninstallString", "\"C:\\Users\\voce\\AppData\\Roaming\\Spotify\\Spotify.exe\" /uninstall"),
        ]);
        let p = from_values("2|Spotify".into(), &v, Some(300_000), false).unwrap();
        assert_eq!(p.install_date.as_deref(), Some("2025-11-02"));
        assert_eq!(p.size, Some(300_000 * 1024));
        assert_eq!(p.install_location.as_deref(), Some("C:\\Users\\voce\\AppData\\Roaming\\Spotify"));

        assert!(from_values("x".into(), &v, None, true).is_none(), "SystemComponent");
        let mut upd = v.clone();
        upd.insert("ParentKeyName".into(), "Office".into());
        assert!(from_values("x".into(), &upd, None, false).is_none());
        assert!(from_values("x".into(), &values(&[("UninstallString", "a.exe")]), None, false).is_none());
    }

    #[test]
    fn splits_uninstall_commands() {
        assert_eq!(
            split_command("\"C:\\Program Files\\App\\unins000.exe\" /SILENT"),
            ("C:\\Program Files\\App\\unins000.exe".into(), "/SILENT".into())
        );
        assert_eq!(
            split_command("MsiExec.exe /I{1234-5678}"),
            ("MsiExec.exe".into(), "/I{1234-5678}".into())
        );
        assert_eq!(
            split_command("C:\\Program Files\\App\\uninstall.exe --remove"),
            ("C:\\Program Files\\App\\uninstall.exe".into(), "--remove".into())
        );
        assert_eq!(fix_msiexec("MsiExec.exe", "/I{1234}"), "/X{1234}");
        assert_eq!(fix_msiexec("unins000.exe", "/I"), "/I");
    }

    #[test]
    fn protected_dirs_are_never_leftovers() {
        assert!(is_protected_dir(Path::new("C:\\Program Files")));
        assert!(is_protected_dir(Path::new("C:\\")));
        assert!(is_protected_dir(Path::new("C:\\Users\\voce\\AppData\\Local")));
        assert!(!is_protected_dir(Path::new("C:\\Program Files\\Spotify")));
    }
}
