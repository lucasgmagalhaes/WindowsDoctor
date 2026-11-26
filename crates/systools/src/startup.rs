//! Programas que iniciam com o Windows (chaves Run e pastas Inicializar).
//!
//! Ativar/desativar usa o mesmo mecanismo do Gerenciador de Tarefas
//! (`Explorer\StartupApproved`), então a entrada original não é apagada e a
//! mudança aparece também lá.

use serde::Serialize;

use crate::reg::{self, Hive};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupEntry {
    /// `<origem>:<nome>`, usado para ativar/desativar.
    pub id: String,
    pub name: String,
    pub command: String,
    pub location: &'static str,
    pub enabled: bool,
    pub needs_admin: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Registry(&'static str),
    /// Pasta Inicializar; o caminho vem de uma variável de ambiente.
    Folder { env: &'static str, sub: &'static str },
}

struct Origin {
    hive: Hive,
    kind: Kind,
    /// Nome da subchave em `StartupApproved`.
    approved: &'static str,
    location: &'static str,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

const ORIGINS: [Origin; 5] = [
    Origin {
        hive: Hive::CurrentUser,
        kind: Kind::Registry(RUN),
        approved: "Run",
        location: "Registro (este usuário)",
    },
    Origin {
        hive: Hive::LocalMachine,
        kind: Kind::Registry(RUN),
        approved: "Run",
        location: "Registro (todos os usuários)",
    },
    Origin {
        hive: Hive::LocalMachine,
        kind: Kind::Registry(RUN32),
        approved: "Run32",
        location: "Registro 32 bits (todos os usuários)",
    },
    Origin {
        hive: Hive::CurrentUser,
        kind: Kind::Folder {
            env: "APPDATA",
            sub: r"Microsoft\Windows\Start Menu\Programs\Startup",
        },
        approved: "StartupFolder",
        location: "Pasta Inicializar (este usuário)",
    },
    Origin {
        hive: Hive::LocalMachine,
        kind: Kind::Folder {
            env: "ProgramData",
            sub: r"Microsoft\Windows\Start Menu\Programs\StartUp",
        },
        approved: "StartupFolder",
        location: "Pasta Inicializar (todos os usuários)",
    },
];

fn approved_key(o: &Origin) -> String {
    format!(r"{APPROVED}\{}", o.approved)
}

fn entries_of(o: &Origin) -> Result<Vec<(String, String)>, String> {
    match o.kind {
        Kind::Registry(path) => reg::string_values(o.hive, path),
        Kind::Folder { env, sub } => {
            let Some(base) = std::env::var_os(env) else {
                return Ok(Vec::new());
            };
            let dir = std::path::Path::new(&base).join(sub);
            let Ok(read) = std::fs::read_dir(&dir) else {
                return Ok(Vec::new());
            };
            Ok(read
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().display().to_string()))
                .filter(|(name, _)| !name.eq_ignore_ascii_case("desktop.ini"))
                .collect())
        }
    }
}

// ---------------------------------------------------------------- tarefas e serviços

