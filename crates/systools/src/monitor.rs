//! Vigia o log do sistema em busca de travamentos do driver de vídeo (a tela
//! preta de ~2 segundos) enquanto o app está aberto.
//!
//! Usa o `wevtutil`, que já vem no Windows e é leve, consultando só os eventos
//! com número de registro maior que o último visto.

use serde::Serialize;

/// Provedores e ids de "o driver de vídeo parou de responder e se recuperou".
const QUERY: &str = "(Provider[@Name='Display'] and EventID=4101) or \
                     (Provider[@Name='nvlddmkm'] and (EventID=13 or EventID=14 or EventID=153)) or \
                     Provider[@Name='amdkmdag'] or Provider[@Name='amdwddmg']";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlackScreen {
    pub record_id: u64,
    /// Horário UTC do evento (ISO 8601).
    pub time_utc: String,
    pub provider: String,
    pub event_id: u32,
    /// Programa em primeiro plano quando o evento foi visto.
    pub foreground_app: Option<String>,
}

/// Extrai os eventos da saída XML do `wevtutil qe ... /f:xml`.
pub fn parse_events(xml: &str) -> Vec<BlackScreen> {
    xml.split("<Event ")
        .skip(1)
        .filter_map(|ev| {
            let record_id = between(ev, "<EventRecordID>", "</EventRecordID>")?.trim().parse().ok()?;
            let event_id = between(ev, "<EventID", "</EventID>")
                .and_then(|s| s.rsplit('>').next())
                .and_then(|s| s.trim().parse().ok())?;
            let provider = between(ev, "<Provider Name='", "'")
                .or_else(|| between(ev, "<Provider Name=\"", "\""))
                .unwrap_or_default()
                .to_string();
            let time_utc = between(ev, "SystemTime='", "'")
                .or_else(|| between(ev, "SystemTime=\"", "\""))
                .unwrap_or_default()
                .to_string();
            Some(BlackScreen {
                record_id,
                time_utc,
                provider,
                event_id,
                foreground_app: None,
            })
        })
        .collect()
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)? + i;
    Some(&s[i..j])
}

fn wevtutil(xpath: &str, count: u32, newest_first: bool) -> Result<String, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let q = format!("/q:*[System[{xpath}]]");
    let c = format!("/c:{count}");
    let mut args = vec!["qe", "System", q.as_str(), c.as_str(), "/f:xml"];
    if newest_first {
        args.push("/rd:true");
    }
    crate::shell::run("wevtutil.exe", &args)
}

/// Número de registro do evento de vídeo mais recente (0 se não houver).
pub fn latest_record_id() -> Result<u64, String> {
    let out = wevtutil(&format!("({QUERY})"), 1, true)?;
    Ok(parse_events(&out).first().map(|e| e.record_id).unwrap_or(0))
}

/// Eventos novos depois de `after`, do mais antigo ao mais novo, com o
/// programa em primeiro plano agora.
pub fn new_events(after: u64) -> Result<Vec<BlackScreen>, String> {
    let out = wevtutil(&format!("({QUERY}) and EventRecordID>{after}"), 20, false)?;
    let mut events = parse_events(&out);
    if !events.is_empty() {
        let app = foreground_app();
        for e in &mut events {
            e.foreground_app = app.clone();
        }
    }
    Ok(events)
}

/// Nome do executável da janela em primeiro plano.
pub fn foreground_app() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(str::to_string)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Display'/><EventID Qualifiers='0'>4101</EventID><Version>0</Version><Level>3</Level><TimeCreated SystemTime='2026-11-26T01:14:22.1234567Z'/><EventRecordID>98231</EventRecordID></System></Event>
<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='nvlddmkm' Guid='{x}'/><EventID>153</EventID><TimeCreated SystemTime='2026-11-26T01:14:23.0000000Z'/><EventRecordID>98232</EventRecordID></System></Event>"#;

    #[test]
    fn parses_wevtutil_xml() {
        let ev = parse_events(SAMPLE);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].record_id, 98231);
        assert_eq!(ev[0].event_id, 4101);
        assert_eq!(ev[0].provider, "Display");
        assert_eq!(ev[0].time_utc, "2026-11-26T01:14:22.1234567Z");
        assert_eq!(ev[1].provider, "nvlddmkm");
        assert_eq!(ev[1].event_id, 153);
        assert!(parse_events("").is_empty());
        assert!(parse_events("Nenhum evento encontrado.").is_empty());
    }
}
