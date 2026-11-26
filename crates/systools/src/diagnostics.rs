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
    pub devices: Vec<Device>,
    pub minidumps: Vec<Minidump>,
    pub temps: Vec<Temperature>,
    pub pending_reboot: bool,
    pub battery: Option<Battery>,
}

/// Dispositivo com erro no Gerenciador de Dispositivos.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    pub name: String,
    pub code: u32,
    pub class: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Minidump {
    pub name: String,
    pub time: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Battery {
    pub name: String,
    /// mWh
    pub design_capacity: u64,
    /// mWh
    pub full_charge_capacity: u64,
    pub cycle_count: Option<u32>,
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
    /// Contadores SMART (só com administrador).
    pub temperature: Option<i32>,
    /// Percentual de vida útil consumida (SSD).
    pub wear: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
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
    pub temps: Vec<Temperature>,
    pub battery: Option<Battery>,
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
        check_bsod(raw),
        check_devices(raw),
        check_temperatures(raw),
        check_windows_update(raw),
    ];
    checks.extend(check_battery(raw));
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
            temps: raw.temps.clone(),
            battery: raw.battery.clone(),
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
    // SMART: desgaste do SSD, temperatura e erros não corrigidos.
    let mut smart_warn = Vec::new();
    let mut smart_crit = Vec::new();
    for d in &raw.disks {
        if let Some(w) = d.wear {
            if w >= 95 {
                smart_crit.push(format!("{}: {w}% da vida útil consumida", d.name));
            } else if w >= 80 {
                smart_warn.push(format!("{}: {w}% da vida útil consumida", d.name));
            }
        }
        if let Some(t) = d.temperature.filter(|t| *t >= 65) {
            smart_warn.push(format!("{}: {t} °C", d.name));
        }
        let errors = d.read_errors.unwrap_or(0) + d.write_errors.unwrap_or(0);
        if errors > 0 {
            smart_crit.push(format!("{}: {errors} erros de leitura/escrita não corrigidos", d.name));
        }
        let mut parts = Vec::new();
        if let Some(t) = d.temperature {
            parts.push(format!("{t} °C"));
        }
        if let Some(w) = d.wear {
            parts.push(format!("desgaste {w}%"));
        }
        if let Some(h) = d.power_on_hours {
            parts.push(format!("{h} h ligado"));
        }
        if !parts.is_empty() {
            details.push(format!("{} (SMART): {}", d.name, parts.join(", ")));
        }
    }
    details.extend(smart_crit.iter().chain(&smart_warn).map(|m| format!("⚠ {m}")));
    let status = if !smart_crit.is_empty() || !bad.is_empty() || month >= 3 {
        Status::Crit
    } else if month > 0 || !smart_warn.is_empty() {
        Status::Warn
    } else if unverified {
        Status::Info
    } else {
        Status::Ok
    };
    let summary = if !smart_crit.is_empty() {
        format!("{}. Faça backup dos seus arquivos agora.", smart_crit.join("; "))
    } else if !bad.is_empty() {
        format!(
            "{} com problema de saúde. Faça backup dos seus arquivos agora.",
            bad.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
        )
    } else if month > 0 {
        format!(
            "{} de disco ou de sistema de arquivos nos últimos 30 dias.",
            plural(month, "erro", "erros")
        )
    } else if !smart_warn.is_empty() {
        format!("{}.", smart_warn.join("; "))
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

// ---------------------------------------------------------------- telas azuis

/// Códigos de parada mais comuns: nome e o que costuma causar.
fn stop_code_hint(code: u32) -> Option<(&'static str, &'static str)> {
    Some(match code {
        0x0A => ("IRQL_NOT_LESS_OR_EQUAL", "driver com defeito ou memória instável"),
        0x1A => ("MEMORY_MANAGEMENT", "memória RAM com defeito ou instável (XMP/EXPO)"),
        0x1E => ("KMODE_EXCEPTION_NOT_HANDLED", "driver com defeito"),
        0x3B => ("SYSTEM_SERVICE_EXCEPTION", "driver com defeito, frequentemente de vídeo"),
        0x50 => ("PAGE_FAULT_IN_NONPAGED_AREA", "memória RAM ou driver com defeito"),
        0x7E => ("SYSTEM_THREAD_EXCEPTION_NOT_HANDLED", "driver com defeito"),
        0x7F => ("UNEXPECTED_KERNEL_MODE_TRAP", "hardware instável (overclock, memória, temperatura)"),
        0x9F => ("DRIVER_POWER_STATE_FAILURE", "driver que não acorda da suspensão (USB, Wi-Fi, vídeo)"),
        0xC5 => ("DRIVER_CORRUPTED_EXPOOL", "driver com defeito"),
        0xD1 => ("DRIVER_IRQL_NOT_LESS_OR_EQUAL", "driver com defeito, frequentemente de rede"),
        0xEF => ("CRITICAL_PROCESS_DIED", "arquivos do sistema corrompidos ou disco com problema"),
        0x116 => ("VIDEO_TDR_FAILURE", "driver ou placa de vídeo (a mesma causa das telas pretas de 2 segundos)"),
        0x117 => ("VIDEO_TDR_TIMEOUT_DETECTED", "driver ou placa de vídeo"),
        0x119 => ("VIDEO_SCHEDULER_INTERNAL_ERROR", "driver de vídeo"),
        0x124 => ("WHEA_UNCORRECTABLE_ERROR", "erro de hardware: overclock, tensão, temperatura ou peça com defeito"),
        0x133 => ("DPC_WATCHDOG_VIOLATION", "driver de armazenamento ou firmware do SSD desatualizado"),
        0x139 => ("KERNEL_SECURITY_CHECK_FAILURE", "driver incompatível ou memória com defeito"),
        0x154 => ("UNEXPECTED_STORE_EXCEPTION", "SSD/HD com problema ou driver de armazenamento"),
        _ => return None,
    })
}

/// "0x00000116 (0xffff…, …)" ou "0x116" → 0x116.
fn parse_stop_code(detail: &str) -> Option<u32> {
    let hex = detail.trim().strip_prefix("0x").or_else(|| detail.trim().strip_prefix("0X"))?;
    let digits: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    u32::from_str_radix(&digits, 16).ok()
}

fn check_bsod(raw: &RawSystem) -> Check {
    let crashes: Vec<&Event> = raw
        .events
        .iter()
        .filter(|e| e.provider == "Microsoft-Windows-WER-SystemErrorReporting" && e.id == 1001)
        .collect();
    let (week, month, _) = recent(crashes.iter().copied());
    let mut sorted = crashes.clone();
    sorted.sort_by(|a, b| a.age_hours.total_cmp(&b.age_hours));
    let mut details: Vec<String> = sorted
        .iter()
        .take(8)
        .map(|e| match parse_stop_code(&e.detail) {
            Some(code) => match stop_code_hint(code) {
                Some((name, cause)) => format!("{}  0x{code:X} {name}: {cause}", e.time),
                None => format!("{}  0x{code:X}", e.time),
            },
            None => format!("{}  {}", e.time, e.detail),
        })
        .collect();
    if !raw.minidumps.is_empty() {
        details.push(format!(
            "Minidumps em C:\\Windows\\Minidump: {}",
            raw.minidumps.iter().map(|m| format!("{} ({})", m.name, m.time)).collect::<Vec<_>>().join(", ")
        ));
    }
    // A causa mais citada vira o resumo.
    let mut causes: Vec<(&str, usize)> = Vec::new();
    for e in &crashes {
        if let Some((name, _)) = parse_stop_code(&e.detail).and_then(stop_code_hint) {
            match causes.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => causes.push((name, 1)),
            }
        }
    }
    causes.sort_by_key(|c| std::cmp::Reverse(c.1));
    let status = if week > 0 {
        Status::Crit
    } else if month > 0 {
        Status::Warn
    } else {
        Status::Ok
    };
    let summary = if month == 0 {
        "Nenhuma tela azul nos últimos 30 dias.".to_string()
    } else {
        let mut s = format!("{} nos últimos 30 dias.", plural(month, "tela azul", "telas azuis"));
        if let Some((name, n)) = causes.first() {
            s.push_str(&format!(" A mais frequente é {name} ({}).", plural(*n, "vez", "vezes")));
        }
        s
    };
    let video = causes.iter().any(|(n, _)| n.starts_with("VIDEO_"));
    let mut actions = Vec::new();
    if month > 0 {
        if video {
            actions.push("As telas azuis apontam para o vídeo: faça uma instalação limpa do driver (DDU) e tire overclock da placa.".to_string());
        }
        actions.extend([
            "Veja a explicação de cada código nos detalhes abaixo.".to_string(),
            "Para descobrir o driver exato, abra os minidumps com o WinDbg (Microsoft Store) ou o BlueScreenView.".into(),
            "Atualize a BIOS, o chipset e os drivers de vídeo, rede e armazenamento.".into(),
            "Desative overclock e XMP/EXPO e teste a memória (Reparos > Teste de memória).".into(),
        ]);
    }
    Check {
        id: "bsod",
        title: "Telas azuis",
        status,
        summary,
        details,
        actions,
        tweak: None,
        module: None,
    }
}

// ---------------------------------------------------------------- dispositivos

fn device_error(code: u32) -> &'static str {
    match code {
        1 => "não está configurado corretamente",
        3 => "o driver está corrompido ou faltando memória",
        10 => "não pode ser iniciado",
        12 => "sem recursos livres (conflito)",
        14 => "precisa reiniciar o PC",
        18 => "precisa reinstalar o driver",
        19 => "configuração no Registro corrompida",
        21 => "sendo removido",
        22 => "desativado",
        24 => "não está presente ou não funciona",
        28 => "sem driver instalado",
        29 => "desativado pelo firmware (BIOS)",
        31 => "o driver não pôde ser carregado",
        32 => "o serviço do driver está desativado",
        37 => "o driver falhou ao iniciar",
        39 => "o driver está corrompido ou faltando",
        43 => "o Windows parou o dispositivo porque ele relatou problemas",
        48 => "o driver foi bloqueado por incompatibilidade",
        52 => "o driver não tem assinatura digital válida",
        _ => "com problema",
    }
}

