//! Comandos dos módulos Limpeza, Diagnóstico e Otimizações.
//!
//! Tudo que pode demorar (PowerShell, varrer caches) roda fora da thread
//! principal para a janela não congelar.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use systools::{cleanup, diagnostics, elevation, restore, startup, tweaks};
use tauri::State;

use systools::history::{History, Undo};

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn is_elevated() -> bool {
    elevation::is_elevated()
}

#[tauri::command]
pub async fn system_report() -> CmdResult<diagnostics::Report> {
    blocking(diagnostics::run).await
}

#[tauri::command]
pub async fn list_tweaks() -> CmdResult<Vec<tweaks::TweakInfo>> {
    blocking(|| Ok(tweaks::list())).await
}

#[tauri::command]
pub async fn set_tweak(history: State<'_, Arc<History>>, id: String, apply: bool) -> CmdResult<tweaks::TweakInfo> {
    let history = history.inner().clone();
    blocking(move || {
        let info = tweaks::set(&id, apply)?;
        record_tweak(&history, &info, apply);
        Ok(info)
    })
    .await
}

fn record_tweak(history: &History, info: &tweaks::TweakInfo, apply: bool) {
    history.record(
        "tweak",
        format!("{}: {}", if apply { "Aplicado" } else { "Desfeito" }, info.title),
        if info.needs_restart { "Vale depois de reiniciar." } else { "" },
        Some(Undo::Tweak { id: info.id.into(), apply: !apply }),
    );
}

#[tauri::command]
pub async fn list_startup() -> CmdResult<Vec<startup::StartupEntry>> {
    blocking(startup::list).await
}

#[tauri::command]
pub async fn set_startup(history: State<'_, Arc<History>>, id: String, enabled: bool) -> CmdResult<()> {
    let history = history.inner().clone();
    blocking(move || {
        startup::set_enabled(&id, enabled)?;
        record_startup(&history, &id, enabled);
        Ok(())
    })
    .await
}

fn record_startup(history: &History, id: &str, enabled: bool) {
    let name = id.split_once(':').map(|(_, n)| n).unwrap_or(id);
    history.record(
        "startup",
        format!("{} na inicialização: {name}", if enabled { "Ativado" } else { "Desativado" }),
        "",
        Some(Undo::Startup { id: id.into(), enabled: !enabled }),
    );
}

// ---------------------------------------------------------------- ponto de restauração

/// Se já foi criado (ou tentado) um ponto de restauração nesta sessão.
#[derive(Default)]
pub struct RestoreSession(AtomicBool);

/// Cria um ponto de restauração na primeira alteração da sessão. Devolve
/// `None` se já foi feito nesta sessão.
#[tauri::command]
pub async fn ensure_restore_point(
    session: State<'_, RestoreSession>,
    history: State<'_, Arc<History>>,
    reason: String,
) -> CmdResult<Option<restore::Outcome>> {
    let elevated = elevation::is_elevated();
    // Sem administrador não dá para criar; tenta de novo se o app for elevado.
    if !elevated {
        return Ok(Some(restore::Outcome::NotAdmin));
    }
    if session.0.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    let history = history.inner().clone();
    blocking(move || {
        let outcome = restore::create(&format!("WindowsDoctor: {reason}"), elevated);
        if outcome == restore::Outcome::Created {
            history.record("restore", "Ponto de restauração criado", reason, None);
        }
        Ok(Some(outcome))
    })
    .await
}

// ---------------------------------------------------------------- histórico

#[tauri::command]
pub fn list_history(history: State<'_, Arc<History>>) -> Vec<systools::history::Entry> {
    history.list()
}

#[tauri::command]
pub fn clear_history(history: State<'_, Arc<History>>) {
    history.clear();
}

/// Desfaz uma entrada do histórico (ajuste ou programa de inicialização).
#[tauri::command]
pub async fn undo_history(history: State<'_, Arc<History>>, id: u64) -> CmdResult<()> {
    let history = history.inner().clone();
    blocking(move || {
        let entry = history.get(id).ok_or("entrada não encontrada")?;
        if entry.undone {
            return Err("esta alteração já foi desfeita".into());
        }
        match entry.undo.ok_or("esta ação não pode ser desfeita pelo app")? {
            Undo::Tweak { id: tweak, apply } => {
                let info = tweaks::set(&tweak, apply)?;
                record_tweak(&history, &info, apply);
            }
            Undo::Startup { id: item, enabled } => {
                startup::set_enabled(&item, enabled)?;
                record_startup(&history, &item, enabled);
            }
        }
        history.mark_undone(id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn list_cleanup() -> CmdResult<Vec<cleanup::TargetInfo>> {
    blocking(|| Ok(cleanup::targets().iter().map(cleanup::measure).collect())).await
}

#[tauri::command]
pub async fn run_cleanup(ids: Vec<String>) -> CmdResult<Vec<cleanup::CleanResult>> {
    blocking(move || {
        Ok(ids
            .iter()
            .map(|id| match cleanup::find(id) {
                Some(t) => cleanup::clean(&t),
                None => cleanup::CleanResult {
                    id: id.clone(),
                    error: Some("item de limpeza desconhecido".into()),
                    ..Default::default()
                },
            })
            .collect())
    })
    .await
}
