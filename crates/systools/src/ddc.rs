//! Leitura (somente) das informações que o monitor informa por DDC/CI:
//! modelo, versão do firmware e versão do MCCS. Nenhum comando de escrita é
//! enviado ao monitor.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDdc {
    /// Descrição do Windows ("Generic PnP Monitor" etc.).
    pub description: String,
    /// `model(...)` da string de capacidades.
    pub model: Option<String>,
    /// VCP 0xC9 ("Display Firmware Level"), como "1.02".
    pub firmware: Option<String>,
    /// `mccs_ver(...)`, como "2.2".
    pub mccs_version: Option<String>,
    /// O monitor respondeu por DDC/CI.
    pub ddc: bool,
}

/// Valor de um campo `nome(valor)` da string de capacidades MCCS.
pub fn capability<'a>(caps: &'a str, key: &str) -> Option<&'a str> {
    let lower = caps.to_ascii_lowercase();
    let start = lower.find(&format!("{key}("))? + key.len() + 1;
    let end = caps[start..].find(')')? + start;
    let v = caps[start..end].trim();
    (!v.is_empty()).then_some(v)
}

/// VCP 0xC9: byte alto = versão, byte baixo = revisão.
pub fn firmware_from_vcp(value: u32) -> Option<String> {
    let (major, minor) = ((value >> 8) & 0xFF, value & 0xFF);
    (value != 0).then(|| format!("{major}.{minor:02}"))
}

/// Consulta todos os monitores conectados. Pode levar ~1 s por monitor.
pub fn read_all() -> Vec<MonitorDdc> {
    #[cfg(windows)]
    {
        imp::read_all()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Devices::Display::{
        CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, GetCapabilitiesStringLength,
        GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply,
        PHYSICAL_MONITOR,
    };
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        let list = &mut *(data as *mut Vec<HMONITOR>);
        list.push(h);
        1
    }

    pub fn read_all() -> Vec<MonitorDdc> {
        let mut handles: Vec<HMONITOR> = Vec::new();
        let mut out = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect),
                &mut handles as *mut _ as LPARAM,
            );
            for h in handles {
                let mut n = 0u32;
                if GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut n) == 0 || n == 0 {
                    continue;
                }
                let mut phys = vec![PHYSICAL_MONITOR::default(); n as usize];
                if GetPhysicalMonitorsFromHMONITOR(h, n, phys.as_mut_ptr()) == 0 {
                    continue;
                }
                for p in &phys {
                    out.push(read_one(p));
                }
                DestroyPhysicalMonitors(n, phys.as_ptr());
            }
        }
        out
    }

    unsafe fn read_one(p: &PHYSICAL_MONITOR) -> MonitorDdc {
        // A struct é "packed": copia os campos antes de usar.
        let desc = p.szPhysicalMonitorDescription;
        let handle = p.hPhysicalMonitor;
        let desc_len = desc.iter().position(|c| *c == 0).unwrap_or(desc.len());
        let mut m = MonitorDdc {
            description: String::from_utf16_lossy(&desc[..desc_len]),
            ..MonitorDdc::default()
        };
        let mut len = 0u32;
        if GetCapabilitiesStringLength(handle, &mut len) != 0 && len > 0 && len < 64 * 1024 {
            let mut buf = vec![0u8; len as usize];
            if CapabilitiesRequestAndCapabilitiesReply(handle, buf.as_mut_ptr(), len) != 0 {
                m.ddc = true;
                let caps = String::from_utf8_lossy(&buf);
                let caps = caps.trim_end_matches('\0');
                m.model = capability(caps, "model").map(str::to_string);
                m.mccs_version = capability(caps, "mccs_ver").map(str::to_string);
            }
        }
        let (mut current, mut max) = (0u32, 0u32);
        if GetVCPFeatureAndVCPFeatureReply(handle, 0xC9, std::ptr::null_mut(), &mut current, &mut max) != 0 {
            m.ddc = true;
            m.firmware = firmware_from_vcp(current);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_capability_fields() {
        let caps = "(prot(monitor)type(LCD)model(VG27AQ1A)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 C9)mccs_ver(2.2))";
        assert_eq!(capability(caps, "model"), Some("VG27AQ1A"));
        assert_eq!(capability(caps, "mccs_ver"), Some("2.2"));
        assert_eq!(capability(caps, "MODEL"), None, "a chave é passada em minúsculas");
        assert_eq!(capability("(Model(XG27))", "model"), Some("XG27"));
        assert_eq!(capability("(type(LCD))", "model"), None);
    }

    #[test]
    fn formats_firmware_level() {
        assert_eq!(firmware_from_vcp(0x0102).as_deref(), Some("1.02"));
        assert_eq!(firmware_from_vcp(0x0A00).as_deref(), Some("10.00"));
        assert_eq!(firmware_from_vcp(0), None);
    }
}
