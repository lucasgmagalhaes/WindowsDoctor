//! Reparos do Windows com progresso: SFC, DISM, CHKDSK e teste de memória.

use std::io::Read;
use std::process::Stdio;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sfc,
    Dism,
    Chkdsk,
    Memory,
    /// `compact /CompactOS:always`: comprime os arquivos do Windows.
    CompactOs,
    /// `compact /CompactOS:never`.
    UncompactOs,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "sfc" => Kind::Sfc,
            "dism" => Kind::Dism,
            "chkdsk" => Kind::Chkdsk,
            "memory" => Kind::Memory,
            "compactos" => Kind::CompactOs,
            "uncompactos" => Kind::UncompactOs,
            _ => return None,
        })
    }

    pub fn title(self) -> &'static str {
        match self {
            Kind::Sfc => "Verificar arquivos do sistema (SFC)",
            Kind::Dism => "Reparar a imagem do Windows (DISM)",
            Kind::Chkdsk => "Verificar o disco do sistema (CHKDSK)",
            Kind::Memory => "Teste de memória do Windows",
            Kind::CompactOs => "Compactar o Windows (CompactOS)",
            Kind::UncompactOs => "Descompactar o Windows (CompactOS)",
        }
    }

    fn command(self) -> (&'static str, Vec<String>) {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        match self {
            Kind::Sfc => ("sfc.exe", vec!["/scannow".into()]),
            Kind::Dism => (
                "DISM.exe",
                ["/Online", "/Cleanup-Image", "/RestoreHealth"].map(String::from).to_vec(),
            ),
            Kind::Chkdsk => ("chkdsk.exe", vec![drive, "/scan".into()]),
            // Abre a janela do Windows que agenda o teste para o próximo reinício.
            Kind::Memory => ("mdsched.exe", vec![]),
            Kind::CompactOs => ("compact.exe", vec!["/CompactOS:always".into()]),
            Kind::UncompactOs => ("compact.exe", vec!["/CompactOS:never".into()]),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub line: String,
    pub percent: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub kind: String,
    pub title: String,
    pub exit_code: Option<i32>,
    pub ok: bool,
    /// Últimas linhas da saída, que trazem o resultado.
    pub tail: Vec<String>,
}

/// O SFC escreve em UTF-16 quando a saída é redirecionada; o DISM e o CHKDSK
/// usam a página de código OEM. Decide pelo conteúdo.
pub fn decode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    if bytes.len() >= 4 && zeros * 3 > bytes.len() / 2 {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units).replace('\u{feff}', "");
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|b| cp850(*b)).collect(),
    }
}

/// Página 850 (OEM do Windows em português) para as letras acentuadas mais comuns.
fn cp850(b: u8) -> char {
    match b {
        0x80 => 'Ç', 0x81 => 'ü', 0x82 => 'é', 0x83 => 'â', 0x84 => 'ä', 0x85 => 'à', 0x87 => 'ç',
        0x88 => 'ê', 0x8A => 'è', 0x8C => 'î', 0x90 => 'É', 0x93 => 'ô', 0x94 => 'ö', 0x96 => 'û',
        0xA0 => 'á', 0xA1 => 'í', 0xA2 => 'ó', 0xA3 => 'ú', 0xA4 => 'ñ', 0xB5 => 'Á', 0xB6 => 'Â',
        0xB7 => 'À', 0xC6 => 'ã', 0xC7 => 'Ã', 0xD2 => 'Ê', 0xE0 => 'Ó', 0xE2 => 'Ô', 0xE4 => 'õ',
        0xE5 => 'Õ', 0xE9 => 'Ú', 0xD6 => 'Í',
        b if b < 0x80 => b as char,
        _ => '?',
    }
}

/// "Verificação 45% concluída", "[====  42.3%  ]" → 45 / 42.3.
pub fn percent(line: &str) -> Option<f32> {
    let i = line.find('%')?;
    let num: String = line[..i]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let v: f32 = num.replace(',', ".").trim_matches('.').parse().ok()?;
    (0.0..=100.0).contains(&v).then_some(v)
}

/// Roda o reparo e chama `on_progress` a cada linha (ou atualização de
/// porcentagem). Exige administrador, menos o teste de memória.
pub fn run(kind: Kind, mut on_progress: impl FnMut(Progress)) -> Result<Outcome, String> {
    if !cfg!(windows) {
        return Err(crate::WINDOWS_ONLY.into());
    }
    let (program, args) = kind.command();
    if kind == Kind::Memory {
        crate::shell::command(program)
            .args(&args)
            .spawn()
            .map_err(|e| format!("falha ao abrir {program}: {e}"))?;
        return Ok(Outcome {
            kind: "memory".into(),
            title: kind.title().into(),
            exit_code: Some(0),
            ok: true,
            tail: vec!["Escolha na janela do Windows quando reiniciar; o resultado aparece depois do login.".into()],
        });
    }
    let mut child = crate::shell::command(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("falha ao executar {program}: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("sem saída")?;
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    let mut emitted = 0usize;
    let mut last_line = String::new();
    loop {
        let n = stdout.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        all.extend_from_slice(&buf[..n]);
        // Decodifica tudo de novo (barato) e emite as linhas novas.
        let text = decode(&all);
        let pieces: Vec<&str> = text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect();
        // A última pode estar incompleta: espera a próxima leitura.
        let complete = pieces.len().saturating_sub(1);
        for line in pieces.iter().take(complete).skip(emitted) {
            if *line != last_line {
                on_progress(Progress {
                    line: line.to_string(),
                    percent: percent(line),
                });
                last_line = line.to_string();
            }
        }
        emitted = emitted.max(complete);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let text = decode(&all);
    let lines: Vec<String> = text
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|l| !l.is_empty() && percent(l).is_none())
        .map(str::to_string)
        .collect();
    let tail = lines.iter().rev().take(6).rev().cloned().collect();
    Ok(Outcome {
        kind: format!("{kind:?}").to_lowercase(),
        title: kind.title().into(),
        exit_code: status.code(),
        ok: status.success(),
        tail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf16_sfc_output() {
        let text = "Verificação 45% concluída.\r\n";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&bytes), text);
    }

    #[test]
    fn decodes_oem_dism_output() {
        // "Operação concluída" em CP850.
        let bytes = b"Opera\x87\xC6o conclu\xA1da";
        assert_eq!(decode(bytes), "Operação concluída");
        assert_eq!(decode("já é UTF-8".as_bytes()), "já é UTF-8");
    }

    #[test]
    fn reads_percentages() {
        assert_eq!(percent("Verification 45% complete."), Some(45.0));
        assert_eq!(percent("[=====      42.3%       ]"), Some(42.3));
        assert_eq!(percent("[====  10,0%  ]"), Some(10.0));
        assert_eq!(percent("sem porcentagem"), None);
        assert_eq!(percent("100%"), Some(100.0));
    }

    #[test]
    fn kinds_round_trip() {
        for k in ["sfc", "dism", "chkdsk", "memory", "compactos", "uncompactos"] {
            let kind = Kind::parse(k).unwrap();
            assert_eq!(format!("{kind:?}").to_lowercase(), k);
        }
        assert!(Kind::parse("rm").is_none());
    }
}
