//! Otimizações reversíveis. Cada uma lê o estado atual do sistema, então a
//! tela sempre mostra a verdade, mesmo se a alteração foi feita por fora.

use serde::Serialize;

use crate::reg::{self, Hive};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TweakInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub needs_admin: bool,
    pub needs_restart: bool,
    /// `None` quando o estado não pôde ser lido (veja `error`).
    pub applied: Option<bool>,
    pub error: Option<String>,
}

struct Tweak {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    category: &'static str,
    needs_admin: bool,
    needs_restart: bool,
    state: fn() -> Result<bool, String>,
    set: fn(bool) -> Result<(), String>,
}

impl Tweak {
    fn info(&self) -> TweakInfo {
        let (applied, error) = match (self.state)() {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        TweakInfo {
            id: self.id,
            title: self.title,
            description: self.description,
            category: self.category,
            needs_admin: self.needs_admin,
            needs_restart: self.needs_restart,
            applied,
            error,
        }
    }
}

pub fn list() -> Vec<TweakInfo> {
    all().iter().map(Tweak::info).collect()
}

/// Aplica (`apply = true`) ou desfaz a otimização e devolve o novo estado.
pub fn set(id: &str, apply: bool) -> Result<TweakInfo, String> {
    let all = all();
    let t = all
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("otimização desconhecida: {id}"))?;
    (t.set)(apply)?;
    Ok(t.info())
}

fn all() -> Vec<Tweak> {
    vec![
        Tweak {
            id: "disable_mpo",
            title: "Desativar Multiplane Overlay (MPO)",
            description: "Corrige piscadas e telas pretas rápidas que acontecem em muitas placas NVIDIA, \
                          principalmente com dois monitores ou taxas de atualização diferentes. \
                          Custo: um pouco mais de uso da GPU em vídeos.",
            category: "Vídeo",
            needs_admin: true,
            needs_restart: true,
            state: || Ok(reg::get_dword(Hive::LocalMachine, DWM, "OverlayTestMode")? == Some(5)),
            set: |on| {
                if on {
                    reg::set_dword(Hive::LocalMachine, DWM, "OverlayTestMode", 5)
                } else {
                    reg::delete_value(Hive::LocalMachine, DWM, "OverlayTestMode")
                }
            },
        },
        Tweak {
            id: "disable_fast_startup",
            title: "Desativar Inicialização Rápida",
            description: "Com ela ligada, \"Desligar\" hiberna o núcleo do Windows em vez de desligar de verdade, \
                          o que mantém drivers com problema carregados e pode causar falhas ao ligar. \
                          O boot fica alguns segundos mais lento.",
            category: "Sistema",
            needs_admin: true,
            needs_restart: false,
            state: || Ok(reg::get_dword(Hive::LocalMachine, SESSION_POWER, "HiberbootEnabled")? == Some(0)),
            set: |on| reg::set_dword(Hive::LocalMachine, SESSION_POWER, "HiberbootEnabled", u32::from(!on)),
        },
        Tweak {
            id: "high_performance_plan",
            title: "Plano de energia Alto Desempenho",
            description: "Mantém o processador em frequências altas e reduz atrasos ao sair do repouso. \
                          Aumenta o consumo e o calor; em notebooks, prefira só na tomada. \
                          Desfazer volta para o plano Equilibrado.",
            category: "Energia",
            needs_admin: false,
            needs_restart: false,
            state: || {
                let out = powercfg(&["/getactivescheme"])?.to_lowercase();
                Ok(out.contains(HIGH_PERFORMANCE) || out.contains(ULTIMATE_PERFORMANCE))
            },
            set: |on| {
                if !on {
                    return powercfg(&["/setactive", BALANCED]).map(|_| ());
                }
                if powercfg(&["/setactive", HIGH_PERFORMANCE]).is_ok() {
                    return Ok(());
                }
                // Em PCs com Modern Standby o plano fica escondido: cria uma cópia.
                let out = powercfg(&["/duplicatescheme", HIGH_PERFORMANCE])?;
                let guid = find_guid(&out).ok_or("o Windows não criou o plano Alto Desempenho")?;
                powercfg(&["/setactive", &guid]).map(|_| ())
            },
        },
        Tweak {
            id: "pcie_aspm_off",
            title: "Desligar economia de energia do PCI Express",
            description: "Impede que o link com a placa de vídeo e o SSD NVMe entre em economia. \
                          Ajuda em telas pretas, travadinhas e quedas da placa de vídeo. \
                          Vale para o plano de energia atual, na tomada.",
            category: "Energia",
            needs_admin: false,
            needs_restart: false,
            state: || Ok(power_value(SUB_PCIEXPRESS, ASPM)? == 0),
            set: |on| set_power_value(SUB_PCIEXPRESS, ASPM, if on { 0 } else { 1 }),
        },
        Tweak {
            id: "disable_usb_selective_suspend",
            title: "Desligar suspensão seletiva de USB",
            description: "Evita que o Windows desligue portas USB para economizar energia. Ajuda quando \
                          mouse, headset ou o hub USB do monitor desconectam sozinhos (e é necessário \
                          para algumas ferramentas de firmware). Vale para o plano atual, na tomada.",
            category: "Energia",
            needs_admin: false,
            needs_restart: false,
            state: || Ok(power_value(SUB_USB, USB_SELECTIVE_SUSPEND)? == 0),
            set: |on| set_power_value(SUB_USB, USB_SELECTIVE_SUSPEND, if on { 0 } else { 1 }),
        },
        Tweak {
            id: "game_mode",
            title: "Modo de Jogo",
            description: "O Windows prioriza o jogo aberto e adia atualizações e notificações enquanto você joga.",
            category: "Jogos",
            needs_admin: false,
            needs_restart: false,
            state: || Ok(reg::get_dword(Hive::CurrentUser, GAME_BAR, "AutoGameModeEnabled")? != Some(0)),
            set: |on| reg::set_dword(Hive::CurrentUser, GAME_BAR, "AutoGameModeEnabled", u32::from(on)),
        },
        Tweak {
            id: "disable_game_dvr",
            title: "Desativar gravação em segundo plano (Game DVR)",
            description: "A Xbox Game Bar pode gravar o jogo o tempo todo para os \"clipes\", o que custa \
                          desempenho. Desative se não usa esse recurso.",
            category: "Jogos",
            needs_admin: false,
            needs_restart: false,
            state: || Ok(reg::get_dword(Hive::CurrentUser, GAME_CONFIG, "GameDVR_Enabled")? == Some(0)),
            set: |on| {
                let v = u32::from(!on);
                reg::set_dword(Hive::CurrentUser, GAME_CONFIG, "GameDVR_Enabled", v)?;
                reg::set_dword(Hive::CurrentUser, GAME_DVR, "AppCaptureEnabled", v)
            },
        },
        Tweak {
            id: "disable_hibernation",
            title: "Desativar hibernação",
            description: "Apaga o hiberfil.sys e libera espaço (cerca de 40% da memória RAM). \
                          Também desliga a Inicialização Rápida. Em notebooks você perde a opção Hibernar.",
            category: "Armazenamento",
            needs_admin: true,
            needs_restart: false,
            state: || Ok(reg::get_dword(Hive::LocalMachine, CONTROL_POWER, "HibernateEnabled")? == Some(0)),
            set: |on| powercfg(&["/hibernate", if on { "off" } else { "on" }]).map(|_| ()),
        },
    ]
}

