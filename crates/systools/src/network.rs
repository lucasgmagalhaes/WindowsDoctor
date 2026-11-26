//! Teste de rede sob demanda: roteador, internet, DNS e Wi-Fi.

use serde::{Deserialize, Serialize};

use crate::diagnostics::{Check, Status};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PingResult {
    pub host: String,
    pub label: String,
    pub sent: u32,
    pub received: u32,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

impl PingResult {
    pub fn loss_pct(&self) -> f64 {
        if self.sent == 0 {
            return 100.0;
        }
        100.0 * f64::from(self.sent - self.received.min(self.sent)) / f64::from(self.sent)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkRaw {
    pub gateway: Option<String>,
    pub adapter: Option<String>,
    pub link_speed: Option<String>,
    pub wifi: bool,
    pub wifi_signal: Option<u32>,
    pub ssid: Option<String>,
    pub targets: Vec<PingResult>,
    pub dns_ms: Option<f64>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkReport {
    pub raw: NetworkRaw,
    pub checks: Vec<Check>,
}

const SCRIPT: &str = include_str!("network.ps1");

/// Leva cerca de 15 segundos (20 pings para cada destino).
pub fn run() -> Result<NetworkReport, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let out = crate::shell::powershell(SCRIPT)?;
    let raw: NetworkRaw = serde_json::from_str(out.trim_start_matches('\u{feff}').trim())
        .map_err(|e| format!("resposta inesperada do PowerShell: {e}"))?;
    Ok(evaluate(raw))
}

fn check(id: &'static str, title: &'static str, status: Status, summary: String, actions: Vec<String>) -> Check {
    Check {
        id,
        title,
        status,
        summary,
        details: vec![],
        actions,
        tweak: None,
        module: None,
    }
}

fn ms(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.0} ms")).unwrap_or_else(|| "sem resposta".into())
}

pub fn evaluate(raw: NetworkRaw) -> NetworkReport {
    let mut checks = Vec::new();
    let router = raw.targets.iter().find(|t| t.label == "Roteador");
    let internet: Vec<&PingResult> = raw.targets.iter().filter(|t| t.label != "Roteador").collect();

    if raw.gateway.is_none() {
        checks.push(check(
            "net_connection",
            "Conexão",
            Status::Crit,
            "Sem rota para a internet: o PC não está conectado a nenhuma rede.".into(),
            vec![
                "Confira o cabo de rede ou conecte-se ao Wi-Fi.".into(),
                "Rode \"Redefinição de rede\" em Configurações > Rede e Internet > Configurações avançadas.".into(),
            ],
        ));
    }

    if let Some(r) = router {
        let loss = r.loss_pct();
        let (status, summary) = if loss >= 10.0 {
            (Status::Crit, format!("{loss:.0}% de perda até o roteador. O problema está na rede local (cabo, Wi-Fi ou roteador)."))
        } else if loss > 0.0 {
            (Status::Warn, format!("{loss:.0}% de perda até o roteador, média {}.", ms(r.avg_ms)))
        } else if r.avg_ms.unwrap_or(0.0) > 20.0 {
            (Status::Info, format!("Latência alta até o roteador: média {}, pico {}.", ms(r.avg_ms), ms(r.max_ms)))
        } else {
            (Status::Ok, format!("Roteador respondendo bem: média {}, sem perda.", ms(r.avg_ms)))
        };
        let actions = if status == Status::Ok {
            vec![]
        } else if raw.wifi {
            vec![
                "Aproxime o PC do roteador ou use a rede de 5 GHz.".into(),
                "Se possível, teste com cabo para confirmar se o problema é o Wi-Fi.".into(),
                "Atualize o driver do adaptador Wi-Fi.".into(),
            ]
        } else {
            vec!["Troque o cabo de rede e teste outra porta do roteador.".into(), "Reinicie o roteador.".into()]
        };
        checks.push(check("net_router", "Rede local (roteador)", status, summary, actions));
    }

    if raw.gateway.is_some() && !internet.is_empty() {
        let worst_loss = internet.iter().map(|t| t.loss_pct()).fold(0.0, f64::max);
        let best_avg = internet.iter().filter_map(|t| t.avg_ms).fold(f64::INFINITY, f64::min);
        let local_ok = router.is_none_or(|r| r.loss_pct() == 0.0);
        let (status, summary) = if internet.iter().all(|t| t.received == 0) {
            (Status::Crit, "A internet não responde, mas o roteador sim: o problema está no roteador ou no provedor.".to_string())
        } else if worst_loss >= 5.0 {
            (
                Status::Warn,
                format!(
                    "{worst_loss:.0}% de perda até a internet{}.",
                    if local_ok { " com a rede local sem perda: provável problema no provedor" } else { "" }
                ),
            )
        } else if best_avg > 100.0 {
            (Status::Info, format!("Latência alta até a internet: {best_avg:.0} ms."))
        } else {
            (Status::Ok, format!("Internet estável: {best_avg:.0} ms, sem perda relevante."))
        };
        checks.push(check(
            "net_internet",
            "Internet",
            status,
            summary,
            if status == Status::Ok {
                vec![]
            } else {
                vec![
                    "Reinicie o modem e o roteador.".into(),
                    "Se persistir com a rede local boa, abra um chamado no provedor com estes números.".into(),
                ]
            },
        ));
    }

    if raw.gateway.is_some() {
        let (status, summary) = match raw.dns_ms {
            None => (Status::Crit, "O DNS não respondeu: sites não abrem mesmo com internet.".to_string()),
            Some(v) if v > 300.0 => (Status::Info, format!("DNS lento: {v:.0} ms para resolver um nome.")),
            Some(v) => (Status::Ok, format!("DNS respondendo em {v:.0} ms.")),
        };
        let mut c = check(
            "net_dns",
            "DNS",
            status,
            summary,
            if status == Status::Ok {
                vec![]
            } else {
                vec!["Troque o DNS do adaptador para 1.1.1.1 e 8.8.8.8 (Configurações > Rede > Editar DNS).".into()]
            },
        );
        if !raw.dns_servers.is_empty() {
            c.details.push(format!("Servidores DNS: {}", raw.dns_servers.join(", ")));
        }
        checks.push(c);
    }

    if let Some(signal) = raw.wifi_signal {
        let status = if signal < 40 {
            Status::Warn
        } else if signal < 60 {
            Status::Info
        } else {
            Status::Ok
        };
        checks.push(check(
            "net_wifi",
            "Sinal do Wi-Fi",
            status,
            format!(
                "Sinal de {signal}%{}.",
                raw.ssid.as_deref().map(|s| format!(" na rede {s}")).unwrap_or_default()
            ),
            if status == Status::Ok {
                vec![]
            } else {
                vec!["Sinal fraco causa quedas e lentidão: aproxime-se do roteador ou use um repetidor/mesh.".into()]
            },
        ));
    }

    for c in &mut checks {
        if c.id == "net_router" || c.id == "net_internet" {
            c.details = raw
                .targets
                .iter()
                .map(|t| {
                    format!(
                        "{} ({}): {}/{} respostas, média {}, pico {}",
                        t.label,
                        t.host,
                        t.received,
                        t.sent,
                        ms(t.avg_ms),
                        ms(t.max_ms)
                    )
                })
                .collect();
        }
    }
    checks.sort_by_key(|c| std::cmp::Reverse(c.status));
    NetworkReport { raw, checks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping(label: &str, received: u32, avg: f64) -> PingResult {
        PingResult {
            host: "x".into(),
            label: label.into(),
            sent: 20,
            received,
            avg_ms: (received > 0).then_some(avg),
            max_ms: (received > 0).then_some(avg * 2.0),
        }
    }

    fn get<'a>(r: &'a NetworkReport, id: &str) -> &'a Check {
        r.checks.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn healthy_network() {
        let r = evaluate(NetworkRaw {
            gateway: Some("192.168.0.1".into()),
            targets: vec![ping("Roteador", 20, 2.0), ping("Internet (Cloudflare)", 20, 14.0)],
            dns_ms: Some(20.0),
            ..NetworkRaw::default()
        });
        assert!(r.checks.iter().all(|c| c.status == Status::Ok), "{:?}", r.checks);
    }

    #[test]
    fn no_route_is_critical() {
        let r = evaluate(NetworkRaw::default());
        assert_eq!(get(&r, "net_connection").status, Status::Crit);
        assert!(r.checks.iter().all(|c| c.id != "net_dns"));
    }

    #[test]
    fn isp_loss_with_clean_local_network_blames_the_provider() {
        let r = evaluate(NetworkRaw {
            gateway: Some("192.168.0.1".into()),
            targets: vec![ping("Roteador", 20, 2.0), ping("Internet (Google)", 17, 30.0)],
            dns_ms: Some(20.0),
            ..NetworkRaw::default()
        });
        let c = get(&r, "net_internet");
        assert_eq!(c.status, Status::Warn);
        assert!(c.summary.contains("provedor"), "{}", c.summary);
        assert_eq!(r.checks[0].id, "net_internet");
    }

    #[test]
    fn weak_wifi_and_lossy_router() {
        let r = evaluate(NetworkRaw {
            gateway: Some("192.168.0.1".into()),
            wifi: true,
            wifi_signal: Some(31),
            targets: vec![ping("Roteador", 16, 9.0), ping("Internet (Google)", 16, 40.0)],
            dns_ms: None,
            ..NetworkRaw::default()
        });
        assert_eq!(get(&r, "net_router").status, Status::Crit);
        assert!(get(&r, "net_router").actions[0].contains("5 GHz"));
        assert_eq!(get(&r, "net_wifi").status, Status::Warn);
        assert_eq!(get(&r, "net_dns").status, Status::Crit);
    }
}
