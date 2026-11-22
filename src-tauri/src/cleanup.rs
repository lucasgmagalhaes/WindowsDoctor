//! Catalogue of locations Windows (and common programs) fill with
//! data, classified by what can be deleted:
//!
//! - `Safe`: temporary files and caches; the app can clean them;
//! - `Review`: deletable, but with consequences (re-download, losing
//!   the option to roll back Windows); the app cleans them only if the user ticks them;
//! - `System`: never delete by hand; we point to the official tool.
//!
//! Paths are always resolved here, in the backend: the frontend only sends
//! the ids of the chosen items, so it can never ask to clean an arbitrary path.

use std::path::PathBuf;
use std::time::Duration;

use diskscan::{clean_dir_contents, measure, CleanReport};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Safe,
    Review,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Delete the contents of `paths`.
    Clean,
    /// Empty the Recycle Bin of every drive.
    RecycleBin,
    /// Only guidance: the user acts through the indicated tool.
    Manual,
}

/// System tools the frontend may open (allow-list).
pub const TOOLS: [(&str, &str); 4] = [
    ("cleanmgr", "cleanmgr.exe"),
    ("systemProtection", "SystemPropertiesProtection.exe"),
    ("performance", "SystemPropertiesPerformance.exe"),
    ("storageSense", "ms-settings:storagesense"),
];

/// Commands that can be run as administrator (allow-list).
pub const ELEVATED: [(&str, &str); 2] = [
    ("dism", "Dism.exe /Online /Cleanup-Image /StartComponentCleanup"),
    ("hibernateOff", "powercfg.exe /h off"),
];

struct Rule {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    level: Level,
    action: Action,
    paths: Vec<PathBuf>,
    /// Files modified more recently than this are kept.
    min_age_hours: u64,
    needs_admin: bool,
    /// Whether to compute the size (WinSxS takes long and the number is misleading due to hard links).
    measure: bool,
    note: Option<&'static str>,
    tool: Option<&'static str>,
    command: Option<&'static str>,
}

impl Rule {
    fn new(id: &'static str, level: Level, action: Action, title: &'static str, description: &'static str) -> Self {
        Self {
            id,
            title,
            description,
            level,
            action,
            paths: Vec::new(),
            min_age_hours: 0,
            needs_admin: false,
            measure: true,
            note: None,
            tool: None,
            command: None,
        }
    }
    fn paths(mut self, p: impl IntoIterator<Item = Option<PathBuf>>) -> Self {
        self.paths = p.into_iter().flatten().filter(|p| p.exists()).collect();
        self.paths.dedup();
        self
    }
    fn min_age_hours(mut self, h: u64) -> Self {
        self.min_age_hours = h;
        self
    }
    fn admin(mut self) -> Self {
        self.needs_admin = true;
        self
    }
    fn no_measure(mut self) -> Self {
        self.measure = false;
        self
    }
    fn note(mut self, n: &'static str) -> Self {
        self.note = Some(n);
        self
    }
    fn tool(mut self, t: &'static str) -> Self {
        self.tool = Some(t);
        self
    }
    fn command(mut self, c: &'static str) -> Self {
        self.command = Some(c);
        self
    }
    fn is_cleanable(&self) -> bool {
        self.level != Level::System && self.action != Action::Manual
    }
}

fn env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn join(base: &Option<PathBuf>, rel: &str) -> Option<PathBuf> {
    base.as_ref().map(|b| b.join(rel))
}

/// `base\<each profile>\<sub>` — browser caches live per profile.
fn per_profile(base: Option<PathBuf>, subs: &[&str]) -> Vec<Option<PathBuf>> {
    let Some(base) = base else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&base) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            for s in subs {
                out.push(Some(e.path().join(s)));
            }
        }
    }
    out
}

