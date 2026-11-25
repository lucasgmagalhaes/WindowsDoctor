//! Limpeza de pastas temporárias e caches que o Windows e os programas recriam
//! sozinhos.
//!
//! Só o *conteúdo* das pastas de uma lista fixa é apagado; o frontend escolhe
//! os alvos pelo `id`, nunca por caminho. Arquivos em uso são pulados.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;

/// De onde vêm os arquivos de um alvo.
#[derive(Debug, Clone)]
pub enum Source {
    /// O conteúdo destas pastas (as pastas em si são mantidas).
    Folders(Vec<PathBuf>),
    /// A Lixeira de todas as unidades.
    RecycleBin,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub needs_admin: bool,
    /// Só arquivos sem modificação há pelo menos esse tempo são apagados.
    pub min_age: Option<Duration>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub needs_admin: bool,
    /// Bytes que seriam liberados.
    pub size: u64,
    /// Quantidade de arquivos (ou de itens, na Lixeira).
    pub files: u64,
    pub paths: Vec<String>,
    /// Aviso, como "pasta sem permissão de leitura".
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub id: String,
    pub freed: u64,
    pub deleted: u64,
    /// Arquivos que não puderam ser apagados (em uso, sem permissão).
    pub skipped: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub size: u64,
    pub files: u64,
    /// Pastas que não puderam ser lidas.
    pub unreadable: u64,
}

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn is_link(meta: &fs::Metadata) -> bool {
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

fn safe_path(path: &Path) -> bool {
    if !path.is_absolute()
        || path.ancestors().any(|p| fs::symlink_metadata(p).map_or(true, |m| is_link(&m)))
    {
        return false;
    }
    let Ok(resolved) = path.canonicalize() else { return false };
    if resolved.components().count() < 3 {
        return false;
    }
    let lower = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    let candidate = lower(&resolved);
    ["SystemRoot", "USERPROFILE", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "LOCALAPPDATA", "APPDATA"]
        .iter()
        .filter_map(|name| std::env::var_os(name).filter(|v| !v.is_empty()))
        .filter_map(|p| PathBuf::from(p).canonicalize().ok())
        .all(|p| {
            let protected = lower(&p);
            protected != candidate && !protected.starts_with(&(candidate.clone() + std::path::MAIN_SEPARATOR_STR))
        })
}

fn is_temp_dir(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .is_some_and(|name| name == "temp" || name == "tmp")
}

/// Os alvos de limpeza disponíveis neste computador (só pastas que existem).
pub fn targets() -> Vec<Target> {
    let mut list = platform_targets();
    for t in &mut list {
        if let Source::Folders(paths) = &mut t.source {
            dedup_paths(paths);
            paths.retain(|p| p.is_dir() && safe_path(p) && (t.id != "user_temp" || is_temp_dir(p)));
        }
    }
    list.retain(|t| match &t.source {
        Source::Folders(p) => !p.is_empty(),
        Source::RecycleBin => true,
    });
    list
}

pub fn find(id: &str) -> Option<Target> {
    targets().into_iter().find(|t| t.id == id)
}

pub fn measure(target: &Target) -> TargetInfo {
    let mut info = TargetInfo {
        id: target.id.into(),
        title: target.title.into(),
        description: target.description.into(),
        needs_admin: target.needs_admin,
        size: 0,
        files: 0,
        paths: Vec::new(),
        note: None,
    };
    match &target.source {
        Source::Folders(paths) => {
            let u = measure_dirs(paths, target.min_age, SystemTime::now());
            info.size = u.size;
            info.files = u.files;
            info.paths = paths.iter().map(|p| p.display().to_string()).collect();
            if u.unreadable > 0 {
                info.note = Some(if target.needs_admin {
                    "parte das pastas exige administrador".into()
                } else {
                    format!("{} pasta(s) sem permissão de leitura", u.unreadable)
                });
            }
        }
        Source::RecycleBin => match recycle_bin_usage() {
            Ok((size, items)) => {
                info.size = size;
                info.files = items;
            }
            Err(e) => info.note = Some(e),
        },
    }
    info
}

pub fn clean(target: &Target) -> CleanResult {
    let mut r = match &target.source {
        Source::Folders(paths) => clean_dirs(paths, target.min_age, SystemTime::now()),
        Source::RecycleBin => empty_recycle_bin(),
    };
    r.id = target.id.into();
    r
}

/// Soma os arquivos dentro de `paths` com idade mínima `min_age`.
pub fn measure_dirs(paths: &[PathBuf], min_age: Option<Duration>, now: SystemTime) -> Usage {
    let mut u = Usage::default();
    for p in paths {
        if !safe_path(p) {
            u.unreadable += 1;
            continue;
        }
        walk(p, &mut |entry| match entry {
            Entry::File(_, meta) => {
                if old_enough(&meta, min_age, now) {
                    u.size += meta.len();
                    u.files += 1;
                }
            }
            Entry::Unreadable => u.unreadable += 1,
            Entry::DirDone(_) => {}
        });
    }
    u
}

/// Apaga os arquivos dentro de `paths` com idade mínima `min_age` e as
/// subpastas que ficarem vazias. As pastas de `paths` não são apagadas.
pub fn clean_dirs(paths: &[PathBuf], min_age: Option<Duration>, now: SystemTime) -> CleanResult {
    let mut r = CleanResult::default();
    for root in paths {
        if !safe_path(root) {
            r.skipped += 1;
            continue;
        }
        walk(root, &mut |entry| match entry {
            Entry::File(path, meta) => {
                if !old_enough(&meta, min_age, now) {
                    return;
                }
                match remove_file(&path) {
                    Ok(()) => {
                        r.freed += meta.len();
                        r.deleted += 1;
                    }
                    Err(_) => r.skipped += 1,
                }
            }
            Entry::DirDone(dir) => {
                if dir != *root {
                    // Só sai se estiver vazia.
                    let _ = fs::remove_dir(&dir);
                }
            }
            Entry::Unreadable => r.skipped += 1,
        });
    }
    r
}

enum Entry {
    File(PathBuf, fs::Metadata),
    /// Chamado depois de visitar todo o conteúdo da pasta.
    DirDone(PathBuf),
    Unreadable,
}

/// Percorre a pasta sem seguir links simbólicos nem junctions.
fn walk(dir: &Path, f: &mut dyn FnMut(Entry)) {
    let Ok(read) = fs::read_dir(dir) else {
        f(Entry::Unreadable);
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let ft = meta.file_type();
        if is_link(&meta) {
            continue;
        }
        if ft.is_dir() {
            walk(&path, f);
        } else {
            f(Entry::File(path, meta));
        }
    }
    f(Entry::DirDone(dir.to_path_buf()));
}

fn old_enough(meta: &fs::Metadata, min_age: Option<Duration>, now: SystemTime) -> bool {
    let Some(min) = min_age else { return true };
    match meta.modified() {
        Ok(t) => now.duration_since(t).map(|age| age >= min).unwrap_or(false),
        Err(_) => false,
    }
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // No Windows arquivos somente leitura não podem ser apagados direto.
            let mut perm = fs::metadata(path)?.permissions();
            if !perm.readonly() {
                return Err(e);
            }
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            fs::set_permissions(path, perm)?;
            fs::remove_file(path)
        }
        other => other,
    }
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.to_string_lossy().to_lowercase()));
}

