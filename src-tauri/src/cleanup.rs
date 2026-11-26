//! Automatic cleanup: known folders and files that Windows and common apps
//! recreate on their own (temporary files, caches, crash dumps, leftovers of
//! updates and installers…).
//!
//! The list is fixed in code and built from environment variables, so the
//! frontend only ever sends category ids, never paths.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;
use serde::Serialize;

/// Temporary files newer than this are kept: an installer that is running
/// right now may still need them.
const TEMP_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Recreated automatically; nothing is lost.
    Safe,
    /// Safe in general, but has a cost (re-download, slower first start, no
    /// way back to the previous Windows version…). Off by default.
    Review,
}

#[derive(Clone)]
enum Target {
    /// Deletes everything inside the folder and keeps the folder itself.
    Contents { dir: PathBuf, min_age: Option<Duration> },
    /// Deletes the folder itself. `take_ownership` runs takeown/icacls first
    /// (folders owned by TrustedInstaller, e.g. Windows.old).
    WholeDir { dir: PathBuf, take_ownership: bool },
    /// Files directly inside `dir` whose lower-case name matches `pred`.
    Files { dir: PathBuf, pred: fn(&str) -> bool },
    File(PathBuf),
    RecycleBin,
    /// `Dism /Online /Cleanup-Image /StartComponentCleanup` (WinSxS).
    Dism,
}

impl Target {
    fn path(&self) -> Option<&Path> {
        match self {
            Target::Contents { dir, .. } | Target::WholeDir { dir, .. } | Target::Files { dir, .. } => Some(dir),
            Target::File(p) => Some(p),
            Target::RecycleBin | Target::Dism => None,
        }
    }

    fn exists(&self) -> bool {
        match self {
            Target::RecycleBin | Target::Dism => cfg!(windows),
            _ => self.path().is_some_and(|p| p.exists()),
        }
    }
}

struct Def {
    id: &'static str,
    group: &'static str,
    title: &'static str,
    detail: &'static str,
    level: Level,
    admin: bool,
    /// Services stopped while cleaning (they keep the files open) and
    /// started again afterwards.
    services: &'static [&'static str],
    targets: Vec<Target>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    id: &'static str,
    group: &'static str,
    title: &'static str,
    detail: &'static str,
    level: Level,
    needs_admin: bool,
    /// None when the size cannot be known in advance (DISM).
    size: Option<u64>,
    files: u64,
    paths: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    is_admin: bool,
    categories: Vec<CategoryInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResult {
    id: String,
    pub title: &'static str,
    pub freed: u64,
    files: u64,
    /// Items that could not be deleted (in use, no permission…).
    failed: u64,
    error: Option<String>,
}

// ---------------------------------------------------------------- catalogue

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT (junctions)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Extra guard against a broken environment variable pointing the cleanup at
/// a drive root, the Windows folder or the user profile.
fn sane(dir: &Path) -> bool {
    if !dir.is_absolute() {
        return false;
    }
    // Reject links in any component, including junctions in a parent folder.
    if dir.ancestors().any(|p| std::fs::symlink_metadata(p).map_or(true, |m| is_link(&m))) {
        return false;
    }
    let Ok(resolved) = dir.canonicalize() else { return false };
    if resolved.components().count() < 3 {
        return false;
    }
    let lower = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    let d = lower(&resolved);
    ["SystemRoot", "USERPROFILE", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "LOCALAPPDATA", "APPDATA"]
        .iter()
        .filter_map(|v| env_path(v))
        .filter_map(|p| p.canonicalize().ok())
        .all(|p| {
            let protected = lower(&p);
            protected != d && !protected.starts_with(&(d.clone() + std::path::MAIN_SEPARATOR_STR))
        })
}

fn is_temp_dir(dir: &Path) -> bool {
    dir.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .is_some_and(|n| n == "temp" || n == "tmp")
}