const DWM: &str = r"SOFTWARE\Microsoft\Windows\Dwm";
const SESSION_POWER: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Power";
const CONTROL_POWER: &str = r"SYSTEM\CurrentControlSet\Control\Power";
const GAME_BAR: &str = r"Software\Microsoft\GameBar";
const GAME_CONFIG: &str = r"System\GameConfigStore";
const GAME_DVR: &str = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";

const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE_PERFORMANCE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
const SUB_PCIEXPRESS: &str = "SUB_PCIEXPRESS";
const ASPM: &str = "ASPM";
const SUB_USB: &str = "2a737441-1930-4402-8d77-b2bebba308a3";
const USB_SELECTIVE_SUSPEND: &str = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226";

fn powercfg(args: &[&str]) -> Result<String, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    crate::shell::run("powercfg.exe", args)
}

/// Valor atual (na tomada) de uma configuração do plano de energia ativo.
fn power_value(sub: &str, setting: &str) -> Result<u32, String> {
    let out = powercfg(&["/query", "SCHEME_CURRENT", sub, setting])?;
    parse_ac_index(&out).ok_or_else(|| "configuração de energia não encontrada neste PC".into())
}

fn set_power_value(sub: &str, setting: &str, value: u32) -> Result<(), String> {
    let v = value.to_string();
    powercfg(&["/setacvalueindex", "SCHEME_CURRENT", sub, setting, &v])?;
    powercfg(&["/setactive", "SCHEME_CURRENT"]).map(|_| ())
}

/// Na saída de `powercfg /query`, os únicos números `0x...` são os índices
/// atuais: primeiro o da tomada (AC), depois o da bateria (DC). Os textos ao
/// redor mudam com o idioma do Windows, os números não.
fn parse_ac_index(out: &str) -> Option<u32> {
    out.split_whitespace()
        .find_map(|tok| tok.strip_prefix("0x"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
}

fn find_guid(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .find(|t| {
            t.len() == 36 && t.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        })
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ac_index_from_any_language() {
        let en = "Power Setting GUID: ee12f906-d277-404b-b6da-e5fa1a576df5  (Link State Power Management)\n\
                  GUID Alias: ASPM\n  Possible Setting Index: 000\n  Possible Setting Friendly Name: Off\n\
                  Current AC Power Setting Index: 0x00000002\n  Current DC Power Setting Index: 0x00000001\n";
        assert_eq!(parse_ac_index(en), Some(2));
        let pt = "Índice de Configuração de Energia CA Atual: 0x00000000\n\
                  Índice de Configuração de Energia CC Atual: 0x00000002";
        assert_eq!(parse_ac_index(pt), Some(0));
        assert_eq!(parse_ac_index("nada aqui"), None);
    }

    #[test]
    fn finds_guid_in_duplicatescheme_output() {
        let out = "Power Scheme GUID: 5E7A8B2C-1111-2222-3333-444455556666  (High performance)";
        assert_eq!(find_guid(out).as_deref(), Some("5e7a8b2c-1111-2222-3333-444455556666"));
        assert_eq!(find_guid("sem guid"), None);
    }

    #[test]
    fn ids_are_unique_and_known_to_diagnostics() {
        let ids: Vec<_> = all().iter().map(|t| t.id).collect();
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len());
        // Ids citados pelas verificações do diagnóstico.
        for id in ["disable_mpo", "disable_fast_startup"] {
            assert!(ids.contains(&id), "{id}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn reports_unavailable_outside_windows() {
        let list = list();
        assert!(list.iter().all(|t| t.applied.is_none() && t.error.is_some()));
        assert!(set("disable_mpo", true).is_err());
        assert!(set("nao_existe", true).unwrap_err().contains("desconhecida"));
    }
}
