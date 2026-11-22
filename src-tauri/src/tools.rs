//! Comandos dos módulos Limpeza, Diagnóstico e Otimizações.
//!
//! Tudo que pode demorar (PowerShell, varrer caches) roda fora da thread
//! principal para a janela não congelar.

use systools::{cleanup, diagnostics, elevation, startup, tweaks};
use tauri::AppHandle;

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

/// Abre o app como administrador e fecha esta instância.
#[tauri::command]
pub async fn relaunch_as_admin(app: AppHandle) -> CmdResult<()> {
    blocking(elevation::relaunch_as_admin).await?;
    app.exit(0);
    Ok(())
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
pub async fn set_tweak(id: String, apply: bool) -> CmdResult<tweaks::TweakInfo> {
    blocking(move || tweaks::set(&id, apply)).await
}

#[tauri::command]
pub async fn list_startup() -> CmdResult<Vec<startup::StartupEntry>> {
    blocking(startup::list).await
}

#[tauri::command]
pub async fn set_startup(id: String, enabled: bool) -> CmdResult<()> {
    blocking(move || startup::set_enabled(&id, enabled)).await
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