fn check_devices(raw: &RawSystem) -> Check {
    let failing: Vec<&Device> = raw.devices.iter().filter(|d| d.code != 22).collect();
    let disabled = raw.devices.len() - failing.len();
    let status = if failing.is_empty() {
        if disabled > 0 {
            Status::Info
        } else {
            Status::Ok
        }
    } else {
        Status::Warn
    };
    let details = raw
        .devices
        .iter()
        .map(|d| {
            let class = if d.class.is_empty() { String::new() } else { format!(" [{}]", d.class) };
            format!("{}{class}: código {} — {}", d.name, d.code, device_error(d.code))
        })
        .collect();
    Check {
        id: "devices",
        title: "Dispositivos com problema",
        status,
        summary: if failing.is_empty() && disabled == 0 {
            "Todos os dispositivos estão funcionando.".into()
        } else if failing.is_empty() {
            format!("{} desativado no Gerenciador de Dispositivos.", plural(disabled, "dispositivo", "dispositivos"))
        } else {
            format!(
                "{} com erro no Gerenciador de Dispositivos.",
                plural(failing.len(), "dispositivo", "dispositivos")
            )
        },
        details,
        actions: if failing.is_empty() {
            vec![]
        } else {
            vec![
                "Código 28: instale o driver do fabricante do PC ou da placa-mãe.".into(),
                "Código 43 em vídeo ou USB: reinstale o driver e teste outra porta ou cabo.".into(),
                "Para os demais: Gerenciador de Dispositivos > botão direito > Desinstalar dispositivo e reinicie.".into(),
            ]
        },
        tweak: None,
        module: None,
    }
}