// ---------------------------------------------------------------- Lixeira

#[cfg(windows)]
fn recycle_bin_usage() -> Result<(u64, u64), String> {
    use trash::os_limited;
    let items = os_limited::list().map_err(|e| e.to_string())?;
    let size = items
        .iter()
        .filter_map(|i| os_limited::metadata(i).ok())
        .filter_map(|m| m.size.size())
        .sum();
    Ok((size, items.len() as u64))
}

#[cfg(not(windows))]
fn recycle_bin_usage() -> Result<(u64, u64), String> {
    Err(crate::WINDOWS_ONLY.into())
}

#[cfg(windows)]
fn empty_recycle_bin() -> CleanResult {
    use trash::os_limited;
    let mut r = CleanResult::default();
    match recycle_bin_usage() {
        Ok((size, items)) => {
            let list = os_limited::list().unwrap_or_default();
            match os_limited::purge_all(list) {
                Ok(()) => {
                    r.freed = size;
                    r.deleted = items;
                }
                Err(e) => r.error = Some(e.to_string()),
            }
        }
        Err(e) => r.error = Some(e),
    }
    r
}

#[cfg(not(windows))]
fn empty_recycle_bin() -> CleanResult {
    CleanResult {
        error: Some(crate::WINDOWS_ONLY.into()),
        ..CleanResult::default()
    }
}

// ---------------------------------------------------------------- alvos