/// Cache folders of a Chromium browser, for every profile in `user_data`.
fn chromium_caches(user_data: PathBuf, out: &mut Vec<Target>) {
    const PER_PROFILE: [&str; 5] = [
        "Cache",
        "Code Cache",
        "GPUCache",
        r"Service Worker\CacheStorage",
        r"Service Worker\ScriptCache",
    ];
    const SHARED: [&str; 3] = ["ShaderCache", "GrShaderCache", "GraphiteDawnCache"];
    for s in SHARED {
        out.push(contents(user_data.join(s)));
    }
    let Ok(entries) = std::fs::read_dir(&user_data) else { return };
    for e in entries.flatten() {
        let p = e.path();
        // A profile folder has a Preferences file (Default, Profile 1, …).
        if p.join("Preferences").is_file() {
            for s in PER_PROFILE {
                out.push(contents(p.join(s)));
            }
        }
    }
    // Opera keeps the profile directly in "Opera Stable".
    for s in PER_PROFILE {
        out.push(contents(user_data.join(s)));
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

fn contents(dir: PathBuf) -> Target {
    Target::Contents { dir, min_age: None }
}

fn build_catalogue() -> Vec<Def> {
    let local = env_path("LOCALAPPDATA");
    let roaming = env_path("APPDATA");
    let profile = env_path("USERPROFILE");
    let windir = env_path("SystemRoot").or_else(|| env_path("windir"));
    let programdata = env_path("ProgramData");
    let pf = env_path("ProgramFiles");
    let pf86 = env_path("ProgramFiles(x86)");
    let sysdrive = env_path("SystemDrive").map(|d| PathBuf::from(format!("{}\\", d.display())));

    let j = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel));
    let c = |base: &Option<PathBuf>, rel: &str| j(base, rel).map(contents);

    let mut defs = Vec::new();

    // ---- System
    let mut user_temp: Vec<PathBuf> = env_path("TEMP").into_iter().chain(j(&local, "Temp")).collect();
    user_temp.dedup_by(|a, b| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()));
    defs.push(Def {
        id: "user-temp",
        group: "Sistema",
        title: "Arquivos temporários do usuário",
        detail: "Pasta %TEMP%. Arquivos das últimas 24 horas são mantidos.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: user_temp
            .into_iter()
            .filter(|d| is_temp_dir(d))
            .map(|dir| Target::Contents { dir, min_age: Some(TEMP_MIN_AGE) })
            .collect(),
    });
    defs.push(Def {
        id: "windows-temp",
        group: "Sistema",
        title: "Arquivos temporários do Windows",
        detail: r"Pasta C:\Windows\Temp. Arquivos das últimas 24 horas são mantidos.",
        level: Level::Safe,
        admin: true,
        services: &[],
        targets: j(&windir, "Temp")
            .map(|dir| Target::Contents { dir, min_age: Some(TEMP_MIN_AGE) })
            .into_iter()
            .collect(),
    });
    defs.push(Def {
        id: "windows-update",
        group: "Sistema",
        title: "Cache do Windows Update",
        detail: "Atualizações já baixadas/instaladas. O serviço Windows Update é parado durante a limpeza.",
        level: Level::Safe,
        admin: true,
        services: &["wuauserv", "bits"],
        targets: c(&windir, r"SoftwareDistribution\Download").into_iter().collect(),
    });
    defs.push(Def {
        id: "delivery-optimization",
        group: "Sistema",
        title: "Otimização de Entrega",
        detail: "Cópias de atualizações compartilhadas com outros PCs.",
        level: Level::Safe,
        admin: true,
        services: &["dosvc"],
        targets: c(
            &windir,
            r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
        )
        .into_iter()
        .collect(),
    });
    defs.push(Def {
        id: "crash-dumps",
        group: "Sistema",
        title: "Dumps de erro e relatórios de falha",
        detail: "Relatórios de travamento (WER), minidumps, MEMORY.DMP e LiveKernelReports.",
        level: Level::Safe,
        admin: true,
        services: &[],
        targets: [
            c(&local, "CrashDumps"),
            c(&local, r"Microsoft\Windows\WER\ReportArchive"),
            c(&local, r"Microsoft\Windows\WER\ReportQueue"),
            c(&programdata, r"Microsoft\Windows\WER\ReportArchive"),
            c(&programdata, r"Microsoft\Windows\WER\ReportQueue"),
            c(&programdata, r"Microsoft\Windows\WER\Temp"),
            c(&windir, "Minidump"),
            c(&windir, "LiveKernelReports"),
            j(&windir, "MEMORY.DMP").map(Target::File),
        ]
        .into_iter()
        .flatten()
        .collect(),
    });
    defs.push(Def {
        id: "windows-logs",
        group: "Sistema",
        title: "Logs do Windows",
        detail: "Logs de instalação e de manutenção (CBS, DISM, Panther).",
        level: Level::Safe,
        admin: true,
        services: &[],
        targets: [c(&windir, r"Logs\CBS"), c(&windir, r"Logs\DISM"), c(&windir, r"Panther")]
            .into_iter()
            .flatten()
            .collect(),
    });
    defs.push(Def {
        id: "thumbnails",
        group: "Sistema",
        title: "Cache de miniaturas e ícones",
        detail: "Recriado pelo Explorer. Arquivos em uso pelo Explorer são ignorados.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: j(&local, r"Microsoft\Windows\Explorer")
            .map(|dir| Target::Files {
                dir,
                pred: |n| (n.starts_with("thumbcache_") || n.starts_with("iconcache_")) && n.ends_with(".db"),
            })
            .into_iter()
            .collect(),
    });
    defs.push(Def {
        id: "recycle-bin",
        group: "Sistema",
        title: "Lixeira",
        detail: "Esvazia a Lixeira de todas as partições.",
        level: Level::Review,
        admin: false,
        services: &[],
        targets: vec![Target::RecycleBin],
    });
    defs.push(Def {
        id: "old-windows",
        group: "Sistema",
        title: "Instalações anteriores do Windows",
        detail: "Windows.old e restos de atualizações ($WINDOWS.~BT, $WINDOWS.~WS…). Depois disso não é mais \
                 possível voltar para a versão anterior do Windows.",
        level: Level::Review,
        admin: true,
        services: &[],
        targets: ["Windows.old", "$WINDOWS.~BT", "$WINDOWS.~WS", "$GetCurrent", "ESD"]
            .iter()
            .filter_map(|n| j(&sysdrive, n))
            .map(|dir| Target::WholeDir { dir, take_ownership: true })
            .collect(),
    });
    defs.push(Def {
        id: "dism",
        group: "Sistema",
        title: "Limpeza de componentes (WinSxS)",
        detail: "Roda 'Dism /Online /Cleanup-Image /StartComponentCleanup', que remove versões antigas de \
                 componentes. Pode levar vários minutos.",
        level: Level::Review,
        admin: true,
        services: &[],
        targets: vec![Target::Dism],
    });

    // ---- Browsers
    let mut browsers = Vec::new();
    for ud in [
        j(&local, r"Google\Chrome\User Data"),
        j(&local, r"Microsoft\Edge\User Data"),
        j(&local, r"BraveSoftware\Brave-Browser\User Data"),
        j(&local, r"Vivaldi\User Data"),
        j(&local, r"Chromium\User Data"),
        j(&local, r"Opera Software\Opera Stable"),
        j(&local, r"Opera Software\Opera GX Stable"),
        j(&roaming, r"Opera Software\Opera Stable"),
        j(&roaming, r"Opera Software\Opera GX Stable"),
    ]
    .into_iter()
    .flatten()
    {
        chromium_caches(ud, &mut browsers);
    }
    if let Some(ff) = j(&local, r"Mozilla\Firefox\Profiles") {
        for p in subdirs(&ff) {
            browsers.extend(["cache2", "startupCache", "thumbnails"].map(|s| contents(p.join(s))));
        }
    }
    defs.push(Def {
        id: "browsers",
        group: "Navegadores",
        title: "Cache dos navegadores",
        detail: "Chrome, Edge, Brave, Opera, Vivaldi e Firefox. Histórico, senhas e logins são mantidos; \
                 feche os navegadores antes para limpar tudo.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: browsers,
    });

    // ---- Apps
    let mut apps = Vec::new();
    for app in ["discord", "Code", "Slack", r"Microsoft\Teams", "Postman", "Notion", "WhatsApp"] {
        if let Some(base) = j(&roaming, app) {
            for s in ["Cache", "Code Cache", "GPUCache", "CachedData", r"Service Worker\CacheStorage", "logs"] {
                apps.push(contents(base.join(s)));
            }
        }
    }
    apps.extend(
        [
            c(&roaming, r"Code\CachedExtensionVSIXs"),
            c(&local, r"Spotify\Data"),
            c(&local, r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\Default\Cache"),
            c(&local, r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\Default\Code Cache"),
        ]
        .into_iter()
        .flatten(),
    );
    defs.push(Def {
        id: "app-caches",
        group: "Aplicativos",
        title: "Cache de aplicativos",
        detail: "Discord, VS Code, Slack, Teams, Spotify e outros. Feche os aplicativos antes para limpar tudo.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: apps,
    });
    defs.push(Def {
        id: "shader-caches",
        group: "Aplicativos",
        title: "Cache de shaders (DirectX, NVIDIA, AMD, Steam)",
        detail: "Recriado pelos jogos; o primeiro carregamento depois da limpeza pode ser mais lento.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: [
            c(&local, "D3DSCache"),
            c(&local, r"NVIDIA\DXCache"),
            c(&local, r"NVIDIA\GLCache"),
            c(&local, r"NVIDIA\OptixCache"),
            c(&local, r"AMD\DxCache"),
            c(&local, r"AMD\DxcCache"),
            c(&local, r"AMD\GLCache"),
            c(&local, r"AMD\VkCache"),
            c(&local, r"Intel\ShaderCache"),
            c(&pf86, r"Steam\steamapps\shadercache"),
            c(&pf86, r"Steam\appcache\httpcache"),
        ]
        .into_iter()
        .flatten()
        .collect(),
    });

    // ---- Installers
    defs.push(Def {
        id: "driver-installers",
        group: "Instaladores",
        title: "Restos de instaladores de drivers",
        detail: r"Pacotes extraídos pelos instaladores da NVIDIA, AMD e Intel (C:\AMD, Installer2, Downloader…).",
        level: Level::Safe,
        admin: true,
        services: &[],
        targets: [
            c(&pf, r"NVIDIA Corporation\Installer2"),
            c(&programdata, r"NVIDIA Corporation\Downloader"),
            c(&programdata, r"NVIDIA\Updatus"),
            j(&sysdrive, "AMD").map(contents),
            j(&sysdrive, "NVIDIA").map(contents),
            j(&sysdrive, "Intel").map(contents),
        ]
        .into_iter()
        .flatten()
        .collect(),
    });
    defs.push(Def {
        id: "downloaded-installers",
        group: "Instaladores",
        title: "Instaladores na pasta Downloads",
        detail: "Arquivos .exe, .msi, .msix e .appx da pasta Downloads. Revise: você pode querer guardar algum.",
        level: Level::Review,
        admin: false,
        services: &[],
        targets: j(&profile, "Downloads")
            .map(|dir| Target::Files {
                dir,
                pred: |n| [".exe", ".msi", ".msix", ".msixbundle", ".appx", ".appxbundle"].iter().any(|e| n.ends_with(e)),
            })
            .into_iter()
            .collect(),
    });

    // ---- Development
    defs.push(Def {
        id: "dev-caches",
        group: "Desenvolvimento",
        title: "Cache de gerenciadores de pacotes",
        detail: "npm, Yarn, pnpm, pip, NuGet (cache HTTP), Go e Cargo. Os pacotes são baixados de novo quando necessário.",
        level: Level::Safe,
        admin: false,
        services: &[],
        targets: [
            c(&local, "npm-cache"),
            c(&roaming, "npm-cache"),
            c(&local, r"Yarn\Cache"),
            c(&local, r"pnpm-cache"),
            c(&local, r"pip\Cache"),
            c(&local, r"NuGet\v3-cache"),
            c(&local, r"NuGet\plugins-cache"),
            c(&local, "go-build"),
            c(&profile, r".cargo\registry\cache"),
        ]
        .into_iter()
        .flatten()
        .collect(),
    });
    defs.push(Def {
        id: "dev-packages",
        group: "Desenvolvimento",
        title: "Pacotes baixados (Gradle, NuGet, Maven, pnpm)",
        detail: "Repositórios locais de dependências. Os projetos baixam tudo de novo no próximo build (pode demorar).",
        level: Level::Review,
        admin: false,
        services: &[],
        targets: [
            c(&profile, r".gradle\caches"),
            c(&profile, r".nuget\packages"),
            c(&profile, r".m2\repository"),
            c(&local, r"pnpm\store"),
        ]
        .into_iter()
        .flatten()
        .collect(),
    });

    for d in &mut defs {
        d.targets.retain(|t| t.path().is_none_or(sane) && t.exists());
    }
    defs.retain(|d| !d.targets.is_empty());
    defs
}

