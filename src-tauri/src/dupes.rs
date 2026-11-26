//! Comandos da busca de arquivos duplicados.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use systools::duplicates::{self, Group, Progress};
use systools::history::{fmt_bytes, History};
use tauri::{AppHandle, Emitter, State};

type CmdResult<T> = Result<T, String>;

#[derive(Default)]
pub struct DupState {
    progress: Mutex<Option<Arc<Progress>>>,
    /// Grupos da última busca: só arquivos deles podem ser apagados.
    last: Mutex<Vec<Group>>,
}

/// Procura duplicados. Sem pastas informadas, usa a pasta do usuário.
/// Emite `duplicates-progress` enquanto roda.
#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    state: State<'_, Arc<DupState>>,
    roots: Option<Vec<String>>,
    min_size: u64,
) -> CmdResult<Vec<Group>> {
    let roots: Vec<PathBuf> = match roots {
        Some(r) if !r.is_empty() => r.into_iter().map(PathBuf::from).collect(),
        _ => vec![std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or("pasta do usuário não encontrada")?],
    };
    let progress = Arc::new(Progress::default());
    {
        let mut slot = state.progress.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("já há uma busca em andamento".into());
        }
        *slot = Some(progress.clone());
    }
    let state = state.inner().clone();
    let groups = tauri::async_runtime::spawn_blocking(move || {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let (app, progress, done) = (app.clone(), progress.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    let _ = app.emit("duplicates-progress", progress.snapshot());
                    std::thread::sleep(Duration::from_millis(300));
                }
            });
        }
        let groups = duplicates::find(&roots, min_size, &progress);
        done.store(true, Ordering::Relaxed);
        groups
    })
    .await
    .map_err(|e| e.to_string());
    *state.progress.lock().unwrap() = None;
    let groups = groups?;
    *state.last.lock().unwrap() = groups.clone();
    Ok(groups)
}

#[tauri::command]
pub fn cancel_duplicates(state: State<'_, Arc<DupState>>) {
    if let Some(p) = state.progress.lock().unwrap().as_ref() {
        p.cancel.store(true, Ordering::Relaxed);
    }
}

/// Envia para a Lixeira cópias escolhidas. Recusa caminhos que não vieram da
/// última busca e nunca apaga todas as cópias de um grupo.
#[tauri::command]
pub async fn delete_duplicates(
    state: State<'_, Arc<DupState>>,
    history: State<'_, Arc<History>>,
    paths: Vec<String>,
) -> CmdResult<u64> {
    let chosen: HashSet<String> = paths.into_iter().collect();
    let last = state.last.lock().map_err(|e| e.to_string())?.clone();
    let mut to_delete: Vec<(String, u64)> = Vec::new();
    for g in &last {
        let picked: Vec<&String> = g.paths.iter().filter(|p| chosen.contains(*p)).collect();
        if picked.len() >= g.paths.len() {
            return Err("mantenha pelo menos uma cópia de cada arquivo".into());
        }
        to_delete.extend(picked.into_iter().map(|p| (p.clone(), g.size)));
    }
    if to_delete.len() != chosen.len() {
        return Err("alguns arquivos não fazem parte da última busca; busque de novo".into());
    }
    let history = history.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut freed = 0;
        let mut errors = Vec::new();
        let mut deleted = HashSet::new();
        for (path, size) in &to_delete {
            match trash::delete(path) {
                Ok(()) => {
                    freed += size;
                    deleted.insert(path.clone());
                }
                Err(e) => errors.push(format!("{path}: {e}")),
            }
        }
        // Tira os apagados da lista guardada.
        let mut last = state.last.lock().unwrap();
        for g in last.iter_mut() {
            g.paths.retain(|p| !deleted.contains(p));
            g.wasted = g.size * (g.paths.len().max(1) as u64 - 1);
        }
        last.retain(|g| g.paths.len() > 1);
        history.record(
            "cleanup",
            format!("Duplicados enviados para a Lixeira: {}", fmt_bytes(freed)),
            format!("{} arquivo(s)", deleted.len()),
            None,
        );
        if errors.is_empty() {
            Ok(freed)
        } else {
            Err(format!("{} liberados, mas houve falhas: {}", fmt_bytes(freed), errors.join("; ")))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