#[cfg(windows)]
fn platform_targets() -> Vec<Target> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let local = env("LOCALAPPDATA").unwrap_or_default();
    let windir = env("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let program_data = env("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));

    let mut user_temp = vec![std::env::temp_dir()];
    user_temp.extend(env("TEMP"));

    let mut browsers = Vec::new();
    for app in [r"Google\Chrome", r"Microsoft\Edge", r"BraveSoftware\Brave-Browser"] {
        let data = local.join(app).join("User Data");
        for profile in subdirs(&data) {
            let name = profile.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name == "Default" || name.starts_with("Profile ") || name == "Guest Profile" {
                for cache in ["Cache", "Code Cache", "GPUCache"] {
                    browsers.push(profile.join(cache));
                }
            }
        }
    }
    for profile in subdirs(&local.join(r"Mozilla\Firefox\Profiles")) {
        browsers.push(profile.join("cache2"));
    }

    vec![
        Target {
            id: "user_temp",
            title: "Arquivos temporários do usuário",
            description: "Pasta %TEMP%. Arquivos usados nas últimas 24 horas são mantidos.",
            needs_admin: false,
            min_age: Some(DAY),
            source: Source::Folders(user_temp),
        },
        Target {
            id: "windows_temp",
            title: "Arquivos temporários do Windows",
            description: r"Pasta Windows\Temp. Arquivos usados nas últimas 24 horas são mantidos.",
            needs_admin: true,
            min_age: Some(DAY),
            source: Source::Folders(vec![windir.join("Temp")]),
        },
        Target {
            id: "windows_update",
            title: "Cache do Windows Update",
            description: "Instaladores de atualizações já baixadas. O Windows baixa de novo se precisar.",
            needs_admin: true,
            min_age: None,
            source: Source::Folders(vec![windir.join(r"SoftwareDistribution\Download")]),
        },
        Target {
            id: "delivery_optimization",
            title: "Otimização de Entrega",
            description: "Cópias de atualizações compartilhadas com outros PCs.",
            needs_admin: true,
            min_age: None,
            source: Source::Folders(vec![windir.join(
                r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
            )]),
        },
        Target {
            id: "shader_cache",
            title: "Cache de shaders da placa de vídeo",
            description: "DirectX, NVIDIA, AMD e Intel. É recriado; jogos podem engasgar um pouco na primeira vez. \
                          Limpar ajuda quando há artefatos ou travamentos após atualizar o driver.",
            needs_admin: false,
            min_age: None,
            source: Source::Folders(vec![
                local.join("D3DSCache"),
                local.join(r"NVIDIA\DXCache"),
                local.join(r"NVIDIA\GLCache"),
                local.join(r"NVIDIA Corporation\NV_Cache"),
                local.join(r"AMD\DxCache"),
                local.join(r"AMD\DxcCache"),
                local.join(r"AMD\GLCache"),
                local.join(r"AMD\VkCache"),
                local.join(r"Intel\ShaderCache"),
            ]),
        },
        Target {
            id: "browser_cache",
            title: "Cache dos navegadores",
            description: "Chrome, Edge, Brave e Firefox. Senhas, histórico e cookies não são afetados. \
                          Feche os navegadores antes para liberar mais.",
            needs_admin: false,
            min_age: None,
            source: Source::Folders(browsers),
        },
        Target {
            id: "error_reports",
            title: "Relatórios de erro e despejos de falha",
            description: "Relatórios do Windows (WER) e despejos de programas que travaram. \
                          Não inclui os minidumps de tela azul.",
            needs_admin: false,
            min_age: None,
            source: Source::Folders(vec![
                local.join("CrashDumps"),
                local.join(r"Microsoft\Windows\WER"),
                program_data.join(r"Microsoft\Windows\WER\ReportArchive"),
                program_data.join(r"Microsoft\Windows\WER\ReportQueue"),
            ]),
        },
        Target {
            id: "recycle_bin",
            title: "Lixeira",
            description: "Esvazia a Lixeira de todas as unidades. Não dá para desfazer.",
            needs_admin: false,
            min_age: None,
            source: Source::RecycleBin,
        },
    ]
}

/// Fora do Windows só existe a pasta temporária (útil para desenvolver).
#[cfg(not(windows))]
fn platform_targets() -> Vec<Target> {
    vec![Target {
        id: "user_temp",
        title: "Arquivos temporários do usuário",
        description: "Pasta temporária. Arquivos usados nas últimas 24 horas são mantidos.",
        needs_admin: false,
        min_age: Some(DAY),
        source: Source::Folders(vec![std::env::temp_dir()]),
    }]
}

#[cfg(windows)]
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}
