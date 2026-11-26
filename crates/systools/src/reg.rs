//! Acesso ao Registro do Windows com mensagens de erro em português.

use std::io;

use winreg::enums::{KEY_READ, KEY_SET_VALUE, REG_BINARY};
use winreg::{RegKey, RegValue, HKCU, HKLM};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    fn key(self) -> &'static RegKey {
        match self {
            Hive::CurrentUser => HKCU,
            Hive::LocalMachine => HKLM,
        }
    }
}

pub(crate) fn describe(e: &io::Error) -> String {
    if e.kind() == io::ErrorKind::PermissionDenied {
        "acesso negado: execute o app como administrador".into()
    } else {
        e.to_string()
    }
}

fn open_read(hive: Hive, path: &str) -> io::Result<Option<RegKey>> {
    match hive.key().open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => Ok(Some(k)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn open_write(hive: Hive, path: &str) -> Result<RegKey, String> {
    hive.key()
        .create_subkey(path)
        .map(|(k, _)| k)
        .map_err(|e| describe(&e))
}

/// Lê um DWORD; `None` se a chave ou o valor não existirem.
pub(crate) fn get_dword(hive: Hive, path: &str, name: &str) -> Result<Option<u32>, String> {
    let Some(key) = open_read(hive, path).map_err(|e| describe(&e))? else {
        return Ok(None);
    };
    match key.get_value::<u32, _>(name) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(describe(&e)),
    }
}

pub(crate) fn set_dword(hive: Hive, path: &str, name: &str, value: u32) -> Result<(), String> {
    open_write(hive, path)?
        .set_value(name, &value)
        .map_err(|e| describe(&e))
}

/// Apaga um valor; não é erro se ele já não existir.
pub(crate) fn delete_value(hive: Hive, path: &str, name: &str) -> Result<(), String> {
    let key = match hive.key().open_subkey_with_flags(path, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(describe(&e)),
    };
    match key.delete_value(name) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(describe(&e)),
        _ => Ok(()),
    }
}

pub(crate) fn get_binary(hive: Hive, path: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(key) = open_read(hive, path).map_err(|e| describe(&e))? else {
        return Ok(None);
    };
    match key.get_raw_value(name) {
        Ok(v) => Ok(Some(v.bytes.into_owned())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(describe(&e)),
    }
}

pub(crate) fn set_binary(hive: Hive, path: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
    let value = RegValue {
        vtype: REG_BINARY,
        bytes: bytes.to_vec().into(),
    };
    open_write(hive, path)?
        .set_raw_value(name, &value)
        .map_err(|e| describe(&e))
}

/// Valores de texto de uma chave (nome, conteúdo). Chave inexistente: lista vazia.
pub(crate) fn string_values(hive: Hive, path: &str) -> Result<Vec<(String, String)>, String> {
    let Some(key) = open_read(hive, path).map_err(|e| describe(&e))? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in key.enum_values() {
        let (name, _) = item.map_err(|e| describe(&e))?;
        if let Ok(text) = key.get_value::<String, _>(&name) {
            out.push((name, text));
        }
    }
    Ok(out)
}