// ---------------------------------------------------------------- walk

#[derive(Default)]
struct Tally {
    bytes: u64,
    files: u64,
    failed: u64,
}

fn old_enough(meta: &std::fs::Metadata, min_age: Option<Duration>) -> bool {
    let Some(min) = min_age else { return true };
    let newest = [meta.modified().ok(), meta.created().ok()].into_iter().flatten().max();
    match newest.and_then(|t| SystemTime::now().duration_since(t).ok()) {
        Some(age) => age >= min,
        None => false,
    }
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Read-only files cannot be deleted on Windows.
            let mut perm = std::fs::symlink_metadata(path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            std::fs::set_permissions(path, perm)?;
            std::fs::remove_file(path)
        }
        r => r,
    }
}

fn handle_file(path: &Path, meta: &std::fs::Metadata, min_age: Option<Duration>, delete: bool, t: &mut Tally) {
    if !old_enough(meta, min_age) {
        return;
    }
    if !delete || remove_file(path).is_ok() {
        t.bytes += meta.len();
        t.files += 1;
    } else {
        t.failed += 1;
    }
}

/// Walks (and, with `delete`, empties) `dir`. Links are removed without
/// following them.
fn walk(dir: &Path, min_age: Option<Duration>, delete: bool, t: &mut Tally) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        t.failed += 1;
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            t.failed += 1;
            continue;
        };
        let ft = meta.file_type();
        if is_link(&meta) {
            if delete && std::fs::remove_dir(&path).or_else(|_| std::fs::remove_file(&path)).is_err() {
                t.failed += 1;
            }
        } else if ft.is_dir() {
            walk(&path, min_age, delete, t);
            if delete {
                // Fails (and is kept) when something inside could not be deleted.
                let _ = std::fs::remove_dir(&path);
            }
        } else {
            handle_file(&path, &meta, min_age, delete, t);
        }
    }
}