// ---------------------------------------------------------------- temperaturas

fn check_temperatures(raw: &RawSystem) -> Check {
    // Zonas ACPI fixas em ~27,8 °C ou acima de 120 °C são leituras falsas.
    let temps: Vec<&Temperature> = raw
        .temps
        .iter()
        .filter(|t| t.celsius > 0.0 && t.celsius < 120.0)
        .collect();
    let max = temps.iter().map(|t| t.celsius).fold(f64::NAN, f64::max);
    let status = if temps.is_empty() {
        Status::Info
    } else if max >= 90.0 {
        Status::Crit
    } else if max >= 80.0 {
        Status::Warn
    } else {
        Status::Ok
    };
    Check {
        id: "temperatures",
        title: "Temperaturas",
        status,
        summary: if temps.is_empty() {
            "O Windows não expõe sensores de temperatura neste PC (ou o app não está como administrador).".into()
        } else {
            format!("Maior temperatura agora: {} °C.", dec1(max))
        },
        details: temps.iter().map(|t| format!("{}: {} °C", t.label, dec1(t.celsius))).collect(),
        actions: match status {
            Status::Info => vec!["Use o HWiNFO ou o software da placa-mãe para ver as temperaturas reais.".into()],
            Status::Warn | Status::Crit => vec![
                "Limpe a poeira dos coolers e filtros e confira se todas as ventoinhas giram.".into(),
                "Em notebooks, use sobre uma superfície dura; troque a pasta térmica se ele tiver alguns anos.".into(),
                "Temperaturas altas em repouso indicam cooler mal encaixado ou bomba do water cooler parada.".into(),
            ],
            _ => vec![],
        },
        tweak: None,
        module: None,
    }
}

