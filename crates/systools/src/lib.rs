//! Ferramentas do sistema usadas pelo app além do mapa de disco:
//!
//! - [`diagnostics`]: relatório do sistema e verificações (driver de vídeo,
//!   erros de hardware, disco, desligamentos inesperados...).
//! - [`tweaks`]: otimizações reversíveis (MPO, plano de energia, Game DVR...).
//! - [`startup`]: programas que iniciam com o Windows.
//!
//! A coleta de dados e as alterações só funcionam no Windows; nos outros
//! sistemas as funções devolvem erro ou listas vazias, o que permite compilar
//! e testar a lógica pura em qualquer lugar.

pub mod ddc;
pub mod diagnostics;
pub mod duplicates;
pub mod history;
pub mod monitor;
pub mod network;
pub mod repair;
pub mod restore;
pub mod schedule;
pub mod startup;
pub mod tweaks;

mod shell;
#[cfg(windows)]
mod reg;
#[cfg(not(windows))]
#[path = "reg_stub.rs"]
mod reg;

/// Mensagem padrão para operações que exigem o Windows.
pub(crate) const WINDOWS_ONLY: &str = "disponível apenas no Windows";