fn process(target: &Target, delete: bool, t: &mut Tally) -> Result<(), String> {
    if target.path().is_some_and(|p| !sane(p)) {
        return Err("caminho de limpeza inseguro".into());
    }
    match target {
        Target::Contents { dir, min_age } => walk(dir, *min_age, delete, t),
        Target::WholeDir { dir, take_ownership } => {
            if delete && *take_ownership {
                win::take_ownership(dir);
            }
            walk(dir, None, delete, t);
            if delete && std::fs::remove_dir(dir).is_err() && t.failed == 0 {
                t.failed += 1;
            }
        }
        Target::Files { dir, pred } => {
            let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                let path = e.path();
                match std::fs::symlink_metadata(&path) {
                    Ok(m) if m.is_file() && pred(&name) => handle_file(&path, &m, None, delete, t),
                    _ => {}
                }
            }
        }
        Target::File(p) => {
            if let Ok(m) = std::fs::symlink_metadata(p) {
                handle_file(p, &m, None, delete, t);
            }
        }
        Target::RecycleBin => {
            let (size, items) = win::recycle_bin_size();
            if delete && items > 0 {
                win::empty_recycle_bin()?;
            }
            t.bytes += size;
            t.files += items;
        }
        Target::Dism => {
            if delete {
                let before = win::free_space_of_system_drive();
                win::run_dism()?;
                let after = win::free_space_of_system_drive();
                t.bytes += after.saturating_sub(before);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- API

pub fn analyze() -> CleanupReport {
    let categories = build_catalogue()
        .into_par_iter()
        .map(|d| {
            let mut t = Tally::default();
            for target in &d.targets {
                let _ = process(target, false, &mut t);
            }
            let measurable = !d.targets.iter().any(|t| matches!(t, Target::Dism));
            CategoryInfo {
                id: d.id,
                group: d.group,
                title: d.title,
                detail: d.detail,
                level: d.level,
                needs_admin: d.admin,
                size: measurable.then_some(t.bytes),
                files: t.files,
                paths: d
                    .targets
                    .iter()
                    .map(|t| match t {
                        Target::RecycleBin => "Lixeira".to_string(),
                        Target::Dism => "Dism.exe".to_string(),
                        t => t.path().map(|p| p.display().to_string()).unwrap_or_default(),
                    })
                    .collect(),
            }
        })
        .collect();
    CleanupReport {
        is_admin: win::is_admin(),
        categories,
    }
}

/// Cleans the given categories in order. `on_start(index, title)` is called
/// before each one. Returns the results and the paths that were touched, so
/// the scan tree can be updated.
pub fn run(ids: &[String], mut on_start: impl FnMut(usize, &str)) -> (Vec<CategoryResult>, Vec<PathBuf>) {
    let defs = build_catalogue();
    let selected: Vec<&Def> = ids.iter().filter_map(|id| defs.iter().find(|d| d.id == id)).collect();
    let mut results = Vec::new();
    let mut touched = Vec::new();
    for (i, d) in selected.iter().enumerate() {
        on_start(i, d.title);
        let stopped: Vec<&str> = d.services.iter().copied().filter(|s| win::stop_service(s)).collect();
        let mut t = Tally::default();
        let mut error = None;
        for target in &d.targets {
            if let Err(e) = process(target, true, &mut t) {
                error = Some(e);
            }
            match target {
                Target::Files { dir, .. } => touched.push(dir.clone()),
                Target::RecycleBin => touched.extend(win::recycle_bin_dirs()),
                t => touched.extend(t.path().map(Path::to_path_buf)),
            }
        }
        for s in stopped {
            win::start_service(s);
        }
        results.push(CategoryResult {
            id: d.id.to_string(),
            title: d.title,
            freed: t.bytes,
            files: t.files,
            failed: t.failed,
            error,
        });
    }
    (results, touched)
}

/// Ids the scheduled cleanup runs: only "safe" categories with something to
/// clean that this process can reach (system folders only when elevated).
pub fn auto_ids() -> Vec<String> {
    let report = analyze();
    let admin = report.is_admin;
    report
        .categories
        .into_iter()
        .filter(|c| c.level == Level::Safe && (admin || !c.needs_admin) && c.size.is_some_and(|s| s > 0))
        .map(|c| c.id.to_string())
        .collect()
}

/// Whether the app runs elevated (administrator).
pub fn is_admin() -> bool {
    win::is_admin()
}

/// Starts a new elevated instance of the app (UAC prompt).
pub fn relaunch_elevated() -> Result<(), String> {
    win::relaunch_elevated()
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::Shell::{
        IsUserAnAdmin, SHEmptyRecycleBinW, SHQueryRecycleBinW, ShellExecuteW, SHERB_NOCONFIRMATION,
        SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn cmd(program: &str) -> Command {
        let mut c = Command::new(program);
        c.creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        c
    }

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    pub fn relaunch_elevated() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let (verb, file) = (wide("runas".as_ref()), wide(exe.as_os_str()));
        const SW_SHOWNORMAL: i32 = 1;
        let h = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // Values up to 32 are errors (e.g. the user declined the UAC prompt).
        if h as isize <= 32 {
            return Err("não foi possível reiniciar como administrador".into());
        }
        Ok(())
    }

    pub fn recycle_bin_size() -> (u64, u64) {
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            i64Size: 0,
            i64NumItems: 0,
        };
        let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        if hr < 0 {
            (0, 0)
        } else {
            (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64)
        }
    }

    pub fn empty_recycle_bin() -> Result<(), String> {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
        if hr < 0 && recycle_bin_size().1 > 0 {
            return Err(format!("não foi possível esvaziar a Lixeira (0x{hr:08X})"));
        }
        Ok(())
    }

    pub fn recycle_bin_dirs() -> Vec<PathBuf> {
        (b'A'..=b'Z')
            .map(|l| PathBuf::from(format!("{}:\\$Recycle.Bin", l as char)))
            .filter(|p| p.exists())
            .collect()
    }

    /// Returns true when the service was running and has been stopped.
    pub fn stop_service(name: &str) -> bool {
        cmd("net").args(["stop", name]).status().is_ok_and(|s| s.success())
    }

    pub fn start_service(name: &str) {
        let _ = cmd("net").args(["start", name]).status();
    }

    pub fn take_ownership(dir: &Path) {
        let d = dir.as_os_str();
        let _ = cmd("takeown").arg("/F").arg(d).args(["/R", "/A"]).status();
        // *S-1-5-32-544 = Administrators, whatever the system language.
        let _ = cmd("icacls").arg(d).args(["/grant", "*S-1-5-32-544:F", "/T", "/C", "/Q"]).status();
        let _ = cmd("attrib").args(["-R", "-S", "-H"]).arg(dir.join("*")).args(["/S", "/D"]).status();
    }

    pub fn run_dism() -> Result<(), String> {
        let status = cmd("Dism.exe")
            .args(["/Online", "/Cleanup-Image", "/StartComponentCleanup", "/Quiet", "/NoRestart"])
            .status()
            .map_err(|e| format!("falha ao executar o DISM: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("o DISM terminou com erro (código {})", status.code().unwrap_or(-1)))
        }
    }

    pub fn free_space_of_system_drive() -> u64 {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_lowercase();
        sysinfo::Disks::new_with_refreshed_list()
            .list()
            .iter()
            .find(|d| d.mount_point().to_string_lossy().to_lowercase().starts_with(&drive))
            .map(|d| d.available_space())
            .unwrap_or(0)
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::{Path, PathBuf};

    pub fn is_admin() -> bool {
        false
    }
    pub fn relaunch_elevated() -> Result<(), String> {
        Err("disponível apenas no Windows".into())
    }
    pub fn recycle_bin_size() -> (u64, u64) {
        (0, 0)
    }
    pub fn empty_recycle_bin() -> Result<(), String> {
        Err("disponível apenas no Windows".into())
    }
    pub fn recycle_bin_dirs() -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn stop_service(_: &str) -> bool {
        false
    }
    pub fn start_service(_: &str) {}
    pub fn take_ownership(_: &Path) {}
    pub fn run_dism() -> Result<(), String> {
        Err("disponível apenas no Windows".into())
    }
    pub fn free_space_of_system_drive() -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/b")).unwrap();
        std::fs::write(d.path().join("x.tmp"), vec![0u8; 100]).unwrap();
        std::fs::write(d.path().join("a/y.tmp"), vec![0u8; 50]).unwrap();
        std::fs::write(d.path().join("a/b/z.tmp"), vec![0u8; 10]).unwrap();
        d
    }

    #[test]
    fn measures_without_deleting() {
        let d = tree();
        let mut t = Tally::default();
        process(&contents(d.path().to_path_buf()), false, &mut t).unwrap();
        assert_eq!((t.bytes, t.files, t.failed), (160, 3, 0));
        assert!(d.path().join("a/b/z.tmp").exists());
    }

    #[test]
    fn empties_folder_but_keeps_it() {
        let d = tree();
        let mut t = Tally::default();
        process(&contents(d.path().to_path_buf()), true, &mut t).unwrap();
        assert_eq!((t.bytes, t.files, t.failed), (160, 3, 0));
        assert!(d.path().exists());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn keeps_recent_files() {
        let d = tree();
        let mut t = Tally::default();
        let target = Target::Contents { dir: d.path().to_path_buf(), min_age: Some(TEMP_MIN_AGE) };
        process(&target, true, &mut t).unwrap();
        assert_eq!(t.files, 0);
        assert!(d.path().join("a/b/z.tmp").exists());
    }

    #[test]
    fn deletes_read_only_files() {
        let d = tree();
        let f = d.path().join("x.tmp");
        let mut p = std::fs::metadata(&f).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&f, p).unwrap();
        let mut t = Tally::default();
        process(&contents(d.path().to_path_buf()), true, &mut t).unwrap();
        assert!(!f.exists());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_links() {
        let d = tree();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("keep"), b"1").unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("link")).unwrap();
        let mut t = Tally::default();
        process(&contents(d.path().to_path_buf()), true, &mut t).unwrap();
        assert!(outside.path().join("keep").exists());
        assert!(!d.path().join("link").exists());
    }

    #[cfg(windows)]
    #[test]
    fn does_not_follow_junctions() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("keep"), b"1").unwrap();
        let root = d.path().join("Temp");
        std::fs::create_dir(&root).unwrap();
        let junction = |link: &Path| {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };

        junction(&root.join("link"));
        let mut t = Tally::default();
        process(&contents(root), true, &mut t).unwrap();
        assert!(outside.path().join("keep").exists());

        let linked_root = d.path().join("TempLink");
        junction(&linked_root);
        assert!(process(&contents(linked_root), true, &mut Tally::default()).is_err());
        assert!(outside.path().join("keep").exists());
    }

    #[test]
    fn whole_dir_and_file_patterns() {
        let d = tree();
        let mut t = Tally::default();
        process(&Target::WholeDir { dir: d.path().join("a"), take_ownership: false }, true, &mut t).unwrap();
        assert!(!d.path().join("a").exists());
        std::fs::write(d.path().join("setup.EXE"), b"12").unwrap();
        let mut t = Tally::default();
        let target = Target::Files { dir: d.path().to_path_buf(), pred: |n| n.ends_with(".exe") };
        process(&target, true, &mut t).unwrap();
        assert_eq!(t.files, 1);
        assert!(d.path().join("x.tmp").exists());
    }

    #[test]
    fn refuses_roots() {
        assert!(!sane(Path::new("/")));
        assert!(!sane(Path::new("relative/path/x")));
    }
}
