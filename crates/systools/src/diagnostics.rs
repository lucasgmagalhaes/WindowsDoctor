//! Relatório do sistema e verificações de problemas comuns.
//!
//! A coleta ([`collect`]) roda um script do PowerShell somente leitura; a
//! avaliação ([`evaluate`]) é pura e testável em qualquer sistema.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- dados brutos

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawSystem {
    pub os: String,
    pub cpu: String,
    pub ram_total: u64,
    pub ram_free: u64,
    pub uptime_secs: u64,
    pub gpus: Vec<Gpu>,
    pub monitors: Vec<RawMonitor>,
    pub events: Vec<Event>,
    pub disks: Vec<PhysicalDisk>,
    pub system_drive: String,
    pub system_total: u64,
    pub system_free: u64,
    pub mpo_disabled: bool,
    pub fast_startup: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Gpu {
    pub name: String,
    pub driver_version: String,
    pub driver_date: String,
    pub driver_age_days: Option<u32>,
    pub refresh_rate: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawMonitor {
    pub manufacturer: String,
    pub name: String,
    pub connection: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Event {
    pub provider: String,
    pub id: u32,
    pub age_hours: f64,
    pub time: String,
    /// Para "Application Error", o nome do programa que travou.
    pub detail: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhysicalDisk {
    pub name: String,
    pub media_type: String,
    pub health: String,
    pub size: u64,
}

// ---------------------------------------------------------------- relatório

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Info,
    Warn,
    Crit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    pub status: Status,
    pub summary: String,
    pub details: Vec<String>,
    /// O que fazer, em ordem.
    pub actions: Vec<String>,
    /// Otimização do módulo Otimizações que resolve ou ajuda.
    pub tweak: Option<&'static str>,
    /// Módulo do app que ajuda ("cleanup" ou "diskmap").
    pub module: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub manufacturer: String,
    pub name: String,
    pub connection: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub os: String,
    pub cpu: String,
    pub ram_total: u64,
    pub ram_free: u64,
    pub uptime_secs: u64,
    pub gpus: Vec<Gpu>,
    pub monitors: Vec<Monitor>,
    pub disks: Vec<PhysicalDisk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub summary: Summary,
    /// Ordenadas da mais grave para a menos grave.
    pub checks: Vec<Check>,
}

// ---------------------------------------------------------------- coleta

const SCRIPT: &str = include_str!("diagnostics.ps1");

/// Coleta os dados do sistema (leva alguns segundos).
pub fn collect() -> Result<RawSystem, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let out = crate::shell::powershell(SCRIPT)?;
    let json = out.trim_start_matches('\u{feff}').trim();
    serde_json::from_str(json).map_err(|e| format!("resposta inesperada do PowerShell: {e}"))
}

pub fn run() -> Result<Report, String> {
    collect().map(|raw| evaluate(&raw))
}

// ---------------------------------------------------------------- avaliação

const WEEK_HOURS: f64 = 24.0 * 7.0;

fn connection_name(code: Option<i64>) -> String {
    match code {
        Some(-1) => "Outra".into(),
        Some(0) => "VGA".into(),
        Some(4) => "DVI".into(),
        Some(5) => "HDMI".into(),
        Some(6) => "LVDS".into(),
        Some(10) => "DisplayPort".into(),
        Some(11) => "DisplayPort (embutido)".into(),
        Some(15) => "Miracast (sem fio)".into(),
        Some(16) => "Indireto".into(),
        Some(0x8000_0000) | Some(-2_147_483_648) => "Tela interna".into(),
        Some(c) => format!("código {c}"),
        None => "desconhecida".into(),
    }
}

fn is_display_event(e: &Event) -> bool {
    matches!(
        (e.provider.as_str(), e.id),
        ("Display", 4101) | ("nvlddmkm", 13 | 14 | 153) | ("amdkmdag" | "amdwddmg", _)
    )
}

fn is_disk_event(e: &Event) -> bool {
    matches!(e.provider.as_str(), "disk" | "Ntfs" | "Microsoft-Windows-Ntfs")
}

fn recent<'a>(events: impl Iterator<Item = &'a Event>) -> (usize, usize, Vec<String>) {
    let list: Vec<&Event> = events.collect();
    let week = list.iter().filter(|e| e.age_hours <= WEEK_HOURS).count();
    let mut sorted = list.clone();
    sorted.sort_by(|a, b| a.age_hours.total_cmp(&b.age_hours));
    let last = sorted
        .iter()
        .take(5)
        .map(|e| format!("{}  ({} {})", e.time, e.provider, e.id))
        .collect();
    (week, list.len(), last)
}

/// Número com uma casa decimal e vírgula, como no português.
fn dec1(x: f64) -> String {
    format!("{x:.1}").replace('.', ",")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Adaptadores que não são uma placa de vídeo física (básico, remoto,
/// máquina virtual). Os drivers deles vêm com o Windows e têm datas antigas.
fn is_virtual_or_basic(g: &Gpu) -> bool {
    let n = g.name.to_lowercase();
    [
        "basic display",
        "basic render",
        "remote display",
        "parsec",
        "hyper-v",
        "vmware",
        "virtualbox",
        "virtual display",
        "citrix",
        "qxl",
        "spice",
    ]
    .iter()
    .any(|k| n.contains(k))
}

fn real_gpus(raw: &RawSystem) -> impl Iterator<Item = &Gpu> {
    raw.gpus.iter().filter(|g| !is_virtual_or_basic(g))
}

pub fn evaluate(raw: &RawSystem) -> Report {
    let mut checks = vec![
        check_display_driver(raw),
        check_driver_age(raw),
        check_mpo(raw),
        check_whea(raw),
        check_kernel_power(raw),
        check_disks(raw),
        check_free_space(raw),
        check_memory(raw),
        check_uptime(raw),
        check_app_crashes(raw),
    ];
    checks.sort_by_key(|c| std::cmp::Reverse(c.status));
    Report {
        summary: Summary {
            os: raw.os.clone(),
            cpu: raw.cpu.clone(),
            ram_total: raw.ram_total,
            ram_free: raw.ram_free,
            uptime_secs: raw.uptime_secs,
            gpus: raw.gpus.clone(),
            monitors: raw
                .monitors
                .iter()
                .map(|m| Monitor {
                    manufacturer: m.manufacturer.clone(),
                    name: m.name.clone(),
                    connection: connection_name(m.connection),
                })
                .collect(),
            disks: raw.disks.clone(),
        },
        checks,
    }
}

fn check_display_driver(raw: &RawSystem) -> Check {
    let (week, month, last) = recent(raw.events.iter().filter(|e| is_display_event(e)));
    let status = if week >= 3 {
        Status::Crit
    } else if month > 0 {
        Status::Warn
    } else {
        Status::Ok
    };
    let summary = if month == 0 {
        "Nenhuma falha do driver de vídeo nos últimos 30 dias. Se a tela apaga, a causa provável \
         é cabo, monitor ou sincronização (VRR/HDR)."
            .to_string()
    } else {
        format!(
            "O driver de vídeo travou e se recuperou {} nos últimos 30 dias ({} nos últimos 7). \
             Cada recuperação deixa a tela preta por cerca de 2 segundos.",
            plural(month, "vez", "vezes"),
            week
        )
    };
    let mut actions = Vec::new();
    if month > 0 {
        actions.extend([
            "Compare os horários abaixo com os momentos em que a tela apagou.".to_string(),
            "Faça uma instalação limpa do driver de vídeo (DDU no Modo de Segurança + driver do site do fabricante)."
                .into(),
            "Remova overclock/undervolt da placa de vídeo e verifique a temperatura dela em jogo.".into(),
            "Se persistir, teste outra fonte ou outro slot PCIe; quedas de energia também causam isso.".into(),
        ]);
    }
    actions.extend([
        "Troque o cabo (DisplayPort certificado VESA ou HDMI Ultra High Speed) e teste outra porta da placa."
            .to_string(),
        "No menu do monitor, desligue Adaptive-Sync/FreeSync/G-SYNC e o overclock de taxa de atualização para testar."
            .into(),
        "Desligue o HDR no Windows (Configurações > Sistema > Tela) para testar.".into(),
        "No menu do monitor, desligue a detecção automática de entrada e o modo de economia do DisplayPort, se houver."
            .into(),
    ]);
    Check {
        id: "display_driver",
        title: "Tela preta / driver de vídeo",
        status,
        summary,
        details: last,
        actions,
        tweak: Some("disable_mpo"),
        module: None,
    }
}

fn check_driver_age(raw: &RawSystem) -> Check {
    let basic = raw
        .gpus
        .iter()
        .any(|g| g.name.to_lowercase().contains("basic display"));
    let oldest = real_gpus(raw)
        .filter_map(|g| g.driver_age_days.map(|d| (d, g)))
        .max_by_key(|(d, _)| *d);
    let details = real_gpus(raw)
        .map(|g| format!("{}: driver {} de {}", g.name, g.driver_version, g.driver_date))
        .collect();
    let (status, summary) = if basic && real_gpus(raw).next().is_none() {
        (
            Status::Crit,
            "O Windows está usando o \"Adaptador de Vídeo Básico\": nenhum driver da placa de vídeo está instalado."
                .to_string(),
        )
    } else {
        match oldest {
            Some((d, g)) if d > 365 => (
                Status::Warn,
                format!("O driver de {} tem {} meses.", g.name, d / 30),
            ),
            Some((d, g)) if d > 180 => (
                Status::Info,
                format!("O driver de {} tem {} meses.", g.name, d / 30),
            ),
            Some(_) => (Status::Ok, "Driver de vídeo recente.".to_string()),
            None if real_gpus(raw).next().is_none() => (
                Status::Ok,
                "Só há adaptador de vídeo virtual ou remoto (máquina virtual ou acesso remoto); nada a verificar."
                    .to_string(),
            ),
            None => (Status::Info, "Não foi possível ler a data do driver.".to_string()),
        }
    };
    Check {
        id: "driver_age",
        title: "Driver da placa de vídeo",
        status,
        summary,
        details,
        actions: if status == Status::Ok {
            vec![]
        } else {
            vec!["Baixe o driver mais recente no site da NVIDIA, AMD ou Intel (o do Windows Update costuma ser antigo).".into()]
        },
        tweak: None,
        module: None,
    }
}

fn check_mpo(raw: &RawSystem) -> Check {
    let nvidia = real_gpus(raw).any(|g| g.name.to_lowercase().contains("nvidia"));
    let (status, summary) = if raw.mpo_disabled {
        (Status::Ok, "MPO (Multiplane Overlay) desativado.".to_string())
    } else if nvidia {
        (
            Status::Info,
            "O MPO (Multiplane Overlay) está ativado. Em placas NVIDIA ele é uma causa conhecida de \
             piscadas e telas pretas rápidas, principalmente com vários monitores ou taxas diferentes."
                .to_string(),
        )
    } else {
        (Status::Ok, "MPO (Multiplane Overlay) ativado, o padrão do Windows.".to_string())
    };
    Check {
        id: "mpo",
        title: "Multiplane Overlay (piscadas)",
        status,
        summary,
        details: vec![],
        actions: if status == Status::Info {
            vec!["Se você tem piscadas ou telas pretas, desative o MPO e reinicie. É reversível.".into()]
        } else {
            vec![]
        },
        tweak: if raw.mpo_disabled { None } else { Some("disable_mpo") },
        module: None,
    }
}

fn check_whea(raw: &RawSystem) -> Check {
    let (week, month, last) = recent(
        raw.events
            .iter()
            .filter(|e| e.provider == "Microsoft-Windows-WHEA-Logger"),
    );
    let status = match (week, month) {
        (w, _) if w > 0 => Status::Crit,
        (_, m) if m > 0 => Status::Warn,
        _ => Status::Ok,
    };
    Check {
        id: "whea",
        title: "Erros de hardware (WHEA)",
        status,
        summary: if month == 0 {
            "Nenhum erro de hardware registrado nos últimos 30 dias.".into()
        } else {
            format!(
                "O processador, a memória ou o barramento PCIe registraram {} nos últimos 30 dias.",
                plural(month, "erro de hardware", "erros de hardware")
            )
        },
        details: last,
        actions: if month == 0 {
            vec![]
        } else {
            vec![
                "Desative overclock, undervolt e o perfil XMP/EXPO da memória para testar.".into(),
                "Atualize a BIOS da placa-mãe.".into(),
                "Teste a memória com o Diagnóstico de Memória do Windows (mdsched.exe).".into(),
                "Verifique as temperaturas do processador e da placa de vídeo.".into(),
            ]
        },
        tweak: None,
        module: None,
    }
}

fn check_kernel_power(raw: &RawSystem) -> Check {
    let (_, month, last) = recent(
        raw.events
            .iter()
            .filter(|e| e.provider == "Microsoft-Windows-Kernel-Power" && e.id == 41),
    );
    let status = match month {
        0 => Status::Ok,
        1..=2 => Status::Warn,
        _ => Status::Crit,
    };
    Check {
        id: "kernel_power",
        title: "Desligamentos inesperados",
        status,
        summary: if month == 0 {
            "Nenhum desligamento inesperado nos últimos 30 dias.".into()
        } else {
            format!(
                "O PC desligou ou reiniciou sem um desligamento normal {} nos últimos 30 dias \
                 (travamento, tela azul, queda de energia ou botão de energia).",
                plural(month, "vez", "vezes")
            )
        },
        details: last,
        actions: if month == 0 {
            vec![]
        } else {
            vec![
                "Se o PC desligou sozinho em jogo ou carga alta, suspeite da fonte ou de superaquecimento.".into(),
                "Veja se houve tela azul: os arquivos ficam em C:\\Windows\\Minidump.".into(),
                "Desative a Inicialização Rápida em Otimizações se os desligamentos acontecem ao ligar o PC.".into(),
            ]
        },
        tweak: if month > 0 && raw.fast_startup {
            Some("disable_fast_startup")
        } else {
            None
        },
        module: None,
    }
}

fn check_disks(raw: &RawSystem) -> Check {
    let (_, month, last) = recent(raw.events.iter().filter(|e| is_disk_event(e)));
    let unverified = raw.disks.is_empty()
        || raw.disks.iter().any(|d| {
            d.health.trim().is_empty() || d.health.eq_ignore_ascii_case("unknown")
        });
    let bad: Vec<&PhysicalDisk> = raw
        .disks
        .iter()
        .filter(|d| {
            !d.health.trim().is_empty()
                && !d.health.eq_ignore_ascii_case("healthy")
                && !d.health.eq_ignore_ascii_case("unknown")
        })
        .collect();
    let mut details: Vec<String> = raw
        .disks
        .iter()
        .map(|d| format!("{} ({}): {}", d.name, d.media_type, translate_health(&d.health)))
        .collect();
    details.extend(last);
    let status = if !bad.is_empty() || month >= 3 {
        Status::Crit
    } else if month > 0 {
        Status::Warn
    } else if unverified {
        Status::Info
    } else {
        Status::Ok
    };
    let summary = if !bad.is_empty() {
        format!(
            "{} com problema de saúde. Faça backup dos seus arquivos agora.",
            bad.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
        )
    } else if month > 0 {
        format!(
            "{} de disco ou de sistema de arquivos nos últimos 30 dias.",
            plural(month, "erro", "erros")
        )
    } else if unverified {
        "Não foi possível verificar a saúde de todos os discos.".into()
    } else {
        "Discos saudáveis e sem erros nos últimos 30 dias.".into()
    };
    Check {
        id: "disks",
        title: "Saúde dos discos",
        status,
        summary,
        details,
        actions: match status {
            Status::Ok => vec![],
            Status::Info => vec!["Confira a saúde dos discos com a ferramenta do fabricante.".into()],
            Status::Warn | Status::Crit => vec![
                "Faça backup dos arquivos importantes.".into(),
                "Rode \"chkdsk C: /scan\" num terminal como administrador.".into(),
                "Confira o cabo SATA ou o encaixe do SSD M.2 e veja os dados SMART com o CrystalDiskInfo.".into(),
            ],
        },
        tweak: None,
        module: None,
    }
}

fn translate_health(h: &str) -> &str {
    match h {
        "Healthy" => "saudável",
        "Warning" => "atenção",
        "Unhealthy" => "com defeito",
        "Unknown" => "desconhecida",
        "" => "desconhecida",
        other => other,
    }
}

fn check_free_space(raw: &RawSystem) -> Check {
    let pct = if raw.system_total > 0 {
        raw.system_free as f64 / raw.system_total as f64
    } else {
        1.0
    };
    let status = if raw.system_total == 0 {
        Status::Info
    } else if pct < 0.05 {
        Status::Crit
    } else if pct < 0.10 {
        Status::Warn
    } else {
        Status::Ok
    };
    let gib = raw.system_free as f64 / 1024f64.powi(3);
    Check {
        id: "free_space",
        title: "Espaço na unidade do sistema",
        status,
        summary: if raw.system_total == 0 {
            "Não foi possível ler o espaço livre.".into()
        } else {
            format!(
                "{} tem {} GB livres ({:.0}%). Abaixo de 10% o Windows fica lento e as atualizações falham.",
                raw.system_drive,
                dec1(gib),
                pct * 100.0
            )
        },
        details: vec![],
        actions: if status >= Status::Warn {
            vec![
                "Use a Limpeza para apagar temporários e caches.".into(),
                "Use o Mapa do disco para achar o que ocupa mais espaço.".into(),
            ]
        } else {
            vec![]
        },
        tweak: None,
        module: (status >= Status::Warn).then_some("cleanup"),
    }
}

fn check_memory(raw: &RawSystem) -> Check {
    let used = raw.ram_total.saturating_sub(raw.ram_free);
    let pct = if raw.ram_total > 0 {
        used as f64 / raw.ram_total as f64
    } else {
        0.0
    };
    let gib = |b: u64| b as f64 / 1024f64.powi(3);
    let status = if pct > 0.9 {
        Status::Warn
    } else if raw.ram_total > 0 && raw.ram_total < 7 * 1024u64.pow(3) {
        Status::Info
    } else {
        Status::Ok
    };
    Check {
        id: "memory",
        title: "Memória",
        status,
        summary: format!(
            "{} GB de {} GB em uso ({:.0}%).",
            dec1(gib(used)),
            dec1(gib(raw.ram_total)),
            pct * 100.0
        ),
        details: vec![],
        actions: match status {
            Status::Warn => vec![
                "Feche programas que não está usando e veja os maiores consumidores no Gerenciador de Tarefas."
                    .into(),
                "Desative programas desnecessários na inicialização (em Otimizações).".into(),
            ],
            Status::Info => vec!["Com menos de 8 GB, o Windows 11 e navegadores com muitas abas ficam lentos.".into()],
            _ => vec![],
        },
        tweak: None,
        module: None,
    }
}

fn check_uptime(raw: &RawSystem) -> Check {
    let days = raw.uptime_secs / 86_400;
    let status = match days {
        0..=6 => Status::Ok,
        7..=29 => Status::Info,
        _ => Status::Warn,
    };
    let mut summary = format!("Último reinício há {}.", plural(days as usize, "dia", "dias"));
    if days >= 7 && raw.fast_startup {
        summary.push_str(
            " Com a Inicialização Rápida ligada, \"Desligar\" não zera o sistema; só \"Reiniciar\" zera.",
        );
    }
    Check {
        id: "uptime",
        title: "Tempo ligado",
        status,
        summary,
        details: vec![],
        actions: if status == Status::Ok {
            vec![]
        } else {
            vec!["Reinicie o PC (Reiniciar, não Desligar) para aplicar atualizações e liberar recursos.".into()]
        },
        tweak: if status != Status::Ok && raw.fast_startup {
            Some("disable_fast_startup")
        } else {
            None
        },
        module: None,
    }
}

fn check_app_crashes(raw: &RawSystem) -> Check {
    let crashes: Vec<&Event> = raw
        .events
        .iter()
        .filter(|e| e.provider == "Application Error" && e.id == 1000 && e.age_hours <= WEEK_HOURS)
        .collect();
    let mut by_app: Vec<(String, usize)> = Vec::new();
    for e in &crashes {
        let app = if e.detail.is_empty() { "desconhecido" } else { &e.detail };
        match by_app.iter_mut().find(|(a, _)| a.eq_ignore_ascii_case(app)) {
            Some((_, n)) => *n += 1,
            None => by_app.push((app.to_string(), 1)),
        }
    }
    by_app.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let status = match crashes.len() {
        0 => Status::Ok,
        1..=4 => Status::Info,
        _ => Status::Warn,
    };
    Check {
        id: "app_crashes",
        title: "Programas que travaram",
        status,
        summary: if crashes.is_empty() {
            "Nenhum programa travou nos últimos 7 dias.".into()
        } else {
            format!(
                "{} nos últimos 7 dias.",
                plural(crashes.len(), "travamento de programa", "travamentos de programas")
            )
        },
        details: by_app
            .iter()
            .take(8)
            .map(|(app, n)| format!("{app}: {}", plural(*n, "vez", "vezes")))
            .collect(),
        actions: if status == Status::Ok {
            vec![]
        } else {
            vec![
                "Atualize ou reinstale os programas que mais travam.".into(),
                "Se vários programas diferentes travam, rode \"sfc /scannow\" como administrador e teste a memória."
                    .into(),
            ]
        },
        tweak: None,
        module: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(provider: &str, id: u32, age_hours: f64) -> Event {
        Event {
            provider: provider.into(),
            id,
            age_hours,
            time: format!("t-{age_hours}"),
            detail: String::new(),
        }
    }

    fn base() -> RawSystem {
        RawSystem {
            ram_total: 16 * 1024u64.pow(3),
            ram_free: 8 * 1024u64.pow(3),
            uptime_secs: 3600,
            system_drive: "C:".into(),
            system_total: 500 * 1024u64.pow(3),
            system_free: 200 * 1024u64.pow(3),
            gpus: vec![Gpu {
                name: "NVIDIA GeForce RTX 4070".into(),
                driver_age_days: Some(20),
                ..Gpu::default()
            }],
            ..RawSystem::default()
        }
    }

    fn get<'a>(r: &'a Report, id: &str) -> &'a Check {
        r.checks.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn parses_powershell_json_with_single_items_and_nulls() {
        let json = r#"{"os":"Windows 11 Pro 10.0.22631","cpu":"Ryzen","ramTotal":17179869184,
            "ramFree":1,"uptimeSecs":10,"gpus":[{"name":"X","driverVersion":"1","driverDate":"",
            "driverAgeDays":null,"refreshRate":144,"width":2560,"height":1440}],
            "monitors":[{"manufacturer":"AUS","name":"VG27AQ","connection":10}],
            "events":[],"disks":[],"systemDrive":"C:","systemTotal":0,"systemFree":0,
            "mpoDisabled":false,"fastStartup":true,"extra":1}"#;
        let raw: RawSystem = serde_json::from_str(json).unwrap();
        let r = evaluate(&raw);
        assert_eq!(r.summary.monitors[0].connection, "DisplayPort");
        assert_eq!(raw.gpus[0].driver_age_days, None);
    }

    #[test]
    fn healthy_system_has_no_warnings() {
        let r = evaluate(&base());
        let worst = r.checks.iter().map(|c| c.status).max().unwrap();
        assert!(worst <= Status::Info, "{:?}", r.checks);
        // NVIDIA com MPO ligado gera só uma dica.
        assert_eq!(get(&r, "mpo").status, Status::Info);
        assert_eq!(get(&r, "mpo").tweak, Some("disable_mpo"));
    }

    #[test]
    fn display_driver_resets_are_reported_by_recency() {
        let mut raw = base();
        raw.events = vec![ev("Display", 4101, 300.0)];
        assert_eq!(get(&evaluate(&raw), "display_driver").status, Status::Warn);

        raw.events = vec![
            ev("Display", 4101, 2.0),
            ev("nvlddmkm", 153, 30.0),
            ev("Display", 4101, 100.0),
            ev("Display", 9999, 1.0),
        ];
        let r = evaluate(&raw);
        let c = get(&r, "display_driver");
        assert_eq!(c.status, Status::Crit);
        assert_eq!(c.details.len(), 3);
        assert!(c.details[0].starts_with("t-2"), "mais recente primeiro: {:?}", c.details);
        // A verificação mais grave vem primeiro.
        assert_eq!(r.checks[0].id, "display_driver");
    }

    #[test]
    fn basic_display_adapter_means_no_driver() {
        let mut raw = base();
        raw.gpus = vec![Gpu {
            name: "Microsoft Basic Display Adapter".into(),
            ..Gpu::default()
        }];
        assert_eq!(get(&evaluate(&raw), "driver_age").status, Status::Crit);
    }

    #[test]
    fn virtual_adapters_are_not_checked_for_driver_age() {
        let mut raw = base();
        raw.gpus = vec![Gpu {
            name: "Microsoft Hyper-V Video".into(),
            driver_age_days: Some(7450),
            ..Gpu::default()
        }];
        assert_eq!(get(&evaluate(&raw), "driver_age").status, Status::Ok);
    }

    #[test]
    fn old_driver_warns() {
        let mut raw = base();
        raw.gpus[0].driver_age_days = Some(400);
        assert_eq!(get(&evaluate(&raw), "driver_age").status, Status::Warn);
    }

    #[test]
    fn unhealthy_disk_is_critical() {
        let mut raw = base();
        raw.disks = vec![PhysicalDisk {
            name: "Samsung SSD".into(),
            media_type: "SSD".into(),
            health: "Warning".into(),
            size: 1,
        }];
        let c = get(&evaluate(&raw), "disks").clone();
        assert_eq!(c.status, Status::Crit);
        assert!(c.summary.contains("Samsung SSD"));
    }

    #[test]
    fn unavailable_disk_health_is_not_reported_as_healthy() {
        let mut raw = base();
        let c = get(&evaluate(&raw), "disks").clone();
        assert_eq!(c.status, Status::Info);
        assert!(c.summary.contains("Não foi possível verificar"));

        raw.disks = vec![PhysicalDisk {
            name: "SSD".into(),
            health: "Unknown".into(),
            ..PhysicalDisk::default()
        }];
        assert_eq!(get(&evaluate(&raw), "disks").status, Status::Info);

        raw.events.push(ev("disk", 7, 1.0));
        assert_eq!(get(&evaluate(&raw), "disks").status, Status::Warn);

        raw.events.clear();
        raw.disks[0].health = "Healthy".into();
        assert_eq!(get(&evaluate(&raw), "disks").status, Status::Ok);
    }

    #[test]
    fn low_free_space_points_to_cleanup() {
        let mut raw = base();
        raw.system_free = raw.system_total / 25;
        let r = evaluate(&raw);
        let c = get(&r, "free_space");
        assert_eq!(c.status, Status::Crit);
        assert_eq!(c.module, Some("cleanup"));
        assert!(c.summary.contains("20,0 GB"), "{}", c.summary);
    }

    #[test]
    fn app_crashes_are_grouped_by_program() {
        let mut raw = base();
        for (app, age) in [("game.exe", 1.0), ("game.exe", 2.0), ("chrome.exe", 3.0), ("old.exe", 500.0)] {
            let mut e = ev("Application Error", 1000, age);
            e.detail = app.into();
            raw.events.push(e);
        }
        let c = get(&evaluate(&raw), "app_crashes").clone();
        assert_eq!(c.status, Status::Info);
        assert_eq!(c.details, vec!["game.exe: 2 vezes", "chrome.exe: 1 vez"]);
    }

    #[test]
    fn long_uptime_with_fast_startup_suggests_disabling_it() {
        let mut raw = base();
        raw.uptime_secs = 40 * 86_400;
        raw.fast_startup = true;
        let c = get(&evaluate(&raw), "uptime").clone();
        assert_eq!(c.status, Status::Warn);
        assert_eq!(c.tweak, Some("disable_fast_startup"));
    }
}
