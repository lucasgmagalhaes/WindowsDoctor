mod cleanup;


use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use diskscan::{Crumb, FileInfo, NodeKind, NodeView, Progress, ScanOptions, ScanStats, Tree};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

mod tools;

#[derive(Default)]
struct AppState {
    tree: Mutex<Option<Tree>>,
    progress: Mutex<Option<Arc<Progress>>>,
}

type CmdResult<T> = Result<T, String>;

fn with_tree<T>(state: &AppState, f: impl FnOnce(&mut Tree) -> CmdResult<T>) -> CmdResult<T> {
    let mut guard = state.tree.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_mut().ok_or("nenhuma varredura carregada")?;
    f(tree)
}

// ---------------------------------------------------------------- partições

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DriveInfo {
    mount: String,
    label: String,
    file_system: String,
    total: u64,
    available: u64,
    removable: bool,
    kind: String,
}

#[tauri::command]
fn list_drives() -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DriveInfo {
            mount: d.mount_point().to_string_lossy().into_owned(),
            label: d.name().to_string_lossy().into_owned(),
            file_system: d.file_system().to_string_lossy().into_owned(),
            total: d.total_space(),
            available: d.available_space(),
            removable: d.is_removable(),
            kind: format!("{:?}", d.kind()),
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

// ---------------------------------------------------------------- varredura

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanFinished {
    ok: bool,
    error: Option<String>,
    stats: Option<ScanStats>,
    /// Preenchido quando foi uma re-varredura de uma subpasta.
    rescanned_path: Option<String>,
    /// A varredura foi cancelada. Numa re-varredura cancelada os dados
    /// anteriores da pasta são mantidos.
    cancelled: bool,
}

/// Inicia uma varredura em segundo plano. Emite `scan-progress` periodicamente
/// e `scan-finished` ao terminar.
///
/// Se `node_id` for informado, re-varre só aquela pasta da árvore atual.
#[tauri::command]
fn start_scan(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: Option<String>,
    node_id: Option<u32>,
    min_file_size: Option<u64>,
) -> CmdResult<()> {
    let target: PathBuf = match (node_id, path) {
        (Some(id), _) => with_tree(&state, |t| {
            if t.kind(id) != Some(NodeKind::Dir) {
                return Err("só é possível re-varrer pastas".into());
            }
            t.path_of(id).ok_or_else(|| "pasta não encontrada".into())
        })?,
        (None, Some(p)) => PathBuf::from(p),
        (None, None) => return Err("informe um caminho".into()),
    };

    let progress = Arc::new(Progress::new());
    {
        let mut slot = state.progress.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("já existe uma varredura em andamento".into());
        }
        *slot = Some(progress.clone());
    }

    let opts = ScanOptions {
        min_file_size: min_file_size.unwrap_or(1024 * 1024),
        ..ScanOptions::default()
    };
    let state = state.inner().clone();
    let done = Arc::new(AtomicBool::new(false));

    // Thread que publica o progresso.
    {
        let (app, progress, done) = (app.clone(), progress.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let _ = app.emit("scan-progress", progress.snapshot());
                std::thread::sleep(Duration::from_millis(200));
            }
        });
    }

    std::thread::Builder::new()
        .name("scan".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let result = diskscan::scan(&target, &opts, &progress);
            done.store(true, Ordering::Relaxed);
            let payload = match result {
                Err(e) => ScanFinished {
                    ok: false,
                    error: Some(format!("{}: {e}", target.display())),
                    stats: None,
                    rescanned_path: None,
                    cancelled: false,
                },
                Ok(sub) => {
                    let cancelled = progress.is_cancelled();
                    let mut guard = state.tree.lock().unwrap();
                    let mut rescanned = None;
                    match (node_id, guard.as_mut()) {
                        (Some(_), Some(tree)) => {
                            // O id pode ter mudado durante a varredura; procura de novo pelo caminho.
                            if let Some(id) = tree.find(&target) {
                                // Cancelada: um resultado parcial deixaria a pasta menor do que é.
                                if !cancelled {
                                    tree.replace_subtree(id, sub);
                                }
                                rescanned = Some(target.to_string_lossy().into_owned());
                            }
                        }
                        _ => *guard = Some(sub),
                    }
                    ScanFinished {
                        ok: true,
                        error: None,
                        stats: guard.as_ref().map(|t| t.stats()),
                        rescanned_path: rescanned,
                        cancelled,
                    }
                }
            };
            *state.progress.lock().unwrap() = None;
            let _ = app.emit("scan-finished", payload);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn cancel_scan(state: State<'_, Arc<AppState>>) {
    if let Some(p) = state.progress.lock().unwrap().as_ref() {
        p.cancel();
    }
}

// ---------------------------------------------------------------- consultas

#[tauri::command]
fn get_stats(state: State<'_, Arc<AppState>>) -> CmdResult<ScanStats> {
    with_tree(&state, |t| Ok(t.stats()))
}

#[tauri::command]
fn get_node(
    state: State<'_, Arc<AppState>>,
    id: Option<u32>,
    depth: u32,
    max_children: usize,
    min_fraction: Option<f64>,
) -> CmdResult<NodeView> {
    with_tree(&state, |t| {
        let id = id.unwrap_or(t.root_id());
        let base = t.view(id, 0, 0, 0).ok_or("item não encontrado")?.size;
        // Itens menores que essa fração do nó consultado viram "Rest".
        let min_size = (base as f64 * min_fraction.unwrap_or(0.0).clamp(0.0, 1.0)) as u64;
        t.view(id, depth.min(8), max_children, min_size)
            .ok_or_else(|| "item não encontrado".into())
    })
}

#[tauri::command]
fn get_ancestors(state: State<'_, Arc<AppState>>, id: u32) -> CmdResult<Vec<Crumb>> {
    with_tree(&state, |t| Ok(t.ancestors(id)))
}

#[tauri::command]
fn get_path(state: State<'_, Arc<AppState>>, id: u32) -> CmdResult<String> {
    with_tree(&state, |t| {
        t.path_of(id)
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| "item não encontrado".into())
    })
}