fn rules() -> Vec<Rule> {
    use Action::*;
    use Level::*;

    let sys = env("SystemRoot").or_else(|| Some(PathBuf::from(r"C:\Windows")));
    let drive = env("SystemDrive")
        .map(|d| PathBuf::from(format!("{}\\", d.display())))
        .or_else(|| Some(PathBuf::from(r"C:\")));
    let local = env("LOCALAPPDATA");
    let roaming = env("APPDATA");
    let home = env("USERPROFILE");
    let pdata = env("ProgramData");
    let pf = env("ProgramFiles");

    let mut browser = Vec::new();
    for base in [
        r"Google\Chrome\User Data",
        r"Microsoft\Edge\User Data",
        r"BraveSoftware\Brave-Browser\User Data",
    ] {
        browser.extend(per_profile(join(&local, base), &["Cache", "Code Cache", "GPUCache"]));
    }
    browser.extend(per_profile(join(&local, r"Mozilla\Firefox\Profiles"), &["cache2"]));

    vec![
        // ------------------------------------------------------------ can delete
        Rule::new("userTemp", Safe, Clean, "Arquivos temporários do usuário",
            "Arquivos que instaladores e programas deixam para trás na pasta TEMP.")
            .paths([Some(std::env::temp_dir())])
            .min_age_hours(24)
            .note("Arquivos modificados nas últimas 24 horas são mantidos (podem estar em uso)."),
        Rule::new("windowsTemp", Safe, Clean, "Arquivos temporários do Windows",
            "Temporários criados pelo sistema e por serviços.")
            .paths([join(&sys, "Temp")])
            .min_age_hours(24)
            .admin(),
        Rule::new("updateCache", Safe, Clean, "Cache do Windows Update",
            "Atualizações já baixadas. O Windows baixa de novo se precisar.")
            .paths([join(&sys, r"SoftwareDistribution\Download")])
            .min_age_hours(24)
            .admin()
            .note("Evite durante uma atualização em andamento."),
        Rule::new("crashDumps", Safe, Clean, "Relatórios e dumps de erro",
            "Despejos de memória e relatórios de travamento de programas e do Windows.")
            .paths([
                join(&local, "CrashDumps"),
                join(&sys, "Minidump"),
                join(&sys, "LiveKernelReports"),
                join(&local, r"Microsoft\Windows\WER\ReportArchive"),
                join(&local, r"Microsoft\Windows\WER\ReportQueue"),
                join(&pdata, r"Microsoft\Windows\WER\ReportArchive"),
                join(&pdata, r"Microsoft\Windows\WER\ReportQueue"),
            ]),
        Rule::new("shaderCache", Safe, Clean, "Cache de shaders (placa de vídeo)",
            "Shaders compilados por jogos e pelo DirectX/OpenGL. São recriados automaticamente.")
            .paths([
                join(&local, "D3DSCache"),
                join(&local, r"NVIDIA\DXCache"),
                join(&local, r"NVIDIA\GLCache"),
                join(&local, r"AMD\DxCache"),
                join(&local, r"AMD\GLCache"),
            ])
            .note("Jogos podem demorar um pouco mais para abrir na primeira vez."),
        Rule::new("browserCache", Safe, Clean, "Cache dos navegadores",
            "Cópias de páginas e imagens do Chrome, Edge, Brave e Firefox. Não apaga senhas, histórico nem favoritos.")
            .paths(browser)
            .note("Feche os navegadores antes; arquivos em uso são ignorados."),
        Rule::new("recycleBin", Safe, RecycleBin, "Lixeira",
            "Itens já excluídos. Esvaziar libera o espaço de verdade.")
            .note("Não é possível recuperar os itens depois."),
        // ------------------------------------------------------------ review
        Rule::new("devCaches", Review, Clean, "Caches de ferramentas de programação",
            "Pacotes baixados por npm, Yarn, pip, NuGet, Gradle, Maven e Cargo.")
            .paths([
                join(&local, "npm-cache"),
                join(&roaming, "npm-cache"),
                join(&local, r"Yarn\Cache"),
                join(&local, r"pip\Cache"),
                join(&home, r".nuget\packages"),
                join(&home, r".gradle\caches"),
                join(&home, r".m2\repository"),
                join(&home, r".cargo\registry\cache"),
            ])
            .note("Serão baixados de novo (exige internet) na próxima compilação dos projetos."),
        Rule::new("nvidiaInstaller", Review, Clean, "Instaladores antigos de driver NVIDIA",
            "Cópias dos instaladores de drivers de vídeo já instalados.")
            .paths([join(&pf, r"NVIDIA Corporation\Installer2")])
            .admin(),
        Rule::new("downloads", Review, Manual, "Pasta Downloads",
            "Costuma acumular instaladores, ISOs e arquivos esquecidos. Só você sabe o que ainda é útil.")
            .paths([join(&home, "Downloads")])
            .note("Revise e apague pela aba Explorar."),
        Rule::new("windowsOld", Review, Manual, "Instalação anterior do Windows (Windows.old)",
            "Sobra de uma atualização do Windows. Removê-la elimina a opção de voltar à versão anterior.")
            .paths([join(&drive, "Windows.old")])
            .tool("cleanmgr")
            .note("Na Limpeza de Disco, clique em \"Limpar arquivos do sistema\" e marque \"Instalações anteriores do Windows\"."),
        Rule::new("hibernation", Review, Manual, "Arquivo de hibernação (hiberfil.sys)",
            "Usado pela hibernação e pela Inicialização Rápida. Só desative se não usa esses recursos.")
            .paths([join(&drive, "hiberfil.sys")])
            .command("hibernateOff"),
        // ------------------------------------------------------------ do not delete
        Rule::new("winsxs", System, Manual, "Componentes do Windows (WinSxS)",
            "Bibliotecas usadas pelo sistema. Apagar à mão quebra o Windows. O DISM remove versões antigas com segurança.")
            .paths([join(&sys, "WinSxS")])
            .no_measure()
            .command("dism")
            .note("O tamanho mostrado pelo Explorer é exagerado: muitos arquivos são links para outras pastas do sistema."),
        Rule::new("installer", System, Manual, "Instaladores de programas (Windows\\Installer)",
            "O Windows usa estes arquivos para reparar, atualizar e desinstalar programas. Nunca apague.")
            .paths([join(&sys, "Installer")]),
        Rule::new("pagefile", System, Manual, "Memória virtual (pagefile.sys / swapfile.sys)",
            "Extensão da memória RAM no disco. Apagar não é possível; o tamanho pode ser ajustado.")
            .paths([join(&drive, "pagefile.sys"), join(&drive, "swapfile.sys")])
            .tool("performance")
            .note("Em Desempenho > Avançado > Memória virtual."),
        Rule::new("restorePoints", System, Manual, "Pontos de restauração (System Volume Information)",
            "Cópias de segurança usadas para desfazer alterações no sistema.")
            .paths([join(&drive, "System Volume Information")])
            .tool("systemProtection")
            .note("Em Proteção do Sistema > Configurar é possível excluir pontos antigos ou limitar o espaço."),
        Rule::new("programFiles", System, Manual, "Programas instalados (Program Files)",
            "Apagar pastas de programas deixa restos no registro e pode quebrar outros programas.")
            .paths([pf.clone(), env("ProgramFiles(x86)")])
            .no_measure()
            .tool("storageSense")
            .note("Desinstale em Configurações > Aplicativos."),
    ]
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    level: Level,
    action: Action,
    paths: Vec<String>,
    size: Option<u64>,
    file_count: Option<u64>,
    min_age_hours: u64,
    needs_admin: bool,
    note: Option<&'static str>,
    tool: Option<&'static str>,
    command_id: Option<&'static str>,
    /// Text of the command, to show the user before running it.
    command: Option<&'static str>,
}

/// Lists the catalogue items that exist on this computer, with their size.
pub fn analyze() -> Vec<CleanupItem> {
    rules()
        .into_iter()
        .filter(|r| !r.paths.is_empty() || r.action == Action::RecycleBin)
        .filter(|r| r.action != Action::RecycleBin || recycle_bin::supported())
        .map(|r| {
            let (size, files) = if r.action == Action::RecycleBin {
                recycle_bin::query().map_or((None, None), |(s, n)| (Some(s), Some(n)))
            } else if r.measure {
                let mut total: Option<(u64, u64)> = None;
                for p in &r.paths {
                    if let Some((s, n)) = measure(p) {
                        let t = total.get_or_insert((0, 0));
                        t.0 += s;
                        t.1 += n;
                    }
                }
                (total.map(|t| t.0), total.map(|t| t.1))
            } else {
                (None, None)
            };
            CleanupItem {
                id: r.id,
                title: r.title,
                description: r.description,
                level: r.level,
                action: r.action,
                paths: r.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
                size,
                file_count: files,
                min_age_hours: r.min_age_hours,
                needs_admin: r.needs_admin,
                note: r.note,
                tool: r.tool,
                command_id: r.command,
                command: r.command.and_then(|c| ELEVATED.iter().find(|e| e.0 == c).map(|e| e.1)),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub id: String,
    pub title: String,
    pub report: CleanReport,
    pub error: Option<String>,
}

/// Runs the cleanup of the chosen items. Unknown ids and items that the
/// app does not clean (`System`/`Manual`) are rejected with an error per item.
/// Also returns the paths touched, so the caller can update the tree.
pub fn run(ids: &[String]) -> (Vec<CleanupResult>, Vec<PathBuf>) {
    let all = rules();
    let mut results = Vec::new();
    let mut touched = Vec::new();
    for id in ids {
        let Some(rule) = all.iter().find(|r| r.id == id) else {
            results.push(CleanupResult {
                id: id.clone(),
                title: id.clone(),
                report: CleanReport::default(),
                error: Some("item desconhecido".into()),
            });
            continue;
        };
        let mut res = CleanupResult {
            id: id.clone(),
            title: rule.title.into(),
            report: CleanReport::default(),
            error: None,
        };
        if !rule.is_cleanable() {
            res.error = Some("este item não é limpo automaticamente".into());
        } else if rule.action == Action::RecycleBin {
            match recycle_bin::empty() {
                Ok(freed) => res.report.freed = freed,
                Err(e) => res.error = Some(e),
            }
            touched.extend(recycle_bin::folders());
        } else {
            let min_age = Duration::from_secs(rule.min_age_hours * 3600);
            for p in &rule.paths {
                res.report += clean_dir_contents(p, min_age);
                touched.push(p.clone());
            }
        }
        results.push(res);
    }
    (results, touched)
}

pub fn tool_target(tool: &str) -> Option<&'static str> {
    TOOLS.iter().find(|t| t.0 == tool).map(|t| t.1)
}

pub fn elevated_command(id: &str) -> Option<&'static str> {
    ELEVATED.iter().find(|e| e.0 == id).map(|e| e.1)
}

#[cfg(windows)]
mod recycle_bin {
    use std::path::PathBuf;
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
        SHQUERYRBINFO,
    };

    pub fn supported() -> bool {
        true
    }

    /// (bytes, items) across every drive.
    pub fn query() -> Option<(u64, u64)> {
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            i64Size: 0,
            i64NumItems: 0,
        };
        // Null root = every drive.
        let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        (hr >= 0).then(|| (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
    }

    pub fn empty() -> Result<u64, String> {
        let before = query().map_or(0, |q| q.0);
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
        // E_UNEXPECTED is returned when the Recycle Bin is already empty.
        const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;
        if hr < 0 && hr != E_UNEXPECTED {
            return Err(format!("o Windows recusou esvaziar a Lixeira (0x{:08X})", hr as u32));
        }
        let after = query().map_or(0, |q| q.0);
        Ok(before.saturating_sub(after))
    }

    /// `X:\$Recycle.Bin` folders of the existing drives (to update the tree).
    pub fn folders() -> Vec<PathBuf> {
        (b'A'..=b'Z')
            .map(|l| PathBuf::from(format!("{}:\\$Recycle.Bin", l as char)))
            .filter(|p| p.exists())
            .collect()
    }
}

#[cfg(not(windows))]
mod recycle_bin {
    pub fn supported() -> bool {
        false
    }
    pub fn query() -> Option<(u64, u64)> {
        None
    }
    pub fn empty() -> Result<u64, String> {
        Err("disponível apenas no Windows".into())
    }
    pub fn folders() -> Vec<std::path::PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_actions_consistent() {
        let rs = rules();
        let mut ids: Vec<_> = rs.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), rs.len());
        for r in &rs {
            // Nothing from the system is cleaned automatically.
            if r.level == Level::System {
                assert!(!r.is_cleanable(), "{} must not be cleanable", r.id);
            }
            if let Some(t) = r.tool {
                assert!(tool_target(t).is_some(), "unknown tool {t}");
            }
            if let Some(c) = r.command {
                assert!(elevated_command(c).is_some(), "unknown command {c}");
            }
        }
    }

    #[test]
    fn run_rejects_unknown_and_system_items() {
        let (res, touched) = run(&["nope".into(), "winsxs".into(), "installer".into()]);
        assert!(res.iter().all(|r| r.error.is_some() && r.report.deleted == 0));
        assert!(touched.is_empty());
    }
}
