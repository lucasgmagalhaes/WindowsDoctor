//! Espaço usado pelos pontos de restauração (cópias de sombra do VSS).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RestorePoint {
    pub sequence: u32,
    pub description: String,
    pub time: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShadowInfo {
    pub drive: String,
    pub used: u64,
    pub max: u64,
    /// Capacidade da unidade, para mostrar o limite em %.
    pub drive_size: u64,
    pub copies: u32,
    pub restore_points: Vec<RestorePoint>,
}

const LIST: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$drive = $env:SystemDrive
$vol = Get-CimInstance Win32_Volume -Filter "DriveLetter='$drive'"
$storage = Get-CimInstance Win32_ShadowStorage | Where-Object { $_.Volume.DeviceID -eq $vol.DeviceID } | Select-Object -First 1
if (-not $storage) { $storage = Get-CimInstance Win32_ShadowStorage | Select-Object -First 1 }
$copies = @(Get-CimInstance Win32_ShadowCopy | Where-Object { $_.VolumeName -eq $vol.DeviceID })
$points = @(Get-ComputerRestorePoint | Sort-Object SequenceNumber -Descending | ForEach-Object {
    [pscustomobject]@{
        sequence = [int]$_.SequenceNumber
        description = [string]$_.Description
        time = ([Management.ManagementDateTimeConverter]::ToDateTime($_.CreationTime)).ToString('yyyy-MM-dd HH:mm')
    }
})
[pscustomobject]@{
    drive = $drive
    used = [uint64]$storage.UsedSpace
    max = [uint64]$storage.MaxSpace
    driveSize = [uint64]$vol.Capacity
    copies = $copies.Count
    restorePoints = $points
} | ConvertTo-Json -Depth 4 -Compress
"#;

pub fn info() -> Result<ShadowInfo, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let out = crate::shell::powershell(LIST)?;
    serde_json::from_str(out.trim_start_matches('\u{feff}').trim())
        .map_err(|e| format!("resposta inesperada do PowerShell: {e}"))
}

/// Apaga todas as cópias de sombra da unidade do sistema menos a mais nova.
pub fn delete_all_but_newest() -> Result<u32, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let out = crate::shell::powershell(
        r#"
$ErrorActionPreference = 'Stop'
$vol = Get-CimInstance Win32_Volume -Filter "DriveLetter='$env:SystemDrive'"
$old = @(Get-CimInstance Win32_ShadowCopy | Where-Object { $_.VolumeName -eq $vol.DeviceID } |
    Sort-Object InstallDate -Descending | Select-Object -Skip 1)
$old | Remove-CimInstance
$old.Count
"#,
    )?;
    out.trim().parse().map_err(|_| format!("resposta inesperada: {out}"))
}

/// Limita o espaço dos pontos de restauração a `percent`% da unidade.
pub fn set_max_percent(percent: u8) -> Result<(), String> {
    if !(1..=50).contains(&percent) {
        return Err("limite inválido".into());
    }
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let for_ = format!("/for={drive}");
    let on = format!("/on={drive}");
    let max = format!("/maxsize={percent}%");
    crate::shell::run("vssadmin.exe", &["resize", "shadowstorage", &for_, &on, &max]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info() {
        let json = r#"{"drive":"C:","used":5368709120,"max":10737418240,"driveSize":512110190592,"copies":3,
            "restorePoints":[{"sequence":42,"description":"WindowsDoctor: Desativar MPO","time":"2026-11-26 10:00"}]}"#;
        let i: ShadowInfo = serde_json::from_str(json).unwrap();
        assert_eq!(i.copies, 3);
        assert_eq!(i.restore_points[0].sequence, 42);
        // Sem pontos o PowerShell manda [] e sem armazenamento, zeros.
        let i: ShadowInfo = serde_json::from_str(r#"{"drive":"C:","used":0,"max":0,"copies":0,"restorePoints":[]}"#).unwrap();
        assert_eq!(i.drive_size, 0);
    }

    #[test]
    fn rejects_bad_limits() {
        assert!(set_max_percent(0).is_err());
        assert!(set_max_percent(90).is_err());
    }
}