#[tauri::command]
fn get_top_files(state: State<'_, Arc<AppState>>, limit: usize) -> CmdResult<Vec<FileInfo>> {
    with_tree(&state, |t| Ok(t.top_files(limit).to_vec()))
}

#[tauri::command]
fn find_path(state: State<'_, Arc<AppState>>, path: String) -> CmdResult<Option<u32>> {
    with_tree(&state, |t| Ok(t.find(Path::new(&path))))
}

// ---------------------------------------------------------------- ações

#[tauri::command]
fn reveal_path(path: String, select: bool) -> CmdResult<()> {
    // Abrir um arquivo no Explorer o executaria; arquivos são sempre só selecionados.
    let select = select || !Path::new(&path).is_dir();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer");
        if select {
            cmd.raw_arg(format!("/select,\"{path}\""));
        } else {
            cmd.raw_arg(format!("\"{path}\""));
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        let p = Path::new(&path);
        let target = if select { p.parent().unwrap_or(p) } else { p };
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener)
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Subpastas do Windows cujo *conteúdo* pode ser apagado com segurança.
const WINDOWS_CLEANABLE: [&str; 2] = ["temp", r"softwaredistribution\download"];

/// Recusa apagar raízes de partição e a pasta do Windows (exceto o conteúdo
/// das pastas em [`WINDOWS_CLEANABLE`]).
fn check_deletable(path: &Path) -> CmdResult<()> {
    if path.parent().is_none() {
        return Err("não é permitido apagar a raiz de uma partição".into());
    }
    #[cfg(windows)]
    {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let lower = |p: &Path| p.to_string_lossy().to_lowercase();
        let (p, s) = (lower(path), lower(Path::new(&sys)));
        let cleanable = WINDOWS_CLEANABLE
            .iter()
            .any(|sub| p.starts_with(&format!("{s}\\{sub}\\")));
        if !cleanable && (p == s || p.starts_with(&format!("{s}\\"))) {
            return Err(
                "itens dentro da pasta do Windows não podem ser apagados por aqui \
                 (use a Limpeza de Disco / DISM)"
                    .into(),
            );
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteResult {
    freed: u64,
    stats: ScanStats,
}

/// Apaga um item da varredura atual, identificado pelo `id` do nó ou pelo
/// `path` (usado na lista de maiores arquivos, onde arquivos pequenos podem
/// não ter nó próprio).
#[tauri::command]
async fn delete_node(
    state: State<'_, Arc<AppState>>,
    history: State<'_, Arc<systools::history::History>>,
    id: Option<u32>,
    path: Option<String>,
    permanent: bool,
) -> CmdResult<DeleteResult> {
    let state = state.inner().clone();
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = with_tree(&state, |t| {
            let (id, path) = match (id, path) {
                (Some(id), _) => (Some(id), t.path_of(id).ok_or("item não encontrado")?),
                (None, Some(p)) => {
                    let p = PathBuf::from(p);
                    let id = t.find(&p);
                    if id.is_none() && t.top_file_size(&p).is_none() {
                        return Err("item não encontrado na varredura atual".into());
                    }
                    (id, p)
                }
                (None, None) => return Err("informe o item a apagar".into()),
            };
            if let Some(id) = id {
                if !matches!(t.kind(id), Some(NodeKind::Dir | NodeKind::File)) {
                    return Err("este item não pode ser apagado".into());
                }
                if id == t.root_id() {
                    return Err("não é possível apagar a pasta raiz da varredura".into());
                }
            }
            Ok(path)
        })?;
        check_deletable(&path)?;

        // O lock não é mantido durante a exclusão (pode demorar).
        let res = if permanent {
            if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            }
            .map_err(|e| e.to_string())
        } else {
            trash::delete(&path).map_err(|e| e.to_string())
        };

        with_tree(&state, |t| match res {
            Ok(()) => {
                let freed = t.remove_path(&path).unwrap_or(0);
                history.record(
                    "delete",
                    format!(
                        "{} {}",
                        if permanent { "Excluído permanentemente:" } else { "Enviado para a Lixeira:" },
                        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    ),
                    format!("{} · {}", path.display(), systools::history::fmt_bytes(freed)),
                    None,
                );
                Ok(DeleteResult {
                    freed,
                    stats: t.stats(),
                })
            }
            Err(e) => {
                // Exclusão parcial: re-varre o que sobrou para a árvore ficar correta.
                match t.find(&path) {
                    Some(id) if path.exists() && t.kind(id) == Some(NodeKind::Dir) => {
                        let opts = ScanOptions::default();
                        if let Ok(sub) = diskscan::scan(&path, &opts, &Progress::new()) {
                            t.replace_subtree(id, sub);
                        }
                    }
                    _ if !path.exists() => {
                        t.remove_path(&path);
                    }
                    _ => {}
                }
                Err(format!("falha ao apagar {}: {e}", path.display()))
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------- limpeza automática

#[tauri::command]
async fn cleanup_analyze() -> CmdResult<cleanup::CleanupReport> {
    tauri::async_runtime::spawn_blocking(cleanup::analyze)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupProgress {
    index: usize,
    total: usize,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupRunResult {
    results: Vec<cleanup::CategoryResult>,
    /// Totais da varredura atual depois de atualizada (None sem varredura).
    stats: Option<ScanStats>,
}

/// Limpa as categorias informadas e atualiza a árvore da varredura atual nas
/// pastas afetadas. Emite `cleanup-progress` antes de cada categoria.
#[tauri::command]
async fn cleanup_run(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    history: State<'_, Arc<systools::history::History>>,
    ids: Vec<String>,
) -> CmdResult<CleanupRunResult> {
    if state.progress.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("aguarde a varredura em andamento terminar".into());
    }
    let state = state.inner().clone();
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let total = ids.len();
        let (results, touched) = cleanup::run(&ids, |index, title| {
            let _ = app.emit("cleanup-progress", CleanupProgress { index, total, title: title.into() });
        });
        let freed: u64 = results.iter().map(|r| r.freed).sum();
        let names: Vec<&str> = results.iter().map(|r| r.title).collect();
        history.record(
            "cleanup",
            format!("Limpeza automática: {} liberados", systools::history::fmt_bytes(freed)),
            names.join(", "),
            None,
        );
        let mut guard = state.tree.lock().map_err(|e| e.to_string())?;
        if let Some(t) = guard.as_mut() {
            for path in touched {
                let Some(id) = t.find(&path) else { continue };
                if !path.exists() {
                    t.remove_path(&path);
                } else if t.kind(id) == Some(NodeKind::Dir) {
                    if let Ok(sub) = diskscan::scan(&path, &ScanOptions::default(), &Progress::new()) {
                        t.replace_subtree(id, sub);
                    }
                }
            }
        }
        Ok(CleanupRunResult {
            results,
            stats: guard.as_ref().map(|t| t.stats()),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Abre uma nova instância do app elevada (UAC) e fecha esta.
#[tauri::command]
fn relaunch_as_admin(app: AppHandle) -> CmdResult<()> {
    cleanup::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

/// Abre um link do projeto no navegador padrão. Só aceita páginas do GitHub,
/// para o frontend não conseguir abrir qualquer coisa.
#[tauri::command]
fn open_url(url: String) -> CmdResult<()> {
    if !url.starts_with("https://github.com/") || url.contains('"') || url.contains(' ') {
        return Err("link não permitido".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener).arg(&url).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------- app

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(AppState::default()))
        .manage(tools::RestoreSession::default())
        .setup(|app| {
            let path = app.path().app_local_data_dir().ok().map(|d| d.join("history.json"));
            app.manage(Arc::new(systools::history::History::load(path)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_drives,
            start_scan,
            cancel_scan,
            get_stats,
            get_node,
            get_ancestors,
            get_path,
            get_top_files,
            find_path,
            reveal_path,
            delete_node,
            tools::is_elevated,
            tools::system_report,
            tools::list_tweaks,
            tools::set_tweak,
            tools::list_startup,
            tools::set_startup,
            tools::ensure_restore_point,
            tools::list_history,
            tools::undo_history,
            tools::clear_history,
            cleanup_analyze,
            cleanup_run,
            relaunch_as_admin,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar a aplicação");
}
