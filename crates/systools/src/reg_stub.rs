//! Substituto de `reg.rs` fora do Windows: tudo falha com [`crate::WINDOWS_ONLY`].
#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hive {
    CurrentUser,
    LocalMachine,
}

fn no<T>() -> Result<T, String> {
    Err(crate::WINDOWS_ONLY.into())
}

pub(crate) fn get_dword(_: Hive, _: &str, _: &str) -> Result<Option<u32>, String> {
    no()
}
pub(crate) fn set_dword(_: Hive, _: &str, _: &str, _: u32) -> Result<(), String> {
    no()
}
pub(crate) fn delete_value(_: Hive, _: &str, _: &str) -> Result<(), String> {
    no()
}
pub(crate) fn get_binary(_: Hive, _: &str, _: &str) -> Result<Option<Vec<u8>>, String> {
    no()
}
pub(crate) fn set_binary(_: Hive, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
    no()
}
pub(crate) fn string_values(_: Hive, _: &str) -> Result<Vec<(String, String)>, String> {
    no()
}
pub(crate) fn can_write_machine() -> bool {
    false
}
