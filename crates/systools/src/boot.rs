//! Tempo de inicialização e o que a atrasou, pelo log
//! Microsoft-Windows-Diagnostics-Performance/Operational (exige administrador).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Boot {
    pub time: String,
    /// Até a área de trabalho estar utilizável.
    pub boot_ms: u64,
    pub main_path_ms: u64,
    pub post_boot_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SlowItem {
    /// "app", "driver" ou "service".
    pub kind: String,
    pub name: String,
    pub file: String,
    pub degradation_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BootRaw {
    pub boots: Vec<Boot>,
    pub slow: Vec<SlowItem>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SlowSummary {
    pub kind: String,
    pub name: String,
    pub file: String,
    pub times: u32,
    pub avg_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootReport {
    pub boots: Vec<Boot>,
    pub avg_ms: Option<u64>,
    pub slow: Vec<SlowSummary>,
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
function Data($e) { $h = @{}; foreach ($d in ([xml]$e.ToXml()).Event.EventData.Data) { $h[$d.Name] = [string]$d.'#text' }; $h }
try {
    $log = 'Microsoft-Windows-Diagnostics-Performance/Operational'
    $boots = @(Get-WinEvent -LogName $log -FilterXPath '*[System[EventID=100]]' -MaxEvents 10 -ErrorAction SilentlyContinue | ForEach-Object {
        $d = Data $_
        [pscustomobject]@{ time = $_.TimeCreated.ToString('yyyy-MM-dd HH:mm'); bootMs = [uint64]$d.BootTime;
            mainPathMs = [uint64]$d.MainPathBootTime; postBootMs = [uint64]$d.BootPostBootTime }
    })
    $kinds = @{ 101 = 'app'; 102 = 'driver'; 103 = 'service'; 109 = 'driver' }
    $slow = @(Get-WinEvent -LogName $log -FilterXPath '*[System[EventID=101 or EventID=102 or EventID=103 or EventID=109]]' -MaxEvents 200 -ErrorAction SilentlyContinue | ForEach-Object {
        $d = Data $_
        $name = $d.FriendlyName; if (-not $name) { $name = $d.Name }; if (-not $name) { $name = $d.FileName }
        [pscustomobject]@{ kind = $kinds[$_.Id]; name = [string]$name; file = [string]$d.FileName; degradationMs = [uint64]$d.DegradationTime }
    })
    [pscustomobject]@{ boots = $boots; slow = $slow; error = $null } | ConvertTo-Json -Depth 4 -Compress
} catch {
    [pscustomobject]@{ boots = @(); slow = @(); error = $_.Exception.Message } | ConvertTo-Json -Compress
}
"#;

pub fn run() -> Result<BootReport, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let out = crate::shell::powershell(SCRIPT)?;
    let raw: BootRaw = serde_json::from_str(out.trim_start_matches('\u{feff}').trim())
        .map_err(|e| format!("resposta inesperada do PowerShell: {e}"))?;
    if let Some(e) = &raw.error {
        if raw.boots.is_empty() {
            return Err(format!("não foi possível ler o log de inicialização (precisa de administrador): {e}"));
        }
    }
    Ok(summarize(raw))
}

pub fn summarize(raw: BootRaw) -> BootReport {
    let valid: Vec<u64> = raw.boots.iter().map(|b| b.boot_ms).filter(|ms| *ms > 0).collect();
    let avg_ms = (!valid.is_empty()).then(|| valid.iter().sum::<u64>() / valid.len() as u64);
    let mut slow: Vec<SlowSummary> = Vec::new();
    for s in raw.slow.iter().filter(|s| !s.name.trim().is_empty()) {
        match slow.iter_mut().find(|x| x.kind == s.kind && x.name.eq_ignore_ascii_case(&s.name)) {
            Some(x) => {
                x.avg_ms = (x.avg_ms * u64::from(x.times) + s.degradation_ms) / u64::from(x.times + 1);
                x.times += 1;
            }
            None => slow.push(SlowSummary {
                kind: s.kind.clone(),
                name: s.name.trim().to_string(),
                file: s.file.clone(),
                times: 1,
                avg_ms: s.degradation_ms,
            }),
        }
    }
    slow.sort_by(|a, b| (b.avg_ms * u64::from(b.times)).cmp(&(a.avg_ms * u64::from(a.times))));
    slow.truncate(20);
    BootReport {
        boots: raw.boots,
        avg_ms,
        slow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_boots_and_groups_slow_items() {
        let raw: BootRaw = serde_json::from_str(
            r#"{"boots":[{"time":"2026-11-26 08:00","bootMs":42000,"mainPathMs":20000,"postBootMs":22000},
                         {"time":"2026-11-25 08:00","bootMs":38000,"mainPathMs":18000,"postBootMs":20000}],
                "slow":[{"kind":"app","name":"Discord","file":"Update.exe","degradationMs":3000},
                        {"kind":"app","name":"discord","file":"Update.exe","degradationMs":5000},
                        {"kind":"service","name":"Serviço X","file":"x.exe","degradationMs":6000},
                        {"kind":"driver","name":" ","file":"","degradationMs":1}],
                "error":null}"#,
        )
        .unwrap();
        let r = summarize(raw);
        assert_eq!(r.avg_ms, Some(40_000));
        assert_eq!(r.slow.len(), 2);
        assert_eq!(r.slow[0].name, "Discord");
        assert_eq!((r.slow[0].times, r.slow[0].avg_ms), (2, 4000));
        assert_eq!(r.slow[1].kind, "service");
    }

    #[test]
    fn single_boot_from_powershell_still_parses() {
        let raw: BootRaw = serde_json::from_str(r#"{"boots":[],"slow":[],"error":"Acesso negado"}"#).unwrap();
        let r = summarize(raw);
        assert_eq!(r.avg_ms, None);
    }
}
