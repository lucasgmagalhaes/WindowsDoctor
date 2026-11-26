//! Limpeza automática agendada no Agendador de Tarefas do Windows.

use serde::{Deserialize, Serialize};

pub const TASK_NAME: &str = r"WindowsDoctor\Limpeza automática";
/// Argumento que faz o app limpar sem abrir janela.
pub const CLI_FLAG: &str = "--auto-cleanup";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInfo {
    pub enabled: bool,
    /// Próxima execução como o Windows informa (formato local).
    pub next_run: Option<String>,
}

/// Argumentos do `schtasks /Create`. Todo domingo/dia/dia 1 às 12:00; com
/// `elevated` a tarefa roda com privilégios altos e limpa também as pastas
/// do sistema.
pub fn create_args(freq: Frequency, exe: &str, elevated: bool) -> Vec<String> {
    let mut a: Vec<String> = ["/Create", "/F", "/TN", TASK_NAME].map(String::from).to_vec();
    match freq {
        Frequency::Daily => a.extend(["/SC", "DAILY"].map(String::from)),
        Frequency::Weekly => a.extend(["/SC", "WEEKLY", "/D", "SUN"].map(String::from)),
        Frequency::Monthly => a.extend(["/SC", "MONTHLY", "/D", "1"].map(String::from)),
    }
    a.extend(["/ST", "12:00", "/TR"].map(String::from));
    a.push(format!("\"{exe}\" {CLI_FLAG}"));
    if elevated {
        a.extend(["/RL", "HIGHEST"].map(String::from));
    }
    a
}

/// `schtasks /Query /FO CSV /NH` → próxima execução (segunda coluna).
pub fn parse_query(csv: &str) -> Option<String> {
    let line = csv.lines().find(|l| !l.trim().is_empty())?;
    let cols: Vec<&str> = line.split("\",\"").map(|c| c.trim_matches('"')).collect();
    cols.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn schtasks(args: &[&str]) -> Result<String, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    crate::shell::run("schtasks.exe", args)
}

pub fn status() -> Result<ScheduleInfo, String> {
    match schtasks(&["/Query", "/TN", TASK_NAME, "/FO", "CSV", "/NH"]) {
        Ok(out) => Ok(ScheduleInfo {
            enabled: true,
            next_run: parse_query(&out),
        }),
        Err(e) if e == crate::WINDOWS_ONLY => Err(e),
        // Tarefa inexistente: schtasks sai com erro.
        Err(_) => Ok(ScheduleInfo {
            enabled: false,
            next_run: None,
        }),
    }
}

pub fn enable(freq: Frequency, elevated: bool) -> Result<ScheduleInfo, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = create_args(freq, &exe.display().to_string(), elevated);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    schtasks(&refs)?;
    status()
}

pub fn disable() -> Result<(), String> {
    schtasks(&["/Delete", "/F", "/TN", TASK_NAME]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_schtasks_arguments() {
        let a = create_args(Frequency::Weekly, r"C:\Program Files\WindowsDoctor\app.exe", true);
        assert_eq!(
            a,
            [
                "/Create", "/F", "/TN", TASK_NAME, "/SC", "WEEKLY", "/D", "SUN", "/ST", "12:00", "/TR",
                "\"C:\\Program Files\\WindowsDoctor\\app.exe\" --auto-cleanup", "/RL", "HIGHEST",
            ]
        );
        let d = create_args(Frequency::Daily, "x.exe", false);
        assert!(d.contains(&"DAILY".to_string()));
        assert!(!d.contains(&"HIGHEST".to_string()));
    }

    #[test]
    fn reads_next_run_from_csv() {
        let csv = "\"\\WindowsDoctor\\Limpeza automática\",\"30/11/2026 12:00:00\",\"Pronto\"\r\n";
        assert_eq!(parse_query(csv).as_deref(), Some("30/11/2026 12:00:00"));
        assert_eq!(parse_query(""), None);
    }
}