// ---------------------------------------------------------------- Windows Update

fn check_windows_update(raw: &RawSystem) -> Check {
    let failures: Vec<&Event> = raw
        .events
        .iter()
        .filter(|e| e.provider == "Microsoft-Windows-WindowsUpdateClient" && e.id == 20)
        .collect();
    let mut titles: Vec<String> = Vec::new();
    for e in &failures {
        let (code, title) = e.detail.split_once(" | ").unwrap_or(("", e.detail.as_str()));
        let code = code
            .trim()
            .parse::<i64>()
            .map(|c| format!("0x{:08X}", c as u32))
            .unwrap_or_else(|_| code.trim().to_string());
        let line = if code.is_empty() { title.to_string() } else { format!("{title} (erro {code})") };
        if !titles.contains(&line) {
            titles.push(line);
        }
    }
    let status = if failures.len() >= 3 {
        Status::Warn
    } else if !failures.is_empty() || raw.pending_reboot {
        Status::Info
    } else {
        Status::Ok
    };
    let mut summary = if failures.is_empty() {
        "Nenhuma atualização falhou nos últimos 30 dias.".to_string()
    } else {
        format!(
            "{} nos últimos 30 dias.",
            plural(failures.len(), "falha de instalação de atualização", "falhas de instalação de atualizações")
        )
    };
    if raw.pending_reboot {
        summary.push_str(" Há atualizações esperando o PC reiniciar.");
    }
    let mut actions = Vec::new();
    if raw.pending_reboot {
        actions.push("Reinicie o PC para terminar de instalar as atualizações.".to_string());
    }
    if !failures.is_empty() {
        actions.extend([
            "Rode a Solução de Problemas do Windows Update (Configurações > Sistema > Solução de problemas).".to_string(),
            "Se repetir, limpe o cache do Windows Update na Limpeza e rode os reparos SFC e DISM.".into(),
        ]);
    }
    Check {
        id: "windows_update",
        title: "Windows Update",
        status,
        summary,
        details: titles.into_iter().take(8).collect(),
        actions,
        tweak: None,
        module: (!failures.is_empty()).then_some("cleanup"),
    }
}

// ---------------------------------------------------------------- bateria

