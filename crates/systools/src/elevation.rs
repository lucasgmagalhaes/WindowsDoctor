//! Execução como administrador.

/// `true` se o app está rodando como administrador.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        crate::reg::can_write_machine()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Abre uma nova instância do app pedindo elevação (aviso do UAC).
/// Devolve erro se o usuário recusar; nesse caso a instância atual continua.
pub fn relaunch_as_admin() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().replace('\'', "''");
        crate::shell::powershell(&format!(
            "$ErrorActionPreference = 'Stop'; Start-Process -FilePath '{exe}' -Verb RunAs"
        ))
        .map(|_| ())
        .map_err(|_| "a elevação foi cancelada".into())
    }
    #[cfg(not(windows))]
    {
        Err(crate::WINDOWS_ONLY.into())
    }
}
