//! Pontos de restauração do sistema (Restauração do Sistema do Windows).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "message")]
pub enum Outcome {
    Created,
    /// O Windows só cria um ponto a cada 24 horas por padrão; já existe um recente.
    Recent,
    /// Precisa de administrador.
    NotAdmin,
    Failed(String),
}

/// Cria um ponto de restauração com a descrição dada. Pode levar até um minuto.
pub fn create(description: &str, elevated: bool) -> Outcome {
    if !cfg!(windows) {
        return Outcome::Failed(crate::WINDOWS_ONLY.into());
    }
    if !elevated {
        return Outcome::NotAdmin;
    }
    let desc = description.replace('\'', "''");
    let script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8\n\
         try {{\n\
           Checkpoint-Computer -Description '{desc}' -RestorePointType MODIFY_SETTINGS \
             -WarningVariable w -WarningAction SilentlyContinue -ErrorAction Stop\n\
           if ($w) {{ 'recent' }} else {{ 'created' }}\n\
         }} catch {{ 'error: ' + $_.Exception.Message }}"
    );
    match crate::shell::powershell(&script) {
        Ok(out) => parse(&out),
        Err(e) => Outcome::Failed(e),
    }
}

fn parse(out: &str) -> Outcome {
    let out = out.trim();
    match out {
        "created" => Outcome::Created,
        "recent" => Outcome::Recent,
        _ => {
            let msg = out.strip_prefix("error: ").unwrap_or(out).trim();
            Outcome::Failed(if msg.is_empty() {
                "o Windows não respondeu".into()
            } else {
                format!(
                    "{msg} (a Proteção do Sistema pode estar desligada: Painel de Controle > Sistema > Proteção do Sistema)"
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_output() {
        assert_eq!(parse("created\r\n"), Outcome::Created);
        assert_eq!(parse("recent"), Outcome::Recent);
        match parse("error: O serviço não pode ser iniciado") {
            Outcome::Failed(m) => assert!(m.starts_with("O serviço não pode ser iniciado (")),
            o => panic!("{o:?}"),
        }
        assert!(matches!(parse(""), Outcome::Failed(_)));
    }

    #[test]
    fn serializes_for_the_frontend() {
        let j = serde_json::to_string(&Outcome::Failed("x".into())).unwrap();
        assert_eq!(j, r#"{"kind":"failed","message":"x"}"#);
        assert_eq!(serde_json::to_string(&Outcome::Recent).unwrap(), r#"{"kind":"recent"}"#);
    }
}
