//! Comandos do desinstalador.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use systools::history::{fmt_bytes, History};
use systools::programs::{self, Program};
use tauri::State;

type CmdResult<T> = Result<T, String>;

#[derive(Default)]
pub struct ProgramsState {
    /// Programas cujo desinstalador foi aberto (a entrada some do Registro).
    uninstalled: Mutex<HashMap<String, Program>>,
    /// Pastas de restos encontradas: só elas podem ser apagadas.
    leftovers: Mutex<HashSet<String>>,
}

#[tauri::command]
pub async fn list_programs() -> CmdResult<Vec<Program>> {
    tauri::async_runtime::spawn_blocking(programs::list)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn uninstall_program(
    state: State<'_, Arc<ProgramsState>>,
    history: State<'_, Arc<History>>,
    id: String,
) -> CmdResult<()> {
    let state = state.inner().clone();
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let p = programs::uninstall(&id)?;
        history.record("uninstall", format!("Desinstalador aberto: {}", p.name), p.publisher.clone(), None);
        state.uninstalled.lock().unwrap().insert(id, p);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pastas que sobraram de um programa desinstalado. Se ele ainda estiver
/// instalado, avisa em vez de procurar.
#[tauri::command]
pub async fn find_leftovers(state: State<'_, Arc<ProgramsState>>, id: String) -> CmdResult<Vec<(String, u64)>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if programs::list()?.iter().any(|p| p.id == id) {
            return Err("o programa ainda está instalado; termine a desinstalação primeiro".to_string());
        }
        let p = state
            .uninstalled
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or("abra o desinstalador por aqui antes de procurar restos")?;
        let found: Vec<(String, u64)> = programs::leftovers(&p)
            .into_iter()
            .map(|d| {
                let size = dir_size(&d);
                (d.display().to_string(), size)
            })
            .collect();
        state.leftovers.lock().unwrap().extend(found.iter().map(|(d, _)| d.clone()));
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn dir_size(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
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
        })
        .unwrap_or(0)
}

/// Envia para a Lixeira pastas devolvidas por `find_leftovers`.
#[tauri::command]
pub async fn delete_leftovers(
    state: State<'_, Arc<ProgramsState>>,
    history: State<'_, Arc<History>>,
    paths: Vec<String>,
) -> CmdResult<u64> {
    let allowed = state.leftovers.lock().map_err(|e| e.to_string())?.clone();
    if let Some(bad) = paths.iter().find(|p| !allowed.contains(*p)) {
        return Err(format!("pasta não permitida: {bad}"));
    }
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut freed = 0;
        for p in &paths {
            let size = dir_size(std::path::Path::new(p));
            trash::delete(p).map_err(|e| format!("{p}: {e}"))?;
            freed += size;
        }
        history.record(
            "uninstall",
            format!("Restos de programa enviados para a Lixeira: {}", fmt_bytes(freed)),
            paths.join(", "),
            None,
        );
        Ok(freed)
    })
    .await
    .map_err(|e| e.to_string())?
}