fn check_battery(raw: &RawSystem) -> Option<Check> {
    let b = raw.battery.as_ref().filter(|b| b.design_capacity > 0)?;
    let health = b.full_charge_capacity as f64 / b.design_capacity as f64;
    let status = if health < 0.6 {
        Status::Warn
    } else if health < 0.8 {
        Status::Info
    } else {
        Status::Ok
    };
    let mut details = vec![format!(
        "Capacidade original {} mWh, atual {} mWh",
        b.design_capacity, b.full_charge_capacity
    )];
    if let Some(c) = b.cycle_count {
        details.push(format!("{c} ciclos de carga"));
    }
    Some(Check {
        id: "battery",
        title: "Bateria",
        status,
        summary: format!(
            "A bateria{} guarda {:.0}% da carga que guardava nova.",
            if b.name.is_empty() { String::new() } else { format!(" ({})", b.name) },
            (health * 100.0).min(100.0)
        ),
        details,
        actions: match status {
            Status::Warn => vec!["Abaixo de 60% a autonomia cai muito; considere trocar a bateria.".into()],
            Status::Info => vec![
                "Para preservar a bateria, use o limite de carga em 80% do software do fabricante, se houver.".into(),
            ],
            _ => vec![],
        },
        tweak: None,
        module: None,
    })
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
            ..PhysicalDisk::default()
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

    #[test]
    fn blue_screens_are_explained_by_stop_code() {
        let mut raw = base();
        for (age, code) in [(5.0, "0x00000116 (0xffffa50f, 0x1)"), (30.0, "0x00000116 (0x0)"), (400.0, "0x0000001a (0x41790)")] {
            let mut e = ev("Microsoft-Windows-WER-SystemErrorReporting", 1001, age);
            e.detail = code.into();
            raw.events.push(e);
        }
        raw.minidumps = vec![Minidump { name: "112526-1.dmp".into(), time: "2026-11-25 20:00".into() }];
        let c = get(&evaluate(&raw), "bsod").clone();
        assert_eq!(c.status, Status::Crit);
        assert!(c.summary.contains("3 telas azuis"), "{}", c.summary);
        assert!(c.summary.contains("VIDEO_TDR_FAILURE (2 vezes)"), "{}", c.summary);
        assert!(c.details[0].contains("0x116 VIDEO_TDR_FAILURE"), "{:?}", c.details);
        assert!(c.details.last().unwrap().contains("112526-1.dmp"));
        assert!(c.actions[0].contains("DDU"));
        assert_eq!(parse_stop_code("0x9F"), Some(0x9F));
        assert_eq!(parse_stop_code("sem código"), None);
    }

    #[test]
    fn failing_devices_warn_but_disabled_ones_only_inform() {
        let mut raw = base();
        raw.devices = vec![Device { name: "Bluetooth".into(), code: 22, class: "Bluetooth".into() }];
        assert_eq!(get(&evaluate(&raw), "devices").status, Status::Info);
        raw.devices.push(Device { name: "USB Hub".into(), code: 43, class: "USB".into() });
        let c = get(&evaluate(&raw), "devices").clone();
        assert_eq!(c.status, Status::Warn);
        assert!(c.details[1].contains("relatou problemas"));
    }

    #[test]
    fn smart_wear_and_errors_raise_disk_status() {
        let mut raw = base();
        raw.disks = vec![PhysicalDisk {
            name: "NVMe".into(),
            health: "Healthy".into(),
            wear: Some(85),
            temperature: Some(41),
            power_on_hours: Some(9000),
            ..PhysicalDisk::default()
        }];
        let c = get(&evaluate(&raw), "disks").clone();
        assert_eq!(c.status, Status::Warn);
        assert!(c.details.iter().any(|d| d.contains("41 °C, desgaste 85%, 9000 h ligado")), "{:?}", c.details);
        raw.disks[0].read_errors = Some(3);
        assert_eq!(get(&evaluate(&raw), "disks").status, Status::Crit);
    }

    #[test]
    fn temperatures_ignore_bogus_readings() {
        let mut raw = base();
        assert_eq!(get(&evaluate(&raw), "temperatures").status, Status::Info);
        raw.temps = vec![
            Temperature { label: "ACPI".into(), celsius: -273.1 },
            Temperature { label: "RTX".into(), celsius: 84.0 },
        ];
        let c = get(&evaluate(&raw), "temperatures").clone();
        assert_eq!(c.status, Status::Warn);
        assert_eq!(c.details, vec!["RTX: 84,0 °C"]);
    }

    #[test]
    fn windows_update_failures_and_pending_reboot() {
        let mut raw = base();
        raw.pending_reboot = true;
        assert_eq!(get(&evaluate(&raw), "windows_update").status, Status::Info);
        let mut e = ev("Microsoft-Windows-WindowsUpdateClient", 20, 3.0);
        e.detail = "-2147024784 | 2026-11 Atualização Cumulativa".into();
        raw.events = vec![e.clone(), e.clone(), e];
        let c = get(&evaluate(&raw), "windows_update").clone();
        assert_eq!(c.status, Status::Warn);
        assert_eq!(c.details, vec!["2026-11 Atualização Cumulativa (erro 0x80070070)"]);
    }

    #[test]
    fn battery_health_only_when_present() {
        let mut raw = base();
        assert!(evaluate(&raw).checks.iter().all(|c| c.id != "battery"));
        raw.battery = Some(Battery { name: "LGC".into(), design_capacity: 50_000, full_charge_capacity: 27_000, cycle_count: Some(612) });
        let c = get(&evaluate(&raw), "battery").clone();
        assert_eq!(c.status, Status::Warn);
        assert!(c.summary.contains("54%"));
    }
}