const EXTRA_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$tasks = @(Get-ScheduledTask | Where-Object {
    $_.TaskPath -notlike '\Microsoft\*' -and
    (@($_.Triggers) | Where-Object { $_.CimClass.CimClassName -in 'MSFT_TaskLogonTrigger', 'MSFT_TaskBootTrigger' })
} | ForEach-Object {
    [pscustomobject]@{
        path = [string]$_.TaskPath
        name = [string]$_.TaskName
        enabled = ($_.State -ne 'Disabled')
        command = ((@($_.Actions) | ForEach-Object { ("$($_.Execute) $($_.Arguments)").Trim() }) -join '; ')
    }
})
$services = @(Get-CimInstance Win32_Service | Where-Object {
    $_.StartMode -eq 'Auto' -and $_.PathName -and $_.PathName -notmatch '\\Windows\\'
} | ForEach-Object {
    [pscustomobject]@{ name = [string]$_.Name; display = [string]$_.DisplayName; command = [string]$_.PathName }
})
[pscustomobject]@{ tasks = $tasks; services = $services } | ConvertTo-Json -Depth 4 -Compress
"#;

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Extra {
    tasks: Vec<ExtraTask>,
    services: Vec<ExtraService>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ExtraTask {
    path: String,
    name: String,
    enabled: bool,
    command: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ExtraService {
    name: String,
    display: String,
    command: String,
}

fn extra_entries(json: &str) -> Vec<StartupEntry> {
    let Ok(extra) = serde_json::from_str::<Extra>(json.trim_start_matches('\u{feff}').trim()) else {
        return Vec::new();
    };
    let mut out: Vec<StartupEntry> = extra
        .tasks
        .into_iter()
        .map(|t| StartupEntry {
            id: format!("task:{}{}", t.path, t.name),
            name: t.name,
            command: t.command,
            location: "Tarefa agendada (ao entrar ou ligar)",
            enabled: t.enabled,
            needs_admin: true,
        })
        .collect();
    out.extend(extra.services.into_iter().map(|s| StartupEntry {
        id: format!("svc:{}", s.name),
        name: if s.display.is_empty() { s.name } else { s.display },
        command: s.command,
        location: "Serviço de outro fabricante (início automático)",
        enabled: true,
        needs_admin: true,
    }));
    out
}

/// "\Pasta\Nome" → ("\Pasta\", "Nome").
fn split_task(full: &str) -> (&str, &str) {
    match full.rfind('\\') {
        Some(i) => (&full[..=i], &full[i + 1..]),
        None => ("\\", full),
    }
}

fn set_extra(id: &str, enabled: bool) -> Option<Result<(), String>> {
    let q = |s: &str| s.replace('\'', "''");
    let script = if let Some(full) = id.strip_prefix("task:") {
        let (path, name) = split_task(full);
        format!(
            "$ErrorActionPreference = 'Stop'; {} -TaskPath '{}' -TaskName '{}' | Out-Null",
            if enabled { "Enable-ScheduledTask" } else { "Disable-ScheduledTask" },
            q(path),
            q(name)
        )
    } else {
        let name = id.strip_prefix("svc:")?;
        // "Manual": o serviço ainda pode ser iniciado pelo programa quando precisar.
        format!(
            "$ErrorActionPreference = 'Stop'; Set-Service -Name '{}' -StartupType {}",
            q(name),
            if enabled { "Automatic" } else { "Manual" }
        )
    };
    Some(
        crate::shell::powershell(&script)
            .map(|_| ())
            .map_err(|e| if e.contains("ccess") || e.contains("negado") {
                "acesso negado: execute o app como administrador".to_string()
            } else {
                e
            }),
    )
}

pub fn list() -> Result<Vec<StartupEntry>, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let mut out = Vec::new();
    for (i, o) in ORIGINS.iter().enumerate() {
        let Ok(entries) = entries_of(o) else { continue };
        for (name, command) in entries {
            let state = reg::get_binary(o.hive, &approved_key(o), &name).unwrap_or(None);
            out.push(StartupEntry {
                id: format!("{i}:{name}"),
                name: display_name(&name),
                command,
                location: o.location,
                enabled: is_enabled(state.as_deref()),
                needs_admin: o.hive == Hive::LocalMachine,
            });
        }
    }
    if let Ok(json) = crate::shell::powershell(EXTRA_SCRIPT) {
        out.extend(extra_entries(&json));
    }
    out.sort_by_key(|e| e.name.to_lowercase());
    Ok(out)
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    if let Some(r) = set_extra(id, enabled) {
        return r;
    }
    let (index, name) = id.split_once(':').ok_or("id inválido")?;
    let o = index
        .parse::<usize>()
        .ok()
        .and_then(|i| ORIGINS.get(i))
        .ok_or("id inválido")?;
    if !entries_of(o)?.iter().any(|(n, _)| n == name) {
        return Err("o programa não está mais na inicialização".into());
    }
    reg::set_binary(o.hive, &approved_key(o), name, &approved_bytes(enabled, now_filetime()))
}

/// Atalhos da pasta Inicializar aparecem sem a extensão.
fn display_name(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in [".lnk", ".url"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

/// Sem valor em `StartupApproved` o item está ativado; com valor, o primeiro
/// byte par (02, 06) significa ativado e ímpar (03, 07), desativado.
fn is_enabled(state: Option<&[u8]>) -> bool {
    state.and_then(|b| b.first()).is_none_or(|b| b & 1 == 0)
}

/// 12 bytes: estado (4) + momento em que foi desativado (FILETIME, 8).
fn approved_bytes(enabled: bool, filetime: u64) -> [u8; 12] {
    let mut b = [0u8; 12];
    if enabled {
        b[0] = 2;
    } else {
        b[0] = 3;
        b[4..].copy_from_slice(&filetime.to_le_bytes());
    }
    b
}

/// Agora em intervalos de 100 ns desde 1601-01-01.
fn now_filetime() -> u64 {
    const UNIX_TO_1601_SECS: u64 = 11_644_473_600;
    let since_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (since_unix.as_secs() + UNIX_TO_1601_SECS) * 10_000_000 + u64::from(since_unix.subsec_nanos() / 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_state_round_trips() {
        assert!(is_enabled(None));
        assert!(is_enabled(Some(&[])));
        assert!(is_enabled(Some(&approved_bytes(true, 5))));
        assert!(!is_enabled(Some(&approved_bytes(false, 5))));
        assert!(is_enabled(Some(&[6, 0, 0, 0])));
        assert!(!is_enabled(Some(&[7, 0, 0, 0])));
        let b = approved_bytes(false, 0x0102_0304_0506_0708);
        assert_eq!(b, [3, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn parses_tasks_and_services() {
        let json = r#"{"tasks":[{"path":"\\","name":"OneDrive Updater","enabled":true,"command":"C:\\x.exe /update"},
                                {"path":"\\ASUS\\","name":"ArmouryCrate","enabled":false,"command":"ac.exe"}],
                       "services":[{"name":"Steam Client Service","display":"","command":"\"C:\\Steam\\steamservice.exe\" /RunAsService"}]}"#;
        let e = extra_entries(json);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].id, "task:\\OneDrive Updater");
        assert_eq!(e[1].id, "task:\\ASUS\\ArmouryCrate");
        assert!(!e[1].enabled);
        assert_eq!(e[2].id, "svc:Steam Client Service");
        assert_eq!(e[2].name, "Steam Client Service");
        assert_eq!(split_task("\\ASUS\\ArmouryCrate"), ("\\ASUS\\", "ArmouryCrate"));
        assert_eq!(split_task("\\Raiz"), ("\\", "Raiz"));
        assert!(extra_entries("lixo").is_empty());
    }

    #[test]
    fn filetime_is_after_2020() {
        // 2020-01-01 em FILETIME.
        assert!(now_filetime() > 132_223_104_000_000_000);
    }

    #[test]
    fn shortcut_names_hide_extension() {
        assert_eq!(display_name("Spotify.lnk"), "Spotify");
        assert_eq!(display_name("Site.URL"), "Site");
        assert_eq!(display_name("OneDrive"), "OneDrive");
    }
}
